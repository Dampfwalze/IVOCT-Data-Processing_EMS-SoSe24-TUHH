use egui::DragValue;

use crate::pipeline::nodes::follow_lumen::{InputId, Node};

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputId;

    fn name(&self) -> &str {
        "Follow Lumen"
    }

    fn color(&self) -> egui::Color32 {
        colors::PROCESS
    }

    fn connect(&mut self, input: Self::InputId, connection: NodeOutput) {
        match (input, PipelineDataType::from(connection.type_id)) {
            (InputId::MScan, PipelineDataType::MScan) => {
                self.m_scan.connect(connection);
            }
            (InputId::CatheterSegmentation, PipelineDataType::MScanSegmentation) => {
                self.catheter_segmentation.connect(connection);
            }
            _ => {}
        }
    }

    fn disconnect(&mut self, input: Self::InputId) {
        match input {
            InputId::MScan => self.m_scan.disconnect(),
            InputId::CatheterSegmentation => self.catheter_segmentation.disconnect(),
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
            InputId::CatheterSegmentation,
            self.catheter_segmentation.connection(),
            PipelineDataType::MScanSegmentation.color(),
            |ui| {
                ui.node_label("Catheter Segmentation");
            },
        );

        ui.add(DragValue::new(&mut self.settings.window_extend_up).prefix("Radius Up: "));
        ui.add(DragValue::new(&mut self.settings.window_extend_down).prefix("Radius Down: "));

        ui.add(
            DragValue::new(&mut self.settings.threshold)
                .speed(0.01)
                .range(0.0..=1.0)
                .prefix("Threshold: "),
        );

        ui.checkbox(&mut self.settings.check_artifact, "Check Artifacts");

        if self.settings.check_artifact {
            ui.add(
                DragValue::new(&mut self.settings.artifact_threshold)
                    .speed(0.01)
                    .range(0.0..=1.0)
                    .prefix("Artifact Threshold: "),
            );
        }
    }
}
