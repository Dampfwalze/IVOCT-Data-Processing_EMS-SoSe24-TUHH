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
    ConnectionHandle, DynNodeTask, InvalidationNotifier, Invalidator, NodeTask, NodeTaskBuilder,
    Request, TaskOutput,
};

#[derive(Debug)]
pub struct PipelineExecutor {
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
        for (node_id, node) in &pipeline.nodes {
            if !self.runners.contains_key(node_id) {
                self.runners.insert(
                    *node_id,
                    RwLock::new(NodeTaskRunner::from_node(node.as_ref())),
                );
            }
        }

        // Connections
        for (node_id, runner) in &self.runners {
            let node = pipeline.nodes.get(node_id).expect("Nodes should be synced");

            let mut runner = runner.write().unwrap();

            runner.sync_connections(node.as_ref(), &self.runners);
            runner.sync_node(node.as_ref());
        }
    }
}

struct NodeTaskRunner {
    output_handles: VecMap<[(OutputId, ConnectionHandle); 4]>,
    inputs: VecMap<[(InputId, NodeOutput); 4]>,
    control_tx: mpsc::UnboundedSender<ControlMsg>,
    sync_tx: watch::Sender<Box<dyn DynPipelineNode>>,
}

impl NodeTaskRunner {
    pub fn from_node(node: &dyn DynPipelineNode) -> Self {
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

struct RunningNodeTask {
    node_task: Box<dyn DynNodeTask>,
    control_rx: mpsc::UnboundedReceiver<ControlMsg>,
    sync_rx: watch::Receiver<Box<dyn DynPipelineNode>>,
    input_connections: Vec<(InputId, InvalidationNotifier)>,
    output_invalidator: Vec<Invalidator>,
}

impl RunningNodeTask {
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
                            }
                        }
                        Some(ControlMsg::Disconnect(input_id)) => {
                            self.node_task.disconnect(input_id);
                            self.input_connections.retain(|(id, _)| *id != input_id);
                        }
                        None => break,
                    };
                    self.invalidate();
                }
                _ = self.sync_rx.changed() => {
                    self.node_task.sync_node(self.sync_rx.borrow().as_ref());
                    self.invalidate();
                }
                _ = Self::on_invalidation(&mut self.input_connections) => {
                    self.invalidate();
                }
                _ = Self::run_task(self.node_task.as_mut()) => {
                }
            }
        }
    }

    async fn run_task(task: &mut dyn DynNodeTask) {
        let result = panic::AssertUnwindSafe(task.run()).catch_unwind().await;

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
    }

    fn invalidate(&mut self) {
        self.output_invalidator
            .iter()
            .for_each(Invalidator::invalidate);

        self.node_task.invalidate();
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
