use core::fmt;
use std::ops::DerefMut;

use egui::{Color32, ComboBox, DragValue, ProgressBar};

use crate::{
    gui::widgets::DragVector,
    pipeline::nodes::filter::{AreaConnectionType, FilterType, Node},
};

use super::prelude::*;

impl fmt::Display for FilterType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FilterType::Gaussian => write!(f, "Gaussian"),
            FilterType::Median => write!(f, "Median"),
            FilterType::AlignBrightness => write!(f, "Align Brightness"),
            FilterType::Wiener => write!(f, "Wiener"),
            FilterType::Prewitt => write!(f, "Prewitt"),
            FilterType::WidenStructures => write!(f, "Widen Structures"),
            FilterType::BWAreaOpen => write!(f, "Binary Area Opening"),
        }
    }
}

impl fmt::Display for AreaConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AreaConnectionType::Star4 => write!(f, "Star 4"),
            AreaConnectionType::Circle8 => write!(f, "Circle 8"),
        }
    }
}

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputIdSingle;

    fn name(&self) -> &str {
        match self.filter_type {
            FilterType::Gaussian => "Gaussian Filter",
            FilterType::Median => "Median Filter",
            FilterType::AlignBrightness => "Align Brightness",
            FilterType::Wiener => "Wiener Filter",
            FilterType::Prewitt => "Prewitt Filter",
            FilterType::WidenStructures => "Widen Structures",
            FilterType::BWAreaOpen => "Binary Area Opening",
        }
    }

    fn color(&self) -> Color32 {
        colors::FILTER
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

        ComboBox::from_id_source(ui.id().with("filter_type"))
            .selected_text(format!("{}", self.filter_type))
            .show_ui(ui, |ui| {
                for filter_type in FilterType::VALUES {
                    ui.selectable_value(
                        &mut self.filter_type,
                        filter_type,
                        format!("{}", filter_type),
                    );
                }
            });

        match self.filter_type {
            FilterType::Gaussian => {
                ui.add(
                    DragValue::new(&mut self.gauss_settings.sigma)
                        .speed(0.1)
                        .range(0.1..=50.0)
                        .prefix("Sigma: "),
                );

                let kernel_size = self.gauss_settings.kernel_size.deref_mut();

                ui.node_label("Kernel Size");
                ui.add(
                    DragVector::new([&mut kernel_size.x, &mut kernel_size.y])
                        .range(1..=100)
                        .prefix(["Rows: ", "Columns: "]),
                );
            }
            FilterType::Median => {
                let size = self.median_settings.size.deref_mut();

                ui.node_label("Kernel Size");
                ui.add(
                    DragVector::new([&mut size.x, &mut size.y])
                        .range(1..=100)
                        .prefix(["Rows: ", "Columns: "]),
                );
            }
            FilterType::AlignBrightness => {}
            FilterType::Wiener => {
                let size = self.wiener_settings.neighborhood_size.deref_mut();

                ui.node_label("Neighborhood Size");
                ui.add(
                    DragVector::new([&mut size.x, &mut size.y])
                        .range(1..=100)
                        .prefix(["Rows: ", "Columns: "]),
                );
            }
            FilterType::Prewitt => {
                ui.add(
                    DragValue::new(&mut self.prewitt_settings.threshold)
                        .speed(0.01)
                        .range(0.0..=1.0)
                        .prefix("Threshold: "),
                );
            }
            FilterType::WidenStructures => {
                ui.add(
                    DragValue::new(&mut self.widen_structures_settings.width)
                        .range(0..=300)
                        .prefix("Size: "),
                );
            }
            FilterType::BWAreaOpen => {
                ui.add(
                    DragValue::new(&mut self.b_w_area_open_settings.area)
                        .range(0..=1000)
                        .prefix("Area Size: "),
                );

                ComboBox::from_id_source(ui.id().with("conn_type"))
                    .selected_text(format!("{}", self.b_w_area_open_settings.connection_type))
                    .show_ui(ui, |ui| {
                        for conn_type in AreaConnectionType::VALUES {
                            ui.selectable_value(
                                &mut self.b_w_area_open_settings.connection_type,
                                conn_type,
                                format!("{}", conn_type),
                            );
                        }
                    });
            }
        }

        if let Some(progress) = self.progress_rx.as_ref().and_then(|rx| rx.borrow().clone()) {
            ui.add(ProgressBar::new(progress).rounding(3.0));
            ui.ctx().request_repaint();
        }
    }
}
