use core::fmt;
use std::{collections::HashMap, panic};

use futures::{future::select_all, FutureExt};
use tokio::sync::{mpsc, watch};
use vec_collections::{AbstractVecMap, VecMap};

use crate::{
    node_graph::{InputId, NodeOutput},
    pipeline::{
        execution::{ConnectionHandle, InvalidationCause, InvalidationNotifier},
        PipelineExecutor,
    },
    view::{views::DynDataView, DataViewsState, ViewId},
};

use super::DynDataViewTask;

// MARK: ViewsExecutor

pub struct ViewsExecutor {
    runners: HashMap<ViewId, ViewTaskRunner>,
}

impl ViewsExecutor {
    pub fn new() -> Self {
        Self {
            runners: HashMap::new(),
        }
    }

    pub fn update(
        &mut self,
        views_state: &mut DataViewsState,
        pipeline_executor: &PipelineExecutor,
    ) {
        // Deleted views
        self.runners
            .retain(|id, _| views_state.views.contains_key(id));

        // New views
        for (view_id, view) in &mut views_state.views {
            if !self.runners.contains_key(view_id) {
                self.runners
                    .insert(*view_id, ViewTaskRunner::from_view(view.as_mut()));
            }
        }

        // Connections
        for (view_id, runner) in &mut self.runners {
            let view = views_state
                .views
                .get(view_id)
                .expect("Views should be synced");

            runner.sync_connections(view.as_ref(), pipeline_executor);
            runner.sync_view(view.as_ref());
        }
    }
}

impl fmt::Debug for ViewsExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewsExecutor")
            .field("runners", &self.runners.keys().collect::<Vec<_>>())
            .finish()
    }
}

// MARK: ViewTaskRunner

struct ViewTaskRunner {
    inputs: VecMap<[(InputId, NodeOutput); 4]>,
    control_tx: mpsc::UnboundedSender<ControlMsg>,
    sync_tx: watch::Sender<Box<dyn DynDataView>>,
}

impl ViewTaskRunner {
    pub fn from_view(view: &mut dyn DynDataView) -> Self {
        let task = view.create_view_task();

        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (sync_tx, sync_rx) = watch::channel(view.clone_boxed());

        tokio::spawn(
            RunningViewTask {
                task,
                control_rx,
                sync_rx,
                input_connections: Vec::new(),
                error_on_last_run: false,
            }
            .run(),
        );

        Self {
            inputs: VecMap::empty(),
            control_tx,
            sync_tx,
        }
    }

    pub fn sync_view(&mut self, view: &dyn DynDataView) {
        self.sync_tx.send_if_modified(|v| {
            if v.changed(view) {
                *v = view.clone_boxed();
                true
            } else {
                false
            }
        });
    }

    pub fn sync_connections(&mut self, view: &dyn DynDataView, pipeline: &PipelineExecutor) {
        let inputs = view.inputs();

        for (input_id, incoming) in inputs {
            let existing = self
                .inputs
                .iter()
                .find(|(id, _)| *id == input_id)
                .map(|(_, o)| *o);

            match (incoming, existing) {
                (i, e) if i == e => {
                    // Do nothing when equal
                }
                (Some(incoming), _) => {
                    self.connect_input(input_id, incoming, pipeline);
                }
                (None, _) => {
                    self.disconnect_input(input_id);
                }
            }
        }
    }

    pub fn disconnect_input(&mut self, input_id: InputId) {
        self.control_tx
            .send(ControlMsg::Disconnect(input_id))
            .expect("Task should be running");
        self.inputs.retain(|(id, _)| *id != input_id);
    }

    pub fn connect_input(
        &mut self,
        input_id: InputId,
        output: NodeOutput,
        pipeline: &PipelineExecutor,
    ) {
        // Find handle for output
        let Some(connection) = pipeline.get_output(output.node_id, output.output_id) else {
            eprintln!(
                "Failed to find connection for output with id {:?}",
                output.output_id
            );
            return;
        };

        // Send connect message to task
        self.control_tx
            .send(ControlMsg::Connect(input_id, connection.clone()))
            .expect("Task should be running");

        // Update input record
        self.inputs.insert(input_id, output);
    }
}

// MARK: RunningViewTask

struct RunningViewTask {
    task: Box<dyn DynDataViewTask>,
    control_rx: mpsc::UnboundedReceiver<ControlMsg>,
    sync_rx: watch::Receiver<Box<dyn DynDataView>>,
    input_connections: Vec<(InputId, InvalidationNotifier)>,
    error_on_last_run: bool,
}

impl RunningViewTask {
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                msg = self.control_rx.recv() => {
                    match msg {
                        Some(ControlMsg::Connect(input_id, mut connection)) => {
                            connection.reset_connection();
                            self.task.connect(input_id, &mut connection);

                            if connection.did_connect() {
                                self.input_connections.push((input_id, connection.get_invalidation_notifier()));
                                self.invalidate(InvalidationCause::Connected);
                            }
                        }
                        Some(ControlMsg::Disconnect(input_id)) => {
                            self.task.disconnect(input_id);
                            self.input_connections.retain(|(id, _)| *id != input_id);
                            self.invalidate(InvalidationCause::Disconnected);
                        }
                        None => break,
                    };
                }
                _ = self.sync_rx.changed() => {
                    self.task.sync_view(self.sync_rx.borrow().as_ref());
                    self.invalidate(InvalidationCause::Synced);
                }
                _ = Self::on_invalidation(&mut self.input_connections) => {
                    self.invalidate(InvalidationCause::InputInvalidated);
                }
                is_error = Self::run_task(self.error_on_last_run, self.task.as_mut()) => {
                    self.error_on_last_run = is_error;
                }
            }
        }
    }

    async fn run_task(error_on_last_run: bool, task: &mut dyn DynDataViewTask) -> bool {
        if error_on_last_run {
            let () = futures::future::pending().await;
        }

        let result = panic::AssertUnwindSafe(task.run()).catch_unwind().await;

        let is_error = !matches!(result, Ok(Ok(_)));

        match result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => eprintln!("Task failed: {:?}", e),
            Err(e) => eprintln!(
                "Task panicked: {}",
                if let Some(msg) = e.downcast_ref::<&'static str>() {
                    msg.to_string()
                } else if let Some(msg) = e.downcast_ref::<String>() {
                    msg.clone()
                } else {
                    format!("?{:?}", e)
                }
            ),
        }

        is_error
    }

    fn invalidate(&mut self, cause: InvalidationCause) {
        self.task.invalidate(cause);

        self.error_on_last_run = false;
    }

    async fn on_invalidation(notifiers: &mut Vec<(InputId, InvalidationNotifier)>) {
        // Cannot borrow self as mutable again at call site
        if !notifiers.is_empty() {
            let (is_channel_open, index, ..) =
                select_all(notifiers.iter_mut().map(|(_, n)| n.on_invalidate())).await;

            if !is_channel_open {
                // Partner dropped, already disconnected
                notifiers.remove(index);
            }
        } else {
            futures::future::pending().await
        }
    }
}

// MARK: ControlMsg

enum ControlMsg {
    Connect(InputId, ConnectionHandle),
    Disconnect(InputId),
}
