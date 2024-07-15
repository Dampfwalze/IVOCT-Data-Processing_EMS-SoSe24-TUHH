use egui::{Color32, ComboBox, ProgressBar};

use crate::{
    gui::widgets::PathInputAction,
    pipeline::{nodes::output::*, types::DataType},
};

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

        if let PipelineDataType::RawMScan | PipelineDataType::MScan = self.input_type {
            ComboBox::from_id_source(ui.id().with("input_type"))
                .selected_text(format!("{}", self.scan_data_type))
                .show_ui(ui, |ui| {
                    for scan_data_type in DataType::VALUES {
                        ui.selectable_value(
                            &mut self.scan_data_type,
                            scan_data_type,
                            format!("{}", scan_data_type),
                        );
                    }
                });
        }

        ui.add(PathInput::new(&mut self.path).action(PathInputAction::SaveFile));

        if ui.button("Save").clicked() {
            self.save();
        }

        if let Some(progress_rx) = &self.progress_rx {
            match progress_rx.borrow().clone() {
                Progress::Idle => {}
                Progress::Working(None) => {
                    ui.add(
                        ProgressBar::new(0.9999)
                            .rounding(3.0)
                            .text("Working...")
                            .animate(true),
                    );
                }
                Progress::Working(Some(progress)) => {
                    ui.add(ProgressBar::new(progress).rounding(3.0));
                    ui.ctx().request_repaint();
                }
            }
        }
    }
}
