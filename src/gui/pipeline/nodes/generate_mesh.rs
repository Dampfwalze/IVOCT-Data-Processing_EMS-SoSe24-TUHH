use egui::DragValue;

use crate::pipeline::nodes::generate_mesh::{InputId, Node};

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputId;

    fn name(&self) -> &str {
        "Generate Mesh"
    }

    fn color(&self) -> egui::Color32 {
        colors::PROCESS
    }

    fn connect(&mut self, input: Self::InputId, connection: NodeOutput) {
        match (input, PipelineDataType::from(connection.type_id)) {
            (InputId::BScans, PipelineDataType::BScanSegmentation) => {
                self.b_scans.connect(connection);
            }
            (InputId::Lumen, PipelineDataType::MScanSegmentation) => {
                self.lumen.connect(connection);
            }
            _ => {}
        }
    }

    fn disconnect(&mut self, input: Self::InputId) {
        match input {
            InputId::BScans => self.b_scans.disconnect(),
            InputId::Lumen => self.lumen.disconnect(),
        }
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        ui.output(
            OutputIdSingle,
            PipelineDataType::Mesh,
            PipelineDataType::Mesh.color(),
            |ui| {
                ui.node_label("Mesh");
            },
        );

        ui.input(
            InputId::BScans,
            self.b_scans.connection(),
            PipelineDataType::BScanSegmentation.color(),
            |ui| {
                ui.node_label("B-Scans");
            },
        );

        ui.input(
            InputId::Lumen,
            self.lumen.connection(),
            PipelineDataType::MScanSegmentation.color(),
            |ui| {
                ui.node_label("Lumen");
            },
        );

        ui.add(
            DragValue::new(&mut self.settings.rotational_samples)
                .clamp_range(0..=1000)
                .prefix("Rotational Samples: "),
        );

        ui.add(
            DragValue::new(&mut self.settings.rotation_frequency)
                .clamp_range(0.0..=f32::INFINITY)
                .prefix("Rotation Frequency: ")
                .suffix(" Hz"),
        );

        ui.add(
            DragValue::new(&mut self.settings.pullback_speed)
                .clamp_range(0.0..=f32::INFINITY)
                .speed(0.01)
                .prefix("Pullback Speed: ")
                .suffix(" mm/s"),
        );

        ui.add(
            DragValue::new(&mut self.settings.mm_per_pixel)
                .clamp_range(0.0..=f32::INFINITY)
                .speed(0.001)
                .prefix("mm per pixel: ")
                .suffix(" mm"),
        );

        ui.add(
            DragValue::new(&mut self.settings.refraction_index)
                .clamp_range(0.0..=f32::INFINITY)
                .speed(0.01)
                .prefix("Refraction Index: "),
        );
    }
}
