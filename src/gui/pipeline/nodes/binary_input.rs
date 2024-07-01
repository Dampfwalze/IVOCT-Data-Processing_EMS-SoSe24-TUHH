use core::fmt;

use egui::{DragValue, ProgressBar};

use super::prelude::*;

use crate::pipeline::{nodes::binary_input::*, types::DataType};

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OutputId::RawMScan => write!(f, "Raw M scan"),
            OutputId::DataVector => write!(f, "Data vector"),
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DataType::U8 => write!(f, "UInt 8"),
            DataType::U16 => write!(f, "UInt 16"),
            DataType::U32 => write!(f, "UInt 32"),
            DataType::U64 => write!(f, "UInt 64"),
            DataType::F32 => write!(f, "Float 32"),
            DataType::F64 => write!(f, "Float 64"),
        }
    }
}

impl EditNode for Node {
    type OutputId = OutputId;
    type InputId = InputIdNone;

    fn name(&self) -> &str {
        "Binary Input"
    }

    fn color(&self) -> egui::Color32 {
        colors::INPUT
    }

    fn connect(&mut self, _input: Self::InputId, _connection: NodeOutput) {
        unreachable!()
    }

    fn disconnect(&mut self, _input: Self::InputId) {
        unreachable!()
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        ui.output(
            self.input_type,
            self.input_type.data_type(),
            self.input_type.data_type().color(),
            |ui| {
                ui.node_label(format!("{}", self.input_type));
            },
        );

        NodeComboBox::from_id_source(ui.id().with("input_type"))
            .selected_text(format!("{}", self.input_type))
            .show_ui(ui, |ui| {
                for input_type in &[OutputId::RawMScan, OutputId::DataVector] {
                    ui.selectable_value(
                        &mut self.input_type,
                        *input_type,
                        format!("{}", input_type),
                    );
                }
            });

        NodeComboBox::from_id_source(ui.id().with("data_type"))
            .selected_text(format!("{}", self.data_type))
            .show_ui(ui, |ui| {
                for data_type in DataType::VALUES.into_iter() {
                    ui.selectable_value(&mut self.data_type, data_type, format!("{}", data_type));
                }
            });

        ui.add(PathInput::new(&mut self.path));

        if let OutputId::RawMScan = self.input_type {
            ui.add(
                DragValue::new(&mut self.a_scan_length)
                    .speed(1)
                    .prefix("A Scan Length: ")
                    .clamp_range(1..=usize::MAX),
            );
        }

        if let Some(progress) = self.progress_rx.as_ref().and_then(|rx| rx.borrow().clone()) {
            ui.add(ProgressBar::new(progress).rounding(3.0));
            ui.ctx().request_repaint();
        }
    }
}
