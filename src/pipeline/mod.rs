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

/// Enum defining all high level data types that are used in the pipeline
/// description, to determine if pins are able to connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineDataType {
    RawMScan,
    DataVector,
    MScan,
    BScanSegmentation,
    MScanSegmentation,
    Diameter,
    Mesh,
}

impl_enum_from_into_id_types!(PipelineDataType, [TypeId], {
    0 => RawMScan,
    1 => DataVector,
    2 => MScan,
    3 => BScanSegmentation,
    4 => MScanSegmentation,
    5 => Diameter,
    6 => Mesh,
});

impl PipelineDataType {
    pub const VALUES: [PipelineDataType; 7] = [
        PipelineDataType::RawMScan,
        PipelineDataType::DataVector,
        PipelineDataType::MScan,
        PipelineDataType::BScanSegmentation,
        PipelineDataType::MScanSegmentation,
        PipelineDataType::Diameter,
        PipelineDataType::Mesh,
    ];
}

/// High Level description of a pipeline
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
