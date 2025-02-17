pub mod binary_input;
pub mod diameter;
pub mod filter;
pub mod follow_catheter;
pub mod follow_lumen;
pub mod generate_mesh;
pub mod output;
pub mod process_raw_m_scan;
pub mod remove_detector_defect;
pub mod segment_b_scans;

use core::fmt;
use std::any;

use vec_collections::VecMap;

use crate::{
    gui::node_graph::{DynEditNode, EditNode},
    node_graph::{InputId, NodeOutput, OutputId, TypeId},
};

use super::execution::{
    ConnectionHandle, DynNodeTask, Invalidator, NodeTaskBuilder, NodeTaskBuilderImpl,
};

/// Important types and traits for pipeline nodes.
#[allow(unused_imports)]
mod prelude {
    pub(crate) use crate::pipeline::{
        execution::{
            ConnectionHandle, InvalidationCause, NodeTask, NodeTaskBuilder, TaskInput, TaskOutput,
        },
        requests, PipelineDataType,
    };

    pub(crate) use super::{deserialize_node, DynPipelineNode, PipelineNode};

    pub(crate) use graph::*;

    pub(crate) mod graph {
        pub(crate) use crate::node_graph::{
            impl_enum_from_into_id_types, InputId, InputIdNone, InputIdSingle, NodeId, NodeInput,
            NodeOutput, OutputId, OutputIdNone, OutputIdSingle, TypeId,
        };
    }

    pub(crate) use serde::{Deserialize, Serialize};
}

/// Trait describing a node in a high level node graph.
pub trait PipelineNode: erased_serde::Serialize
    + fmt::Debug
    + EditNode<InputId = <Self as PipelineNode>::InputId, OutputId = <Self as PipelineNode>::OutputId>
    + Send
    + Sync
    + Clone
    + 'static
{
    /// The type to use as input id inside this node.
    type InputId: From<InputId> + Into<InputId>;
    /// The type to use as output id inside this node.
    type OutputId: From<OutputId> + Into<OutputId>;

    /// Unique string identifier.
    fn slug() -> &'static str;

    /// Return all input ids a node has and their connection state.
    fn inputs(&self)
        -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)>;

    /// Compare this node to other, returning whether they are different.
    /// Returning true triggers a sync and invalidation in the execution system.
    fn changed(&self, other: &Self) -> bool;

    /// Return which output to connect when a data view is requested. The
    /// returned type id is advertised to the data views to determine, which
    /// view fits the best.
    fn get_output_id_for_view_request(
        &self,
    ) -> Option<(<Self as PipelineNode>::OutputId, impl Into<TypeId>)> {
        None as Option<(_, TypeId)>
    }

    /// Creates the task that becomes part of the execution system and
    /// responsible for executing this node.
    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>);
}

/// Dynamic version of [PipelineNode]. This trait is implemented automatically
/// for all types implementing [PipelineNode] and can be used as a trait object.
#[typetag::serde(tag = "type")]
pub trait DynPipelineNode: DynEditNode + Send + Sync + 'static {
    fn as_edit_node_mut(&mut self) -> &mut dyn DynEditNode;

    fn as_any(&self) -> &dyn any::Any;

    fn as_any_mut(&mut self) -> &mut dyn any::Any;

    fn as_debug(&self) -> &dyn fmt::Debug;

    fn clone_boxed(&self) -> Box<dyn DynPipelineNode>;

    fn inputs(&self) -> Vec<(InputId, Option<NodeOutput>)>;

    fn changed(&self, other: &dyn DynPipelineNode) -> bool;

    fn get_output_for_view_request(&self) -> Option<(OutputId, TypeId)>;

    fn create_node_task(
        &mut self,
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

    fn as_any_mut(&mut self) -> &mut dyn any::Any {
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
        match other.as_any().downcast_ref::<T>() {
            Some(other) => self.changed(other),
            None => false,
        }
    }

    fn get_output_for_view_request(&self) -> Option<(OutputId, TypeId)> {
        self.get_output_id_for_view_request()
            .map(|(id, ty)| (id.into(), ty.into()))
    }

    fn create_node_task(
        &mut self,
    ) -> (
        Box<dyn DynNodeTask>,
        VecMap<[(OutputId, ConnectionHandle); 4]>,
        Vec<Invalidator>,
    ) {
        let mut builder = NodeTaskBuilderImpl::<T>::new();
        self.create_node_task(&mut builder);
        builder.build()
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        Self::slug()
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {}
}

/// Workaround for limitations in [typetag]. Use this for all nodes.
macro_rules! deserialize_node {
    ($ty:ty, $slug:expr) => {
        typetag::__private::inventory::submit! {
            <dyn DynPipelineNode>::typetag_register(
                $slug,
                (|deserializer| typetag::__private::Result::Ok(
                    typetag::__private::Box::new(
                        typetag::__private::erased_serde::deserialize::<$ty>(deserializer)?
                    ),
                )) as typetag::__private::DeserializeFn<<dyn DynPipelineNode as typetag::__private::Strictest>::Object>
            )
        }
    };
}

pub(crate) use deserialize_node;
