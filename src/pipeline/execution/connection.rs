use std::{any::Any, sync::Arc};

use futures::future::BoxFuture;
use tokio::sync::{mpsc, watch};

#[derive(Debug)]
pub enum TaskInput<Req: Request> {
    Disconnected(Option<Req::Response>),
    Connected {
        request_tx: mpsc::Sender<Req>,
        response_rx: watch::Receiver<Option<Req::Response>>,
        default_value: Option<Req::Response>,
    },
}

#[derive(Debug)]
pub struct TaskOutput<Req: Request> {
    working_on: Option<Req>,
    request_rx: mpsc::Receiver<Req>,
    response_tx: watch::Sender<Option<Req::Response>>,
}

impl<Req: Request> TaskInput<Req> {
    /// Request data that is valid with respect to [Request::is_response_valid].
    ///
    /// If disconnected, return the default value, or [None] if not valid.
    ///
    /// If there is a valid response already available, return that.
    pub async fn request(&mut self, req: Req) -> Option<Req::Response> {
        match self {
            TaskInput::Disconnected(r) => match r {
                Some(r) if req.is_response_valid(r) => Some(r.clone()),
                _ => None,
            },
            TaskInput::Connected {
                request_tx,
                response_rx: data_rx,
                ..
            } => {
                if let Some(res) = data_rx.borrow_and_update().as_ref() {
                    if req.is_response_valid(res) {
                        // If available response is valid, return it
                        return Some(res.clone());
                    }
                }

                if request_tx.send(req.clone()).await.is_err() {
                    // Partner dropped
                    self.disconnect();
                    return None;
                }

                loop {
                    if data_rx.changed().await.is_err() {
                        // Partner dropped
                        self.disconnect();
                        break None;
                    }

                    if let Some(res) = data_rx.borrow().as_ref() {
                        if req.is_response_valid(res) {
                            break Some(res.clone());
                        }
                    } else {
                        // Invalidated.
                        // Maybe resend request?
                    }
                }
            }
        }
    }

    /// Disconnect this input.
    ///
    /// Input will transition into [TaskInput::Disconnected] state, when not
    /// already. Will keep the default value.
    pub fn disconnect(&mut self) {
        if let TaskInput::Connected { default_value, .. } = self {
            *self = TaskInput::Disconnected(default_value.take());
        }
    }

    /// Connect this input to a connection.
    ///
    /// If already connected, will replace the connection.
    pub fn connect(&mut self, connection: &mut ConnectionHandle) -> bool {
        let default_value = match self {
            TaskInput::Disconnected(r) => r.take(),
            TaskInput::Connected { default_value, .. } => default_value.take(),
        };

        let Some((request_tx, response_rx)) = connection.connection.get_channels() else {
            return false;
        };

        *self = TaskInput::Connected {
            request_tx,
            response_rx,
            default_value,
        };

        connection.did_connect = true;
        true
    }
}

impl<Req: Request> TaskOutput<Req> {
    /// Receive request.
    ///
    /// If there is a request already being worked on, return that.
    ///
    /// Skip requests that have already a valid response. (Multiple senders may
    /// send the same request. Should respond only once.)
    pub async fn receive(&mut self) -> Req {
        if let Some(ref req) = self.working_on {
            req.clone()
        } else {
            let req = loop {
                let req = self.request_rx.recv().await.expect("Should never close");

                let Some(r) = self.response_tx.borrow().clone() else {
                    break req;
                };

                if !req.is_response_valid(&r) {
                    break req;
                }
                // If available response is valid, skip to next request
            };

            self.working_on = Some(req.clone());

            req
        }
    }

    /// Respond to the request.
    ///
    /// The next call to [TaskOutput::receive] will not return the current
    /// request again.
    ///
    /// If there is no request to respond to, does nothing.
    pub fn respond(&mut self, response: Req::Response) {
        if self.working_on.is_none() {
            eprintln!(
                "No request to respond to. This can only happen if the owning task is in an invalid state."
            );
            return;
        }

        self.response_tx
            .send(Some(response))
            .expect("Should never close");

        self.working_on = None;
    }

    /// Invalidate the current response, if not already.
    pub fn invalidate(&mut self) {
        self.response_tx.send_if_modified(|v| match v {
            Some(_) => {
                *v = None;
                true
            }
            None => false,
        });
    }

    pub(super) fn get_invalidator(&self) -> Invalidator {
        let response_tx = self.response_tx.clone();
        Invalidator(Box::new(move || {
            response_tx.send_if_modified(|v| match v {
                Some(_) => {
                    *v = None;
                    true
                }
                None => false,
            });
        }))
    }
}

impl<Req: Request> Default for TaskInput<Req> {
    fn default() -> Self {
        TaskInput::Disconnected(None)
    }
}

pub trait Request: Send + Sync + Clone + 'static {
    type Response: Send + Sync + Clone + 'static;

    fn is_response_valid(&self, _response: &Self::Response) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct ConnectionHandle {
    connection: Arc<dyn _DynConnectionHandle>,
    did_connect: bool,
}

impl ConnectionHandle {
    pub fn new<Req: Request>() -> (Self, TaskOutput<Req>) {
        let (request_tx, request_rx) = mpsc::channel(3);
        let (response_tx, response_rx) = watch::channel(None);

        let connection = Arc::new(_SharedConnectionHandle {
            request_tx,
            response_rx,
        });

        (
            Self {
                connection: connection.clone(),
                did_connect: false,
            },
            TaskOutput {
                working_on: None,
                request_rx,
                response_tx,
            },
        )
    }

    pub(super) fn get_invalidation_notifier(&self) -> InvalidationNotifier {
        self.connection.get_invalidation_notifier()
    }

    pub(super) fn reset_connection(&mut self) {
        self.did_connect = false;
    }

    pub(super) fn did_connect(&self) -> bool {
        self.did_connect
    }
}

trait _DynConnectionHandle: Send + Sync + 'static {
    fn get_channels_any(&self) -> (&dyn Any, &dyn Any);

    fn get_invalidation_notifier(&self) -> InvalidationNotifier;
}

struct _SharedConnectionHandle<Req: Request> {
    request_tx: mpsc::Sender<Req>,
    response_rx: watch::Receiver<Option<Req::Response>>,
}

impl<Req: Request> _DynConnectionHandle for _SharedConnectionHandle<Req> {
    fn get_channels_any(&self) -> (&dyn Any, &dyn Any) {
        (&self.request_tx, &self.response_rx)
    }

    fn get_invalidation_notifier(&self) -> InvalidationNotifier {
        InvalidationNotifier(Box::new(InvalidationNotifierImpl {
            channel_rx: self.response_rx.clone(),
        }))
    }
}

trait _DynConnectionHandleExt: _DynConnectionHandle {
    fn get_channels<Req: Request>(
        &self,
    ) -> Option<(mpsc::Sender<Req>, watch::Receiver<Option<Req::Response>>)>;
}

impl<T: ?Sized + _DynConnectionHandle> _DynConnectionHandleExt for T {
    fn get_channels<Req: Request>(
        &self,
    ) -> Option<(mpsc::Sender<Req>, watch::Receiver<Option<Req::Response>>)> {
        let (request_tx, response_rx) = self.get_channels_any();

        let request_tx = request_tx.downcast_ref::<mpsc::Sender<Req>>()?;
        let response_rx = response_rx.downcast_ref::<watch::Receiver<Option<Req::Response>>>()?;

        Some((request_tx.clone(), response_rx.clone()))
    }
}

pub struct InvalidationNotifier(Box<dyn DynInvalidationNotifier>);

impl InvalidationNotifier {
    pub fn on_invalidate(&mut self) -> BoxFuture<'_, bool> {
        self.0.on_invalidate()
    }
}

trait DynInvalidationNotifier: Send + Sync {
    fn on_invalidate(&mut self) -> BoxFuture<'_, bool>;
}

struct InvalidationNotifierImpl<T: Send + Sync> {
    channel_rx: watch::Receiver<Option<T>>,
}

impl<T: Send + Sync> DynInvalidationNotifier for InvalidationNotifierImpl<T> {
    fn on_invalidate(&mut self) -> BoxFuture<'_, bool> {
        // Box::pin(async move { self.channel_rx.wait_for(Option::is_none).await.is_ok() })
        Box::pin(async move {
            loop {
                if self.channel_rx.changed().await.is_err() {
                    return false;
                }

                if self.channel_rx.borrow().is_none() {
                    return true;
                }
            }
        })
    }
}

pub struct Invalidator(Box<dyn Fn() + Send + Sync>);

impl Invalidator {
    pub fn invalidate(&self) {
        self.0()
    }
}
