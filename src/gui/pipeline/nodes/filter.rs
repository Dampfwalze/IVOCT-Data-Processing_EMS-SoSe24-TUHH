use core::fmt;
use std::ops::DerefMut;

use egui::{Color32, DragValue, ProgressBar};

use crate::{
    gui::widgets::DragVector,
    pipeline::nodes::filter::{FilterType, Node},
};

use super::prelude::*;

impl fmt::Display for FilterType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FilterType::Gaussian => write!(f, "Gaussian"),
        }
    }
}

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputIdSingle;

    fn name(&self) -> &str {
        match self.filter_type {
            FilterType::Gaussian => "Gaussian Filter",
        }
    }

    fn color(&self) -> Color32 {
        colors::INPUT
    }

    fn connect(&mut self, _input: Self::InputId, connection: NodeOutput) {
        if connection.type_id == PipelineDataType::MScan.into() {
            self.input.connect(connection);
        }
    }

    fn disconnect(&mut self, _input: Self::InputId) {
        self.input.disconnect();
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
            self.input.connection(),
            PipelineDataType::MScan.color(),
            |ui| {
                ui.node_label("M Scan");
            },
        );

        NodeComboBox::from_id_source(ui.id().with("filter_type"))
            .selected_text(format!("{}", self.filter_type))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filter_type, FilterType::Gaussian, "Gaussian");
            });

        match self.filter_type {
            FilterType::Gaussian => {
                ui.add(
                    DragValue::new(&mut self.gauss_settings.sigma)
                        .speed(0.1)
                        .clamp_range(0.1..=50.0)
                        .prefix("Sigma: "),
                );

                let kernel_size = self.gauss_settings.kernel_size.deref_mut();

                ui.node_label("Kernel Size");
                ui.add(
                    DragVector::new([&mut kernel_size.x, &mut kernel_size.y])
                        .clamp_range(1..=100)
                        .prefix(["Rows: ", "Columns: "]),
                );
            }
        }

        if let Some(progress) = self.progress_rx.as_ref().and_then(|rx| rx.borrow().clone()) {
            ui.add(ProgressBar::new(progress).rounding(3.0));
            ui.ctx().request_repaint();
        }
    }
}
