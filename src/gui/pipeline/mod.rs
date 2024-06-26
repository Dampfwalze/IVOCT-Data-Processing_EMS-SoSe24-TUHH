pub mod nodes;

use std::path::PathBuf;

use crate::{
    node_graph::NodeId,
    pipeline::{nodes::*, Pipeline},
};

use super::node_graph::{DynEditNode, EditNodeGraph};

impl EditNodeGraph for Pipeline {
    fn get_node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut dyn DynEditNode> {
        match self.nodes.get_mut(&node_id) {
            Some(node) => Some(node.as_edit_node_mut()),
            None => None,
        }
    }

    fn remove_node(&mut self, node_id: NodeId) {
        self.nodes.remove(&node_id);
    }

    fn add_node(&mut self, path: &str) -> NodeId {
        let id: usize = self.nodes.keys().copied().max().unwrap_or(0.into()).into();
        let id = id + 1;

        let node: Box<dyn DynPipelineNode> = match path {
            "Input/Raw M Scan Input" => Box::new(binary_input::Node::m_scan(PathBuf::new(), None)),
            "Input/Binary Vector Input" => {
                Box::new(binary_input::Node::data_vector(PathBuf::new()))
            }
            "Process/Process Raw M Scan" => Box::new(process_raw_m_scan::Node::default()),
            _ => panic!("Invalid path: {}", path),
        };

        self.nodes.insert(id.into(), node);

        id.into()
    }

    fn addable_nodes(&self) -> Vec<&'static str> {
        vec![
            "Input/Raw M Scan Input",
            "Input/Binary Vector Input",
            "Process/Process Raw M Scan",
        ]
    }
}
