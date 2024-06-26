use core::fmt;

use egui::DragValue;

use super::prelude::*;

use crate::pipeline::nodes::binary_input::*;

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OutputId::RawMScan => write!(f, "Raw M scan"),
            OutputId::DataVector => write!(f, "Data vector"),
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
            self.data_type,
            self.data_type.data_type(),
            self.data_type.data_type().color(),
            |ui| {
                ui.node_label(format!("{}", self.data_type));
            },
        );

        NodeComboBox::from_id_source(ui.id().with("data_type"))
            .selected_text(format!("{}", self.data_type))
            .show_ui(ui, |ui| {
                for data_type in &[OutputId::RawMScan, OutputId::DataVector] {
                    ui.selectable_value(&mut self.data_type, *data_type, format!("{}", data_type));
                }
            });

        ui.add(PathInput::new(&mut self.path));

        if let OutputId::RawMScan = self.data_type {
            ui.add(
                DragValue::new(&mut self.a_scan_length)
                    .speed(1)
                    .prefix("A Scan Length: ")
                    .clamp_range(1..=usize::MAX),
            );
        }
    }
}
