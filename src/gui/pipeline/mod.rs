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
            "In Out/Raw M Scan Input" => {
                Box::new(binary_input::Node::raw_m_scan(PathBuf::new(), None))
            }
            "In Out/M Scan Input" => Box::new(binary_input::Node::m_scan(PathBuf::new(), None)),
            "In Out/Binary Vector Input" => {
                Box::new(binary_input::Node::data_vector(PathBuf::new()))
            }
            "In Out/Output" => Box::new(output::Node::default()),
            "Process/Process Raw M Scan" => Box::new(process_raw_m_scan::Node::default()),
            "Process/Remove Detector Defect" => Box::new(remove_detector_defect::Node::new()),
            "Process/Segment B Scans" => Box::new(segment_b_scans::Node::default()),
            "Process/Follow Catheter" => Box::new(follow_catheter::Node::default()),
            "Filter/Gaussian Filter" => Box::new(filter::Node::gaussian()),
            "Filter/Median Filter" => Box::new(filter::Node::median()),
            "Filter/Align Brightness" => Box::new(filter::Node::align_brightness()),
            "Filter/Wiener Filter" => Box::new(filter::Node::wiener()),
            "Filter/Prewitt Filter" => Box::new(filter::Node::prewitt()),
            "Filter/Widen Structures" => Box::new(filter::Node::widen_structures()),
            "Filter/Binary Area Opening" => Box::new(filter::Node::b_ware_open()),
            _ => panic!("Invalid path: {}", path),
        };

        self.nodes.insert(id.into(), node);

        id.into()
    }

    fn addable_nodes(&self) -> Vec<&'static str> {
        vec![
            "In Out/Raw M Scan Input",
            "In Out/M Scan Input",
            "In Out/Binary Vector Input",
            "In Out/Output",
            "Process/Process Raw M Scan",
            "Process/Remove Detector Defect",
            "Process/Segment B Scans",
            "Process/Follow Catheter",
            "Filter/Gaussian Filter",
            "Filter/Median Filter",
            "Filter/Align Brightness",
            "Filter/Wiener Filter",
            "Filter/Prewitt Filter",
            "Filter/Widen Structures",
            "Filter/Binary Area Opening",
        ]
    }
}
