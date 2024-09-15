mod connection;
mod executor;

pub use connection::*;
pub use executor::*;
use futures::{future::BoxFuture, Future};

use crate::node_graph::InputId;

use super::nodes::{DynPipelineNode, PipelineNode};

/// Trait defining a task for a node running in the execution system.
pub trait NodeTask: Send + Sync + 'static {
    type InputId: From<InputId> + Into<InputId>;
    type PipelineNode: PipelineNode;

    /// Sync this task with the corresponding node from the high level
    /// description.
    fn sync_node(&mut self, node: &Self::PipelineNode) {
        let _ = node;
    }

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle);

    fn disconnect(&mut self, input_id: Self::InputId);

    /// The configuration of this node has changed and any data generated
    /// becomes invalid.
    fn invalidate(&mut self, cause: InvalidationCause) {
        let _ = cause;
    }

    /// Listen for requests from this nodes outputs asynchronously.
    fn run(&mut self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Dynamic version of [NodeTask]. This trait is implemented automatically
/// for all types implementing [NodeTask] and can be used as a trait object.
pub trait DynNodeTask: Send + Sync {
    fn sync_node(&mut self, node: &dyn DynPipelineNode);

    fn connect(&mut self, input_id: InputId, input: &mut ConnectionHandle);

    fn disconnect(&mut self, input_id: InputId);

    fn invalidate(&mut self, cause: InvalidationCause);

    fn run(&mut self) -> BoxFuture<'_, anyhow::Result<()>>;
}

impl<T: NodeTask + Send + Sync> DynNodeTask for T {
    fn sync_node(&mut self, node: &dyn DynPipelineNode) {
        let node = node
            .as_any()
            .downcast_ref::<T::PipelineNode>()
            .expect("node should be of type T::PipelineNode");
        self.sync_node(node);
    }

    fn connect(&mut self, input_id: InputId, input: &mut ConnectionHandle) {
        self.connect(input_id.into(), input)
    }

    fn disconnect(&mut self, input_id: InputId) {
        self.disconnect(input_id.into())
    }

    fn invalidate(&mut self, cause: InvalidationCause) {
        self.invalidate(cause)
    }

    fn run(&mut self) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(self.run())
    }
}

pub enum InvalidationCause {
    Connected(InputId),
    Disconnected(InputId),
    Synced,
    InputInvalidated(InputId),
}

/// Passed to [PipelineNode::create_node_task]. Use [NodeTaskBuilder::output] to
/// create the outputs for your node task and [NodeTaskBuilder::task] to submit
/// your created task.
pub trait NodeTaskBuilder {
    type PipelineNode: PipelineNode;

    fn output<Req: Request>(
        &mut self,
        output_id: <Self::PipelineNode as PipelineNode>::OutputId,
    ) -> TaskOutput<Req>;

    fn task(
        &mut self,
        task: impl NodeTask<
            InputId = <Self::PipelineNode as PipelineNode>::InputId,
            PipelineNode = Self::PipelineNode,
        >,
    );
}
