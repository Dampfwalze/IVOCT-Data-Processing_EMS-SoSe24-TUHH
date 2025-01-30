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

pub struct IVOCTApp {
    /// High level pipeline description.
    pipeline: pipeline::Pipeline,
    /// Editing specific information (Where are the nodes placed).
    pipeline_edit_state: NodeGraphEditState,
    /// System responsible for executing the pipeline, described py [pipeline].
    pipeline_executor: pipeline::PipelineExecutor,

    /// High level description of data views (Views that show data from the pipeline).
    data_views_state: DataViewsState,
    /// System responsible for modifying [data_views_state], hooking them into
    /// the pipeline and creating tabs in the UI for them.
    data_views_manager: DataViewsManager,
    /// System responsible for execution of the data views.
    data_views_executor: ViewsExecutor,

    /// States of all tabs in the UI.
    dock_state: DockState,

    /// Key-value cache to reduce redundancy (Mostly when uploading large
    /// resources to the GPU, for example when multiple views show partly the
    /// same data).
    cache: Cache,

    /// The node that got double clicked by the User.
    interacted_node: Option<NodeId>,

    /// Whether and the pipeline to load (JSON). Set in [pipeline_menu_bar],
    /// used in [update].
    load_pipeline: Option<Cow<'static, str>>,
}

impl IVOCTApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Whether and the pipeline the user had open in the last session (JSON)
        let pipeline_json = cc.storage.unwrap().get_string("user_pipeline");

        let pipeline_json: Cow<_> = match pipeline_json {
            Some(json) => json.into(),
            // Use phantom1_1_3 by default
            None => pipeline::presets::PHANTOM_1_1_3.into(),
        };

        let (pipeline, state) = Self::load_pipeline(&pipeline_json);

        IVOCTApp {
            pipeline,
            pipeline_edit_state: state,
            pipeline_executor: pipeline::PipelineExecutor::new(),
            data_views_state: DataViewsState::new(),
            data_views_manager: DataViewsManagerBuilder::new(
                &cc.wgpu_render_state.as_ref().unwrap(),
            )
            // Add all available views, so the DataViewsManager can create them
            .with_view::<views::data_vector::View>()
            .with_view::<views::m_scan::View>()
            .with_view::<views::mesh::View>()
            .build(),
            data_views_executor: ViewsExecutor::new(),
            dock_state: DockState::new(),
            cache: Cache::new(),
            interacted_node: None,
            load_pipeline: None,
        }
    }

    fn load_pipeline(pipeline_json: &str) -> (pipeline::Pipeline, NodeGraphEditState) {
        let (mut pipeline, state) = serde_json::from_str(pipeline_json).unwrap_or_else(|e| {
            eprintln!("Error loading pipeline: {}", e);
            (pipeline::Pipeline::new(), NodeGraphEditState::new())
        });

        // Clear all paths that do not exist
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

impl eframe::App for IVOCTApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.interacted_node = None;

        // Satisfy Borrow Checker: Move dock_state onto the current stack frame
        let mut dock_state = mem::replace(&mut self.dock_state, DockState::new());

        // Render all tabs
        egui_dock::DockArea::new(&mut dock_state)
            .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
            .show(ctx, self);

        // Move dock_state back into the app struct
        self.dock_state = dock_state;

        // Update data view high level description (Create new, reconnect, or
        // delete)
        self.data_views_manager.update(
            &mut self.data_views_state,
            &mut self.pipeline,
            &mut self.dock_state,
            self.interacted_node,
            &self.cache,
            frame.wgpu_render_state().unwrap(),
            ctx.input(|i| i.modifiers.ctrl),
        );

        // Merge differences between high level pipeline description and
        // execution system
        self.pipeline_executor.update(&mut self.pipeline);

        // Same for data views. They might connect into the pipeline_executor
        self.data_views_executor
            .update(&mut self.data_views_state, &self.pipeline_executor);

        // User requested to load new pipeline in this frame
        if let Some(json) = self.load_pipeline.take() {
            let (pipeline, state) = Self::load_pipeline(&json);
            self.set_pipeline(pipeline, state);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Called in regular intervals

        println!("Saving");

        let pipeline = serde_json::to_string(&(&self.pipeline, &self.pipeline_edit_state)).unwrap();

        storage.set_string("user_pipeline", pipeline)
    }
}

// MARK: impl TabViewer

impl egui_dock::TabViewer for IVOCTApp {
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

                // User double clicked a node
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

impl IVOCTApp {
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
