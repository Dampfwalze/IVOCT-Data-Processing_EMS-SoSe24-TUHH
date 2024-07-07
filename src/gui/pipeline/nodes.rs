pub mod binary_input;
pub mod filter;
pub mod follow_catheter;
pub mod follow_lumen;
pub mod output;
pub mod process_raw_m_scan;
pub mod remove_detector_defect;
pub mod segment_b_scans;

use core::fmt;

use egui::Color32;

use crate::pipeline::PipelineDataType;

#[allow(unused_imports)]
mod prelude {
    pub(super) use crate::{
        gui::node_graph::{EditNode, NodeUi},
        pipeline::{Pipeline, PipelineDataType},
    };

    pub(super) use super::colors;

    pub(super) use graph::{
        InputIdNone, InputIdSingle, NodeId, NodeInput, NodeOutput, OutputIdNone, OutputIdSingle,
        TypeId,
    };

    pub(super) use widgets::*;

    pub(super) mod graph {
        pub use crate::node_graph::{
            InputId, InputIdNone, InputIdSingle, NodeId, NodeInput, NodeOutput, OutputId,
            OutputIdNone, OutputIdSingle, TypeId,
        };
    }

    pub(super) mod widgets {
        pub use crate::gui::{
            node_graph::UiNodeExt,
            widgets::{NodeComboBox, PathInput},
        };
    }
}

mod colors {
    use egui::Color32;

    pub const INPUT: Color32 = Color32::from_rgb(121, 70, 29);
    pub const OUTPUT: Color32 = Color32::from_rgb(60, 60, 131);
    pub const PROCESS: Color32 = Color32::from_rgb(43, 101, 43);
    pub const FILTER: Color32 = Color32::from_rgb(131, 49, 74);
    // pub const TRANSFORM: Color32 = Color32::from_rgb(36, 98, 131);
}

impl PipelineDataType {
    fn color(&self) -> Color32 {
        match self {
            PipelineDataType::RawMScan => Color32::from_rgb(121, 70, 29),
            PipelineDataType::DataVector => Color32::from_rgb(0, 128, 255),
            PipelineDataType::MScan => Color32::from_rgb(121, 70, 29),
            PipelineDataType::BScanSegmentation => Color32::from_rgb(0, 128, 255),
            PipelineDataType::MScanSegmentation => Color32::from_rgb(128, 0, 128),
        }
    }
}

impl fmt::Display for PipelineDataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PipelineDataType::RawMScan => write!(f, "Raw M scan"),
            PipelineDataType::DataVector => write!(f, "Data vector"),
            PipelineDataType::MScan => write!(f, "M scan"),
            PipelineDataType::BScanSegmentation => write!(f, "B-scan segmentation"),
            PipelineDataType::MScanSegmentation => write!(f, "M-scan segmentation"),
        }
    }
}
