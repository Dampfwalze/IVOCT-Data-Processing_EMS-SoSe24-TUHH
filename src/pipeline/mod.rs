pub mod nodes;

use std::collections::HashMap;

use crate::{
    gui::node_graph::DynEditNode,
    node_graph::{impl_enum_from_into_id_types, NodeId, TypeId},
};

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
    pub nodes: HashMap<NodeId, Box<dyn DynEditNode>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }
}
