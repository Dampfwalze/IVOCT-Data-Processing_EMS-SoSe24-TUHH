use core::fmt;
use std::{collections::HashMap, panic, sync::RwLock};

use futures::{future::select_all, FutureExt};
use tokio::sync::{mpsc, watch};
use vec_collections::{AbstractVecMap, VecMap};

use crate::{
    node_graph::{InputId, NodeId, NodeOutput, OutputId},
    pipeline::{
        nodes::{DynPipelineNode, PipelineNode},
        Pipeline,
    },
};

use super::{
    ConnectionHandle, DynNodeTask, InvalidationCause, InvalidationNotifier, Invalidator, NodeTask,
    NodeTaskBuilder, Request, TaskOutput,
};

// MARK: PipelineExecutor

/// Pipeline execution system.
///
/// For every node in the high level [Pipeline] description it creates a
/// [tokio::task], running an event loop. This task is referred to as a node
/// task. It further manages a node tasks connections to other tasks and syncing
/// it with its high level [PipelineNode], by sending commands about connection
/// changes and sync requests to the tasks event loop. Each node task is
/// described using the [NodeTask] trait. Implementers are required to listen to
/// data requests from other tasks. Each task handles only one request at a
/// time. The processing of a request might be canceled when the configuration
/// changes, or other tasks are invalidated, which this task depends on.
///
/// [Self::update] syncs the high level [Pipeline] description with the node
/// tasks.
///
/// There is no shared state between node tasks and [PipelineExecutor]. Syncing
/// only uses message channels from [tokio::sync] to communicate to node tasks.
#[derive(Debug)]
pub struct PipelineExecutor {
    /// Each runner corresponds to one node in the [Pipeline].
    runners: HashMap<NodeId, RwLock<NodeTaskRunner>>,
}

impl PipelineExecutor {
    pub fn new() -> Self {
        Self {
            runners: HashMap::new(),
        }
    }

    pub fn update(&mut self, pipeline: &mut Pipeline) {
        // Deleted nodes
        self.runners.retain(|id, _| pipeline.nodes.contains_key(id));

        // New nodes
        for (node_id, node) in &mut pipeline.nodes {
            if !self.runners.contains_key(node_id) {
                self.runners.insert(
                    *node_id,
                    RwLock::new(NodeTaskRunner::from_node(node.as_mut())),
                );
            }
        }

        // Connections and node sync
        for (node_id, runner) in &self.runners {
            let node = pipeline.nodes.get(node_id).expect("Nodes should be synced");

            let mut runner = runner.write().unwrap();

            runner.sync_connections(node.as_ref(), &self.runners);
            runner.sync_node(node.as_ref());
        }
    }

    pub fn get_output(&self, node_id: NodeId, output_id: OutputId) -> Option<ConnectionHandle> {
        self.runners
            .get(&node_id)
            .and_then(|r| r.read().unwrap().get_output(output_id))
    }

    pub fn clear(&mut self) {
        self.runners.clear();
    }
}

// MARK: NodeTaskRunner

/// Handle to a node task, holding information about the node task and all
/// channel ends to communicate to the node task.
struct NodeTaskRunner {
    output_handles: VecMap<[(OutputId, ConnectionHandle); 4]>,
    inputs: VecMap<[(InputId, NodeOutput); 4]>,
    control_tx: mpsc::UnboundedSender<ControlMsg>,
    sync_tx: watch::Sender<Box<dyn DynPipelineNode>>,
}

impl NodeTaskRunner {
    pub fn from_node(node: &mut dyn DynPipelineNode) -> Self {
        let (task, output_handles, invalidator) = node.create_node_task();

        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (sync_tx, sync_rx) = watch::channel(node.clone_boxed());

        tokio::spawn(
            RunningNodeTask {
                node_task: task,
                control_rx,
                sync_rx,
                input_connections: Vec::new(),
                output_invalidator: invalidator,
                error_on_last_run: false,
            }
            .run(),
        );

        Self {
            output_handles,
            inputs: VecMap::empty(),
            control_tx,
            sync_tx,
        }
    }

    pub fn get_output(&self, output_id: OutputId) -> Option<ConnectionHandle> {
        self.output_handles.get(&output_id).cloned()
    }

    pub fn sync_node(&mut self, node: &dyn DynPipelineNode) {
        self.sync_tx.send_if_modified(|v| {
            if v.changed(node) {
                *v = node.clone_boxed();
                true
            } else {
                false
            }
        });
    }

    pub fn sync_connections(
        &mut self,
        node: &dyn DynPipelineNode,
        runners: &HashMap<NodeId, RwLock<NodeTaskRunner>>,
    ) {
        let inputs = node.inputs();

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
                    self.connect_input(input_id, incoming, runners);
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
        runners: &HashMap<NodeId, RwLock<NodeTaskRunner>>,
    ) {
        // Find runner that we want to connect to
        let Some(out_runner) = runners.get(&output.node_id) else {
            eprintln!(
                "Failed to find runner for node with id {:?}",
                output.node_id
            );
            return;
        };

        // Get read lock on runner
        let out_runner = out_runner.read().unwrap();

        // Find creator for output
        let Some(connection) = out_runner.get_output(output.output_id) else {
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

impl fmt::Debug for NodeTaskRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeTaskRunner")
            .field(
                "output_handles",
                &self
                    .output_handles
                    .iter()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>(),
            )
            .field("inputs", &self.inputs)
            .finish()
    }
}

// MARK: RunningNodeTask

/// The actual task running. It runs an event loop to receive control and sync
/// commands from the [PipelineExecutor] and runs the [NodeTask]s own event
/// loop, which is responsible for receiving data requests and processing data.
struct RunningNodeTask {
    node_task: Box<dyn DynNodeTask>,
    control_rx: mpsc::UnboundedReceiver<ControlMsg>,
    sync_rx: watch::Receiver<Box<dyn DynPipelineNode>>,
    input_connections: Vec<(InputId, InvalidationNotifier)>,
    output_invalidator: Vec<Invalidator>,
    error_on_last_run: bool,
}

impl RunningNodeTask {
    /// Main entry point and event loop.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                msg = self.control_rx.recv() => {
                    match msg {
                        Some(ControlMsg::Connect(input_id, mut connection)) => {
                            connection.reset_connection();
                            self.node_task.connect(input_id, &mut connection);

                            if connection.did_connect() {
                                self.input_connections.push((input_id, connection.get_invalidation_notifier()));
                                self.invalidate(InvalidationCause::Connected(input_id));
                            }
                        }
                        Some(ControlMsg::Disconnect(input_id)) => {
                            self.node_task.disconnect(input_id);
                            self.input_connections.retain(|(id, _)| *id != input_id);
                            self.invalidate(InvalidationCause::Disconnected(input_id));
                        }
                        None => break,
                    };
                }
                _ = self.sync_rx.changed() => {
                    self.node_task.sync_node(self.sync_rx.borrow().as_ref());
                    self.invalidate(InvalidationCause::Synced);
                }
                input_id = Self::on_invalidation(&mut self.input_connections) => {
                    // An input got invalidated
                    self.invalidate(InvalidationCause::InputInvalidated(input_id));
                }
                is_error = Self::run_task(self.error_on_last_run, self.node_task.as_mut()) => {
                    self.error_on_last_run = is_error;
                }
            }
        }
    }

    /// Run the [NodeTask::run] method, additionally handling panics and errors.
    async fn run_task(error_on_last_run: bool, task: &mut dyn DynNodeTask) -> bool {
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

    /// Invalidates all outputs of this node task and itself.
    fn invalidate(&mut self, cause: InvalidationCause) {
        self.output_invalidator
            .iter()
            .for_each(Invalidator::invalidate);

        self.node_task.invalidate(cause);

        self.error_on_last_run = false;
    }

    /// Returned future completes when any input received an invalidation
    /// notice.
    async fn on_invalidation(notifiers: &mut Vec<(InputId, InvalidationNotifier)>) -> InputId {
        // Cannot borrow self as mutable again at call site

        if !notifiers.is_empty() {
            // Await all notifiers, stopping if any completes
            let (is_channel_open, index, ..) =
                select_all(notifiers.iter_mut().map(|(_, n)| n.on_invalidate())).await;

            let id = notifiers[index].0;

            if !is_channel_open {
                // Partner dropped, already disconnected
                notifiers.remove(index);
            }

            id
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

impl fmt::Debug for ControlMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlMsg::Connect(input_id, ..) => f.debug_tuple("Connect").field(input_id).finish(),
            ControlMsg::Disconnect(input_id) => {
                f.debug_tuple("Disconnect").field(input_id).finish()
            }
        }
    }
}

// MARK: NodeTaskBuilderImpl

pub struct NodeTaskBuilderImpl<T: PipelineNode> {
    connection_handles: VecMap<[(OutputId, ConnectionHandle); 4]>,
    output_invalidator: Vec<Invalidator>,
    task: Option<Box<dyn DynNodeTask>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: PipelineNode> NodeTaskBuilderImpl<T> {
    pub fn new() -> Self {
        Self {
            connection_handles: VecMap::empty(),
            task: None,
            output_invalidator: Vec::new(),
            _phantom: Default::default(),
        }
    }

    pub fn build(
        self,
    ) -> (
        Box<dyn DynNodeTask>,
        VecMap<[(OutputId, ConnectionHandle); 4]>,
        Vec<Invalidator>,
    ) {
        let task = self
            .task
            .expect("NodeTaskBuilder::task should be called before building");
        (task, self.connection_handles, self.output_invalidator)
    }
}

impl<T: PipelineNode> NodeTaskBuilder for NodeTaskBuilderImpl<T> {
    type PipelineNode = T;

    fn output<Req: Request>(
        &mut self,
        output_id: <Self::PipelineNode as PipelineNode>::OutputId,
    ) -> TaskOutput<Req> {
        let (handle, output) = ConnectionHandle::new();
        self.connection_handles.insert(output_id.into(), handle);
        self.output_invalidator.push(output.get_invalidator());
        output
    }

    fn task(
        &mut self,
        task: impl NodeTask<
            InputId = <Self::PipelineNode as PipelineNode>::InputId,
            PipelineNode = Self::PipelineNode,
        >,
    ) {
        self.task = Some(Box::new(task));
    }
}
