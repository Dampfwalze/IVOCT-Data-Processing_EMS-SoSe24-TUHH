use egui::{Color32, ProgressBar};

use crate::{gui::widgets::PathInputAction, pipeline::nodes::output::*};

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdNone;
    type InputId = InputIdSingle;

    fn name(&self) -> &str {
        "Output"
    }

    fn color(&self) -> egui::Color32 {
        colors::OUTPUT
    }

    fn connect(&mut self, _input: Self::InputId, connection: NodeOutput) {
        self.input.connect(connection);
        self.input_type = connection.type_id.into();
    }

    fn disconnect(&mut self, _input: Self::InputId) {
        self.input.disconnect();
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        ui.input(
            InputIdSingle,
            self.input.connection(),
            match self.input.connection() {
                Some(_) => self.input_type.color(),
                None => Color32::GRAY,
            },
            |ui| {
                ui.node_label(match self.input.connection() {
                    Some(_) => format!("{}", self.input_type),
                    None => "Input".to_string(),
                });
            },
        );

        ui.add(PathInput::new(&mut self.path).action(PathInputAction::SaveFile));

        if ui.button("Save").clicked() {
            self.save();
        }

        if let Some(progress) = self.progress_rx.as_ref().and_then(|rx| rx.borrow().clone()) {
            ui.add(ProgressBar::new(progress).rounding(3.0));
            ui.ctx().request_repaint();
        }
    }
}
