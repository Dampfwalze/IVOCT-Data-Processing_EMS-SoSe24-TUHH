use core::fmt;
use std::fmt::{Display, Formatter};

use egui::Color32;

use crate::{
    gui::{
        node_graph::{EditNode, UiNodeExt},
        widgets::{NodeComboBox, PathInput},
    },
    node_graph::{InputIdNone, OutputIdSingle},
    pipeline::{
        nodes::{self, BinaryInputType, ProcessRawMScanInputId, ProcessRawMScanNode},
        PipelineDataType,
    },
};

mod colors {
    use egui::Color32;

    pub const INPUT: Color32 = Color32::from_rgb(121, 70, 29);
}

impl PipelineDataType {
    fn color(&self) -> Color32 {
        match self {
            PipelineDataType::RawMScan => Color32::from_rgb(121, 70, 29),
            PipelineDataType::DataVector => Color32::from_rgb(0, 128, 255),
            PipelineDataType::MScan => Color32::from_rgb(121, 70, 29),
        }
    }
}

impl Display for BinaryInputType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            BinaryInputType::RawMScan => write!(f, "Raw M scan"),
            BinaryInputType::DataVector => write!(f, "Data vector"),
        }
    }
}

impl EditNode for nodes::BinaryInputNode {
    type OutputId = nodes::BinaryInputType;
    type InputId = InputIdNone;

    fn name(&self) -> &str {
        "Binary Input"
    }

    fn color(&self) -> egui::Color32 {
        colors::INPUT
    }

    fn connect(&mut self, _input: Self::InputId, _connection: crate::node_graph::NodeOutput) {
        unreachable!()
    }

    fn disconnect(&mut self, _input: Self::InputId) {
        unreachable!()
    }

    fn ui(&mut self, ui: &mut crate::gui::node_graph::NodeUi) {
        // let transform = ui
        //     .ctx()
        //     .memory_mut(|mem| mem.layer_transforms.get(&ui.layer_id()).copied())
        //     .unwrap_or(TSTransform::IDENTITY);

        NodeComboBox::from_id_source(ui.id().with("data_type"))
            .selected_text(format!("{}", self.data_type))
            .show_ui(ui, |ui| {
                for data_type in &[BinaryInputType::RawMScan, BinaryInputType::DataVector] {
                    ui.selectable_value(&mut self.data_type, *data_type, format!("{}", data_type));
                }
            });

        // Path
        ui.add(PathInput::new(&mut self.path));

        ui.output(
            self.data_type,
            self.data_type.data_type(),
            self.data_type.data_type().color(),
            |ui| {
                ui.node_label(format!("{}", self.data_type));
            },
        );
    }
}

impl EditNode for ProcessRawMScanNode {
    type OutputId = OutputIdSingle;
    type InputId = ProcessRawMScanInputId;

    fn name(&self) -> &str {
        "Process Raw M Scan"
    }

    fn color(&self) -> Color32 {
        colors::INPUT
    }

    fn connect(&mut self, input: Self::InputId, connection: crate::node_graph::NodeOutput) {
        match (input, connection.type_id.into()) {
            (ProcessRawMScanInputId::RawMScan, PipelineDataType::RawMScan) => {
                self.raw_scan.connect(connection)
            }
            (ProcessRawMScanInputId::Offset, PipelineDataType::DataVector) => {
                self.offset.connect(connection)
            }
            (ProcessRawMScanInputId::Chirp, PipelineDataType::DataVector) => {
                self.chirp.connect(connection)
            }
            _ => {}
        }
    }

    fn disconnect(&mut self, input: Self::InputId) {
        match input {
            ProcessRawMScanInputId::RawMScan => self.raw_scan.disconnect(),
            ProcessRawMScanInputId::Offset => self.offset.disconnect(),
            ProcessRawMScanInputId::Chirp => self.chirp.disconnect(),
        }
    }

    fn ui(&mut self, ui: &mut crate::gui::node_graph::NodeUi) {
        ui.output(
            OutputIdSingle,
            PipelineDataType::MScan,
            PipelineDataType::MScan.color(),
            |ui| {
                ui.node_label("M Scan");
            },
        );

        ui.input(
            ProcessRawMScanInputId::RawMScan,
            self.raw_scan.connection(),
            PipelineDataType::RawMScan.color(),
            |ui| {
                ui.node_label("Raw M Scan");
            },
        );

        ui.input(
            ProcessRawMScanInputId::Offset,
            self.offset.connection(),
            PipelineDataType::DataVector.color(),
            |ui| {
                ui.node_label("Offset");
            },
        );

        ui.input(
            ProcessRawMScanInputId::Chirp,
            self.chirp.connection(),
            PipelineDataType::DataVector.color(),
            |ui| {
                ui.node_label("Chirp");
            },
        );
    }
}
