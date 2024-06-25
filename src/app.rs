use crate::{
    gui::node_graph::{NodeGraphEditState, NodeGraphEditor},
    pipeline,
};

pub struct IVOCTTestApp {
    pipeline: pipeline::Pipeline,
    pipeline_edit_state: NodeGraphEditState,
    pipeline_executor: pipeline::PipelineExecutor,
}

impl IVOCTTestApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> IVOCTTestApp {
        IVOCTTestApp {
            pipeline: pipeline::Pipeline::new(),
            pipeline_edit_state: NodeGraphEditState::new(),
            pipeline_executor: pipeline::PipelineExecutor::new(),
        }
    }
}

impl eframe::App for IVOCTTestApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            NodeGraphEditor::new(&mut self.pipeline, &mut self.pipeline_edit_state).show(ui);
        });

        self.pipeline_executor.update(&mut self.pipeline);
    }
}
