use std::mem;

use crate::{
    gui::{
        dock_state::{DockState, TabType},
        node_graph::{NodeGraphEditState, NodeGraphEditor},
    },
    node_graph::NodeId,
    pipeline,
    view::{views, views_manager::DataViewsManager, DataViewsState},
};

pub struct IVOCTTestApp {
    pipeline: pipeline::Pipeline,
    pipeline_edit_state: NodeGraphEditState,
    pipeline_executor: pipeline::PipelineExecutor,

    data_views_state: DataViewsState,
    data_views_manager: DataViewsManager,

    dock_state: DockState,

    interacted_node: Option<NodeId>,
}

impl IVOCTTestApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> IVOCTTestApp {
        IVOCTTestApp {
            pipeline: pipeline::Pipeline::new(),
            pipeline_edit_state: NodeGraphEditState::new(),
            pipeline_executor: pipeline::PipelineExecutor::new(),
            data_views_state: DataViewsState::new(),
            data_views_manager: DataViewsManager::new().with_view::<views::data_vector::View>(),
            dock_state: DockState::new(),
            interacted_node: None,
        }
    }
}

impl eframe::App for IVOCTTestApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.interacted_node = None;

        let mut dock_state = mem::replace(&mut self.dock_state, DockState::new());

        egui_dock::DockArea::new(&mut dock_state)
            .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
            .show(ctx, self);

        egui::Window::new("debug").show(ctx, |ui| {
            ui.label("Debug panel");

            if ui.button("Add view").clicked() {
                let id: usize = ui.data_mut(|d| {
                    let val = d.get_temp(ui.id().with("debug_view_id")).unwrap_or(0) + 1;
                    d.insert_temp(ui.id().with("debug_view_id"), val);
                    val
                });
                dock_state.add_view_tab(id.into());
            }
        });

        self.dock_state = dock_state;

        self.data_views_manager.update(
            &mut self.data_views_state,
            &mut self.pipeline,
            &mut self.dock_state,
            self.interacted_node,
            ctx.input(|i| i.modifiers.ctrl),
        );

        self.pipeline_executor.update(&mut self.pipeline);
    }
}

impl egui_dock::TabViewer for IVOCTTestApp {
    type Tab = TabType;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            TabType::Pipeline => "Pipeline".into(),
            TabType::DataView(view_id) => {
                format!("Data View {:?}", Into::<usize>::into(*view_id)).into()
            }
        }
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        match tab {
            TabType::Pipeline => false,
            _ => true,
        }
    }

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        match tab {
            TabType::Pipeline => [false, false],
            _ => [true, true],
        }
    }

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        match tab {
            TabType::Pipeline => egui::Id::new("Pipeline"),
            TabType::DataView(view_id) => egui::Id::new(view_id).with("DataView"),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            TabType::Pipeline => {
                let _response =
                    NodeGraphEditor::new(&mut self.pipeline, &mut self.pipeline_edit_state)
                        .show(ui);

                if let Some(interacted_node) = _response.activated {
                    self.interacted_node = Some(interacted_node);
                }
            }
            TabType::DataView(view_id) => {
                ui.label(format!("Data View {:?}", Into::<usize>::into(*view_id)));
                if let Some(view) = self.data_views_state.get_mut(*view_id) {
                    view.ui(ui);
                }
            }
        }
    }
}
