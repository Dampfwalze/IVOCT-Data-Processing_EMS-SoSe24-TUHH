pub mod execution;
pub mod nodes;
pub mod requests;

pub use execution::PipelineExecutor;
use nodes::DynPipelineNode;

use core::fmt;
use std::collections::HashMap;

use crate::node_graph::{impl_enum_from_into_id_types, NodeId, TypeId};

pub enum PipelineDataType {
    RawMScan,
    DataVector,
    MScan,
}

impl_enum_from_into_id_types!(PipelineDataType, [TypeId], {
    0 => RawMScan,
    1 => DataVector,
    2 => MScan,
});

pub struct Pipeline {
    pub nodes: HashMap<NodeId, Box<dyn DynPipelineNode>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }
}

impl fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Helper<'a>(&'a Pipeline);

        impl fmt::Debug for Helper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_map()
                    .entries(self.0.nodes.iter().map(|(id, node)| (id, node.as_debug())))
                    .finish()
            }
        }

        f.debug_struct("Pipeline")
            .field("nodes", &Helper(self))
            .finish()
    }
}
