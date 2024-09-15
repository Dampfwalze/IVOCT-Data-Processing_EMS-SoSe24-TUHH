use tokio::sync::watch;

/// A channel using a queue with specified capacity, where every new receiver
/// starts receiving the oldest value, that is still applicable.
pub fn channel<T: Clone>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = watch::channel(Queue::new(capacity));

    (Sender { tx }, Receiver { rx, pos: 0 })
}

#[derive(Debug, Clone)]
pub struct Sender<T: Clone> {
    tx: watch::Sender<Queue<T>>,
}

#[derive(Debug)]
pub struct Receiver<T: Clone> {
    rx: watch::Receiver<Queue<T>>,
    pos: usize,
}

impl<T: Clone> Sender<T> {
    pub fn send(&self, item: T) {
        self.tx.send_modify(|queue| {
            queue.push(item);
        });
    }

    pub fn subscribe(&self) -> Receiver<T> {
        Receiver {
            rx: self.tx.subscribe(),
            pos: 0,
        }
    }

    pub fn is_lagged(&self) -> bool {
        self.tx.borrow().tail > 0
    }
}

impl<T: Clone> Receiver<T> {
    pub async fn recv(&mut self) -> Result<T, error::RecvError> {
        let (result, tail) = {
            let queue = self.rx.borrow_and_update();

            (queue.get(self.pos), queue.tail)
        }; // Ensure queue is dropped before we await (for some reason, drop(queue) doesn't work here)

        match result {
            Ok(item) => {
                self.pos = self.pos.wrapping_add(1);
                Ok(item)
            }
            Err(error::GetError::TooOld) => {
                self.pos = tail;
                Err(error::RecvError::Lagged)
            }
            Err(error::GetError::TooNew) => {
                self.rx
                    .changed()
                    .await
                    .map_err(|_| error::RecvError::Closed)?;

                let queue = self.rx.borrow_and_update();

                let val = queue.get(self.pos).map_err(|e| match e {
                    error::GetError::TooOld => {
                        self.pos = queue.tail;
                        error::RecvError::Lagged
                    }
                    error::GetError::TooNew => unreachable!(),
                });
                drop(queue);

                self.pos = self.pos.wrapping_add(1);

                val
            }
        }
    }

    pub fn is_lagged(&self) -> bool {
        self.rx.borrow().tail > self.pos
    }
}

impl<T: Clone> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.clone(),
            pos: self.pos,
        }
    }
}

#[derive(Debug, Clone)]
struct Queue<T: Clone> {
    buffer: Box<[Option<T>]>,
    head: usize,
    tail: usize,
}

impl<T: Clone> Queue<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            buffer.push(None);
        }

        Self {
            buffer: buffer.into_boxed_slice(),
            head: 0,
            tail: 0,
        }
    }

    pub fn push(&mut self, item: T) {
        self.buffer[self.head % self.buffer.len()] = Some(item);
        self.head = self.head.wrapping_add(1);
        self.tail = self.tail.max(self.head - self.buffer.len().min(self.head));
    }

    pub fn get(&self, index: usize) -> Result<T, error::GetError> {
        if index >= self.head {
            return Err(error::GetError::TooNew);
        }

        if index < self.tail {
            return Err(error::GetError::TooOld);
        }

        Ok(self.buffer.as_ref()[index % self.buffer.len()]
            .clone()
            .expect("Should not be None"))
    }
}

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
    pub enum RecvError {
        #[error("The queue is lagging behind")]
        Lagged,
        #[error("The queue has been closed")]
        Closed,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
    pub(super) enum GetError {
        #[error("The index is too new")]
        TooNew,
        #[error("The index is too old")]
        TooOld,
    }
}

#[cfg(test)]
mod test {
    use futures::FutureExt;

    use super::*;

    #[tokio::test]
    async fn queue_test() {
        let mut queue = Queue::new(10);

        for i in 0..10 {
            queue.push(i);
        }

        for i in 0..10 {
            assert_eq!(queue.get(i), Ok(i));
        }

        assert_eq!(queue.get(10), Err(super::error::GetError::TooNew));

        queue.push(12);
        assert_eq!(queue.get(0), Err(super::error::GetError::TooOld));
    }

    #[tokio::test]
    async fn channel_test() {
        let (tx, mut rx) = channel(2);

        let mut rx2 = tx.subscribe();
        let mut rx3 = tx.subscribe();

        tx.send(1);

        let mut rx4 = rx.clone();

        assert_eq!(rx.recv().await, Ok(1));
        assert_eq!(rx4.recv().await, Ok(1));

        tx.send(2);

        assert_eq!(rx.recv().await, Ok(2));
        assert_eq!(rx2.recv().await, Ok(1));
        assert_eq!(rx2.recv().await, Ok(2));

        let mut rx5 = rx2.clone();

        tx.send(3);

        assert_eq!(rx.recv().await, Ok(3));
        assert_eq!(rx2.recv().await, Ok(3));
        assert_eq!(rx5.recv().await, Err(error::RecvError::Lagged));

        assert_eq!(rx3.recv().await, Err(error::RecvError::Lagged));
        assert_eq!(rx3.recv().await, Ok(2));

        assert!(rx.recv().now_or_never().is_none());

        let fut = rx.recv();

        tx.send(4);
        drop(tx);

        assert_eq!(fut.await, Ok(4));
        assert_eq!(rx3.recv().await, Ok(3));
        assert_eq!(rx3.recv().await, Ok(4));
    }

    #[tokio::test]
    async fn channel_sends_when_no_receiver() {
        let (tx, rx) = channel(3);

        drop(rx);

        tx.send(1);
        tx.send(2);
        tx.send(3);

        let mut rx = tx.subscribe();

        assert_eq!(rx.recv().await, Ok(1));
        assert_eq!(rx.recv().await, Ok(2));
        assert_eq!(rx.recv().await, Ok(3));
    }
}
