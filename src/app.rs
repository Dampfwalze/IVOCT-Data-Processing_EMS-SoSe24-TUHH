use std::{borrow::Cow, mem};

use crate::{
    cache::Cache,
    gui::{
        dock_state::{DockState, TabType},
        node_graph::{NodeGraphEditState, NodeGraphEditor},
    },
    node_graph::NodeId,
    pipeline::{self, nodes},
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

    load_pipeline: Option<Cow<'static, str>>,
}

impl IVOCTTestApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> IVOCTTestApp {
        let pipeline_json = cc.storage.unwrap().get_string("user_pipeline");

        let pipeline_json: Cow<_> = match pipeline_json {
            Some(json) => json.into(),
            None => pipeline::presets::PHANTOM_1_1_3.into(),
        };

        let (pipeline, state) = Self::load_pipeline(&pipeline_json);

        IVOCTTestApp {
            pipeline,
            pipeline_edit_state: state,
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
            load_pipeline: None,
        }
    }

    fn load_pipeline(pipeline_json: &str) -> (pipeline::Pipeline, NodeGraphEditState) {
        let (mut pipeline, state) = serde_json::from_str(pipeline_json)
            .unwrap_or_else(|_| (pipeline::Pipeline::new(), NodeGraphEditState::new()));

        // Clear all path that do not exist
        for (_, node) in &mut pipeline.nodes {
            if let Some(node) = node
                .as_any_mut()
                .downcast_mut::<nodes::binary_input::Node>()
            {
                if !node.path.exists() {
                    node.path = "".into();
                }
            } else if let Some(node) = node.as_any_mut().downcast_mut::<nodes::output::Node>() {
                if !node.path.exists() {
                    node.path = "".into();
                }
            }
        }

        (pipeline, state)
    }

    fn set_pipeline(&mut self, pipeline: pipeline::Pipeline, state: NodeGraphEditState) {
        self.pipeline = pipeline;
        self.pipeline_edit_state = state;

        self.pipeline_executor.clear();
        self.data_views_state.clear();

        self.dock_state.close_all_views();
    }
}

// MARK: impl App

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

        if let Some(json) = self.load_pipeline.take() {
            let (pipeline, state) = Self::load_pipeline(&json);
            self.set_pipeline(pipeline, state);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        println!("Saving");

        let pipeline = serde_json::to_string(&(&self.pipeline, &self.pipeline_edit_state)).unwrap();

        storage.set_string("user_pipeline", pipeline)
    }
}

// MARK: impl TabViewer

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
                self.pipeline_menu_bar(ui);

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

// MARK: Pipeline Menu Bar

impl IVOCTTestApp {
    fn pipeline_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open").clicked() {
                    let file = native_dialog::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .set_title("Open Pipeline")
                        .show_open_single_file();

                    if let Ok(Some(file)) = file {
                        match std::fs::read_to_string(file) {
                            Ok(json) => self.load_pipeline = Some(json.into()),
                            Err(e) => eprintln!("Error loading pipeline: {}", e),
                        }
                    }

                    ui.close_menu();
                }

                ui.menu_button("Presets", |ui| {
                    if ui.button("Phantom 1.1.3").clicked() {
                        self.load_pipeline = Some(pipeline::presets::PHANTOM_1_1_3.into());
                        ui.close_menu();
                    }
                    if ui.button("Phantom 1.2.4").clicked() {
                        self.load_pipeline = Some(pipeline::presets::PHANTOM_1_2_4.into());
                        ui.close_menu();
                    }
                    if ui.button("Clinic").clicked() {
                        self.load_pipeline = Some(pipeline::presets::CLINIC.into());
                        ui.close_menu();
                    }
                });

                if ui.button("Save").clicked() {
                    let file = native_dialog::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .set_title("Save Pipeline")
                        .show_save_single_file();

                    if let Ok(Some(file)) = file {
                        let serialized = serde_json::to_string_pretty(&(
                            &self.pipeline,
                            &self.pipeline_edit_state,
                        ))
                        .unwrap();
                        if let Err(e) = std::fs::write(file, serialized) {
                            eprintln!("Error saving pipeline: {}", e);
                        }
                    }

                    ui.close_menu();
                }
            });
        });
    }
}
