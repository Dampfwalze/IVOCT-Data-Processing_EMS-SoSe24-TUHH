use egui::DragValue;

use crate::pipeline::nodes::follow_catheter::{InputId, Node};

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputId;

    fn name(&self) -> &str {
        "Follow Catheter"
    }

    fn color(&self) -> egui::Color32 {
        colors::PROCESS
    }

    fn connect(&mut self, input: Self::InputId, connection: NodeOutput) {
        match (input, PipelineDataType::from(connection.type_id)) {
            (InputId::MScan, PipelineDataType::MScan) => {
                self.m_scan.connect(connection);
            }
            (InputId::BScanSegmentation, PipelineDataType::BScanSegmentation) => {
                self.b_scan_segmentation.connect(connection);
            }
            _ => {}
        }
    }

    fn disconnect(&mut self, input: Self::InputId) {
        match input {
            InputId::MScan => self.m_scan.disconnect(),
            InputId::BScanSegmentation => self.b_scan_segmentation.disconnect(),
        }
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        ui.output(
            OutputIdSingle,
            PipelineDataType::MScanSegmentation,
            PipelineDataType::MScanSegmentation.color(),
            |ui| {
                ui.node_label("Segmentation");
            },
        );

        ui.input(
            InputId::MScan,
            self.m_scan.connection(),
            PipelineDataType::MScan.color(),
            |ui| {
                ui.node_label("M-Scan");
            },
        );

        ui.input(
            InputId::BScanSegmentation,
            self.b_scan_segmentation.connection(),
            PipelineDataType::BScanSegmentation.color(),
            |ui| {
                ui.node_label("B-Scan Segmentation");
            },
        );

        ui.add(DragValue::new(&mut self.settings.start_height).prefix("Start Height: "));

        ui.add(DragValue::new(&mut self.settings.window_extend).prefix("Radius: "));
    }
}
