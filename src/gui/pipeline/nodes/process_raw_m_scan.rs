use egui::{Color32, DragValue, ProgressBar};

use crate::pipeline::nodes::process_raw_m_scan::*;

use super::prelude::*;

impl EditNode for Node {
    type OutputId = OutputIdSingle;
    type InputId = InputId;

    fn name(&self) -> &str {
        "Process Raw M Scan"
    }

    fn color(&self) -> Color32 {
        colors::PROCESS
    }

    fn connect(&mut self, input: Self::InputId, connection: NodeOutput) {
        match (input, connection.type_id.into()) {
            (InputId::RawMScan, PipelineDataType::RawMScan) => self.raw_scan.connect(connection),
            (InputId::Offset, PipelineDataType::DataVector) => self.offset.connect(connection),
            (InputId::Chirp, PipelineDataType::DataVector) => self.chirp.connect(connection),
            _ => {}
        }
    }

    fn disconnect(&mut self, input: Self::InputId) {
        match input {
            InputId::RawMScan => self.raw_scan.disconnect(),
            InputId::Offset => self.offset.disconnect(),
            InputId::Chirp => self.chirp.disconnect(),
        }
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
            InputId::RawMScan,
            self.raw_scan.connection(),
            PipelineDataType::RawMScan.color(),
            |ui| {
                ui.node_label("Raw M Scan");
            },
        );

        ui.input(
            InputId::Offset,
            self.offset.connection(),
            PipelineDataType::DataVector.color(),
            |ui| {
                ui.node_label("Offset");
            },
        );

        ui.input(
            InputId::Chirp,
            self.chirp.connection(),
            PipelineDataType::DataVector.color(),
            |ui| {
                ui.node_label("Chirp");
            },
        );

        ui.add(
            DragValue::new(&mut self.factor)
                .range(1.0..=f64::INFINITY)
                .prefix("Factor: "),
        );

        ui.add(
            DragValue::new(&mut self.rescale_cutoff)
                .range(1..=usize::MAX)
                .prefix("Rescale Cutoff: "),
        );

        if let Some(progress) = self.progress_rx.as_ref().and_then(|rx| rx.borrow().clone()) {
            ui.add(ProgressBar::new(progress).rounding(3.0));
            ui.ctx().request_repaint();
        }
    }
}
