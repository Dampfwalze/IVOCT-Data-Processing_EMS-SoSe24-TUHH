use egui::{Checkbox, DragValue};

use crate::pipeline::nodes::diameter::{InputId, Node};

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputId;

    fn name(&self) -> &str {
        "Diameter"
    }

    fn color(&self) -> egui::Color32 {
        colors::PROCESS
    }

    fn connect(&mut self, input: Self::InputId, connection: NodeOutput) {
        match (input, PipelineDataType::from(connection.type_id)) {
            (InputId::BScans, PipelineDataType::BScanSegmentation) => {
                self.b_scans.connect(connection);
            }
            (InputId::Catheter, PipelineDataType::MScanSegmentation) => {
                self.catheter.connect(connection);
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
            InputId::Catheter => self.catheter.disconnect(),
            InputId::Lumen => self.lumen.disconnect(),
        }
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        ui.output(
            OutputIdSingle,
            PipelineDataType::Diameter,
            PipelineDataType::Diameter.color(),
            |ui| {
                ui.node_label("Diameter");
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

        ui.input(
            InputId::Catheter,
            self.catheter.connection(),
            PipelineDataType::MScanSegmentation.color(),
            |ui| {
                ui.node_label("Catheter");
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

        ui.add(
            DragValue::new(&mut self.settings.mm_per_pixel)
                .clamp_range(0.0..=f32::INFINITY)
                .speed(0.001)
                .prefix("mm per pixel: "),
        );

        ui.add(
            DragValue::new(&mut self.settings.refraction_index)
                .clamp_range(0.0..=f32::INFINITY)
                .speed(0.01)
                .prefix("refraction index: "),
        );

        ui.label("Catheter diameter");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.add(Checkbox::without_text(
                &mut self.settings.use_catheter_diameter,
            ));

            ui.add_enabled(
                self.settings.use_catheter_diameter,
                DragValue::new(&mut self.settings.catheter_diameter)
                    .clamp_range(0.0..=f32::INFINITY)
                    .speed(0.01)
                    .suffix(" mm"),
            );
        });
    }
}
