mod connection;
mod executor;

pub use connection::*;
pub use executor::*;
use futures::{future::BoxFuture, Future};

use crate::node_graph::InputId;

use super::nodes::{DynPipelineNode, PipelineNode};

pub trait NodeTask: Send + Sync + 'static {
    type InputId: From<InputId> + Into<InputId>;
    type PipelineNode: PipelineNode;

    fn sync_node(&mut self, _node: &Self::PipelineNode) {}

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle);

    fn disconnect(&mut self, input_id: Self::InputId);

    fn invalidate(&mut self) {}

    fn run(&mut self) -> impl Future<Output = ()> + Send;
}

pub trait DynNodeTask: Send + Sync {
    fn sync_node(&mut self, node: &dyn DynPipelineNode);

    fn connect(&mut self, input_id: InputId, input: &mut ConnectionHandle);

    fn disconnect(&mut self, input_id: InputId);

    fn invalidate(&mut self);

    fn run(&mut self) -> BoxFuture<'_, ()>;
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

    fn invalidate(&mut self) {
        self.invalidate()
    }

    fn run(&mut self) -> BoxFuture<'_, ()> {
        Box::pin(self.run())
    }
}

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
