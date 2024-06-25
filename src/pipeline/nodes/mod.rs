mod input;
mod raw_processing;

pub use input::*;
pub use raw_processing::*;

use core::fmt;
use std::any;

use vec_collections::VecMap;

use crate::{
    gui::node_graph::{DynEditNode, EditNode},
    node_graph::{InputId, NodeOutput, OutputId},
};

use super::execution::{
    ConnectionHandle, DynNodeTask, Invalidator, NodeTaskBuilder, NodeTaskBuilderImpl,
};

pub trait PipelineNode: fmt::Debug
    + EditNode<InputId = <Self as PipelineNode>::InputId, OutputId = <Self as PipelineNode>::OutputId>
    + Send
    + Sync
    + Clone
    + 'static
{
    type InputId: From<InputId> + Into<InputId>;
    type OutputId: From<OutputId> + Into<OutputId>;

    fn inputs(&self)
        -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)>;

    fn changed(&self, other: &Self) -> bool;

    fn create_node_task(&self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>);
}

pub trait DynPipelineNode: DynEditNode + Send + Sync + 'static {
    fn as_edit_node_mut(&mut self) -> &mut dyn DynEditNode;

    fn as_any(&self) -> &dyn any::Any;

    fn as_debug(&self) -> &dyn fmt::Debug;

    fn clone_boxed(&self) -> Box<dyn DynPipelineNode>;

    fn inputs(&self) -> Vec<(InputId, Option<NodeOutput>)>;

    fn changed(&self, other: &dyn DynPipelineNode) -> bool;

    fn create_node_task(
        &self,
    ) -> (
        Box<dyn DynNodeTask>,
        VecMap<[(OutputId, ConnectionHandle); 4]>,
        Vec<Invalidator>,
    );
}

impl<T: PipelineNode> DynPipelineNode for T {
    fn as_edit_node_mut(&mut self) -> &mut dyn DynEditNode {
        self
    }

    fn as_any(&self) -> &dyn any::Any {
        self
    }

    fn as_debug(&self) -> &dyn fmt::Debug {
        self
    }

    fn clone_boxed(&self) -> Box<dyn DynPipelineNode> {
        Box::new(self.clone())
    }

    fn inputs(&self) -> Vec<(InputId, Option<NodeOutput>)> {
        self.inputs().map(|(id, conn)| (id.into(), conn)).collect()
    }

    fn changed(&self, other: &dyn DynPipelineNode) -> bool {
        if let Some(other) = other.as_any().downcast_ref::<T>() {
            self.changed(other)
        } else {
            false
        }
    }

    fn create_node_task(
        &self,
    ) -> (
        Box<dyn DynNodeTask>,
        VecMap<[(OutputId, ConnectionHandle); 4]>,
        Vec<Invalidator>,
    ) {
        let mut builder = NodeTaskBuilderImpl::<T>::new();
        self.create_node_task(&mut builder);
        builder.build()
    }
}
