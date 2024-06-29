use crate::{
    node_graph::NodeOutput,
    pipeline::{Pipeline, PipelineDataType},
    view::DataView,
};

pub struct View {
    input: NodeOutput,
}

impl DataView for View {
    fn from_node_output(node_output: &NodeOutput, _pipeline: &Pipeline) -> Option<Self> {
        if node_output.type_id == PipelineDataType::DataVector.into() {
            Some(Self {
                input: *node_output,
            })
        } else {
            None
        }
    }

    fn connect(&mut self, node_output: NodeOutput, _pipeline: &Pipeline) -> bool {
        if node_output.type_id == PipelineDataType::DataVector.into() {
            self.input = node_output;
            true
        } else {
            false
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Data View for Vector Data");

        ui.label(format!("Connection: {:?}", self.input));
    }
}
