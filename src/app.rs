use std::mem;

use crate::{
    cache::Cache,
    gui::{
        dock_state::{DockState, TabType},
        node_graph::{NodeGraphEditState, NodeGraphEditor},
    },
    node_graph::NodeId,
    pipeline,
    view::{
        execution::executor::ViewsExecutor,
        views,
        views_manager::{DataViewsManager, DataViewsManagerBuilder},
        DataViewsState,
    },
};

pub struct IVOCTTestApp {
    pipeline: pipeline::Pipeline,
    pipeline_edit_state: NodeGraphEditState,
    pipeline_executor: pipeline::PipelineExecutor,

    data_views_state: DataViewsState,
    data_views_manager: DataViewsManager,
    data_views_executor: ViewsExecutor,

    dock_state: DockState,

    cache: Cache,

    interacted_node: Option<NodeId>,
}

impl IVOCTTestApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> IVOCTTestApp {
        IVOCTTestApp {
            pipeline: pipeline::Pipeline::new(),
            pipeline_edit_state: NodeGraphEditState::new(),
            pipeline_executor: pipeline::PipelineExecutor::new(),
            data_views_state: DataViewsState::new(),
            data_views_manager: DataViewsManagerBuilder::new(
                &cc.wgpu_render_state.as_ref().unwrap(),
            )
            .with_view::<views::data_vector::View>()
            .with_view::<views::m_scan::View>()
            .build(),
            data_views_executor: ViewsExecutor::new(),
            dock_state: DockState::new(),
            cache: Cache::new(),
            interacted_node: None,
        }
    }
}

impl eframe::App for IVOCTTestApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.interacted_node = None;

        let mut dock_state = mem::replace(&mut self.dock_state, DockState::new());

        egui_dock::DockArea::new(&mut dock_state)
            .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
            .show(ctx, self);

        self.dock_state = dock_state;

        self.data_views_manager.update(
            &mut self.data_views_state,
            &mut self.pipeline,
            &mut self.dock_state,
            self.interacted_node,
            &self.cache,
            frame.wgpu_render_state().unwrap(),
            ctx.input(|i| i.modifiers.ctrl),
        );

        self.pipeline_executor.update(&mut self.pipeline);

        self.data_views_executor
            .update(&mut self.data_views_state, &self.pipeline_executor);
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

    fn force_close(&mut self, tab: &mut Self::Tab) -> bool {
        // Close data views that are either non-existent anymore or have no
        // inputs
        matches!(
            tab,
            TabType::DataView(view_id) if self
                .data_views_state
                .get(*view_id)
                .map_or(true, |v| v.inputs().is_empty())
        )
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        match tab {
            TabType::Pipeline => false,
            _ => true,
        }
    }

    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        [false, false]
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
                if let Some(view) = self.data_views_state.get_mut(*view_id) {
                    view.ui(ui);
                } else {
                    ui.label(format!(
                        "Data View {:?} does not exist, You can close this tab.",
                        Into::<usize>::into(*view_id)
                    ));
                }
            }
        }
    }
}
