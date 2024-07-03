use egui::DragValue;

use crate::pipeline::nodes::remove_detector_defect::Node;

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputIdSingle;

    fn name(&self) -> &str {
        "Remove Detector Defect"
    }

    fn color(&self) -> egui::Color32 {
        colors::INPUT
    }

    fn connect(&mut self, _input: Self::InputId, connection: NodeOutput) {
        if connection.type_id == PipelineDataType::MScan.into() {
            self.m_scan.connect(connection);
        }
    }

    fn disconnect(&mut self, _input: Self::InputId) {
        self.m_scan.disconnect();
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        ui.output(
            OutputIdSingle,
            PipelineDataType::MScan,
            PipelineDataType::MScan.color(),
            |ui| {
                ui.node_label("M Scan");
            },
        );

        ui.input(
            InputIdSingle,
            self.m_scan.connection(),
            PipelineDataType::MScan.color(),
            |ui| {
                ui.node_label("M Scan");
            },
        );

        ui.add(DragValue::new(&mut self.upper).prefix("Upper: "));
        ui.add(DragValue::new(&mut self.lower).prefix("Lower: "));
    }
}
