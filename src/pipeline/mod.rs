pub mod execution;
pub mod nodes;
pub mod presets;
pub mod requests;
pub mod types;

pub use execution::PipelineExecutor;
use nodes::DynPipelineNode;
use serde::{Deserialize, Serialize};

use core::fmt;
use std::{
    collections::HashMap,
    ops::{Index, IndexMut},
};

use crate::node_graph::{impl_enum_from_into_id_types, NodeId, TypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineDataType {
    RawMScan,
    DataVector,
    MScan,
    BScanSegmentation,
}

impl_enum_from_into_id_types!(PipelineDataType, [TypeId], {
    0 => RawMScan,
    1 => DataVector,
    2 => MScan,
    3 => BScanSegmentation,
});

impl PipelineDataType {
    pub const VALUES: [PipelineDataType; 4] = [
        PipelineDataType::RawMScan,
        PipelineDataType::DataVector,
        PipelineDataType::MScan,
        PipelineDataType::BScanSegmentation,
    ];
}

#[derive(Serialize, Deserialize)]
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

impl Index<NodeId> for Pipeline {
    type Output = dyn DynPipelineNode;

    fn index(&self, index: NodeId) -> &Self::Output {
        self.nodes[&index].as_ref()
    }
}

impl IndexMut<NodeId> for Pipeline {
    fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
        self.nodes.get_mut(&index).unwrap().as_mut()
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
