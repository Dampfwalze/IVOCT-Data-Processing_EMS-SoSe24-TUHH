use egui::{DragValue, ProgressBar};

use crate::{gui::widgets::DragVector, pipeline::nodes::segment_b_scans::Node};

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputIdSingle;

    fn name(&self) -> &str {
        "Segment B Scans"
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
            PipelineDataType::BScanSegmentation,
            PipelineDataType::BScanSegmentation.color(),
            |ui| {
                ui.node_label("B Scan Segmentation");
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

        ui.add(DragValue::new(&mut self.settings.neighbor_count).prefix("Neighbor Count: "));

        ui.add(
            DragValue::new(&mut self.settings.neighborhood_width).prefix("Neighborhood Width: "),
        );

        ui.label("Search Range:");
        ui.add(
            DragVector::new([
                &mut self.settings.search_range_start,
                &mut self.settings.search_range_end,
            ])
            .prefix(["From: ", "To: "]),
        );

        ui.add(DragValue::new(&mut self.settings.offset).prefix("Offset: "));

        if let Some(progress) = self.progress_rx.as_ref().and_then(|rx| rx.borrow().clone()) {
            ui.add(ProgressBar::new(progress).rounding(3.0));
            ui.ctx().request_repaint();
        }
    }
}
