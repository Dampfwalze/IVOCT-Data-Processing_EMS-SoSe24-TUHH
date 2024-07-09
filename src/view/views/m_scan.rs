mod gpu;
mod uis;

use gpu::{upload_b_scan_segmentation, SharedResources};
use uis::{cartesian_m_scan_ui, polar_m_scan_ui, side_m_scan_ui};

use std::{collections::HashSet, sync::Arc};

use crate::{
    cache::Cached, gui::color_maps, pipeline::nodes::diameter, queue_channel::error::RecvError,
};

use super::prelude::*;
use egui::{ComboBox, Layout};
use futures::future;
use nalgebra::DVector;
use tokio::sync::{watch, RwLock};
use types::BScanDiameter;
use wgpu::util::DeviceExt;

/// WGPU requires to specify a maximum number of textures we can bind in a
/// texture array. When we exceed this number, the remaining textures are not
/// rendered.
pub const MAX_TEXTURES: usize = 100;

pub enum InputId {
    MScan,
    BScanSegmentation,
    MScanSegmentation,
    Diameter,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => MScan,
    1 => BScanSegmentation,
    2 => MScanSegmentation,
    3 => Diameter,
});

// MARK: View

pub struct View {
    m_scan: NodeOutput,
    b_scan_segmentation: Option<NodeOutput>,
    m_scan_segmentation: Option<NodeOutput>,
    diameter: Option<NodeOutput>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    m_scan_segmentation_rx: Option<watch::Receiver<Vec<usize>>>,

    b_scan_segmentation_rx: Option<watch::Receiver<Vec<usize>>>,
    b_scan_segmentation_buffer: Option<(wgpu::Buffer, Arc<wgpu::BindGroup>)>,
    b_scan_segmentation_bind_group_layout: Arc<wgpu::BindGroupLayout>,

    diameter_rx: Option<watch::Receiver<Vec<BScanDiameter>>>,

    show_side_view: bool,
    map_idx: u32,
}

impl View {
    fn new(node_output: NodeOutput, cache: &Cache, render_state: &RenderState) -> Self {
        let renderer = render_state.renderer.read();
        let resources = renderer
            .callback_resources
            .get::<SharedResources>()
            .unwrap();

        Self {
            m_scan: node_output,
            b_scan_segmentation: None,
            m_scan_segmentation: None,
            diameter: None,
            textures_state: cache.get((node_output.node_id, node_output.output_id)),
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            bind_group_layout: resources.scan_bind_group_layout.clone(),
            m_scan_segmentation_rx: None,
            b_scan_segmentation_rx: None,
            b_scan_segmentation_buffer: None,
            b_scan_segmentation_bind_group_layout: resources
                .b_scan_segmentation_bind_group_layout
                .clone(),
            diameter_rx: None,
            show_side_view: false,
            map_idx: 26,
        }
    }
}

impl Clone for View {
    fn clone(&self) -> Self {
        Self {
            m_scan: self.m_scan.clone(),
            b_scan_segmentation: self.b_scan_segmentation.clone(),
            m_scan_segmentation: self.m_scan_segmentation.clone(),
            diameter: self.diameter.clone(),
            textures_state: self.textures_state.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            bind_group_layout: self.bind_group_layout.clone(),
            m_scan_segmentation_rx: None,
            b_scan_segmentation_rx: None,
            b_scan_segmentation_buffer: None,
            b_scan_segmentation_bind_group_layout: self
                .b_scan_segmentation_bind_group_layout
                .clone(),
            diameter_rx: None,
            show_side_view: self.show_side_view.clone(),
            map_idx: self.map_idx.clone(),
        }
    }
}

impl DataView for View {
    type InputId = InputId;

    fn init_wgpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> impl std::any::Any {
        SharedResources::new(device, queue, target_format)
    }

    fn from_node_output(
        node_output: &NodeOutput,
        pipeline: &Pipeline,
        cache: &Cache,
        render_state: &RenderState,
    ) -> Option<Self>
    where
        Self: Sized,
    {
        match PipelineDataType::from(node_output.type_id) {
            PipelineDataType::MScan => Some(Self::new(*node_output, cache, render_state)),
            PipelineDataType::BScanSegmentation => {
                let m_scan = find_m_scan_input(pipeline, node_output.node_id)?;
                Some(Self {
                    b_scan_segmentation: Some(*node_output),
                    ..Self::new(m_scan, cache, render_state)
                })
            }
            PipelineDataType::MScanSegmentation => {
                let m_scan = find_m_scan_input(pipeline, node_output.node_id)?;
                Some(Self {
                    m_scan_segmentation: Some(*node_output),
                    ..Self::new(m_scan, cache, render_state)
                })
            }
            PipelineDataType::Diameter => {
                let m_scan = find_m_scan_input(pipeline, node_output.node_id)?;
                let b_scans = find_b_scan_input(pipeline, node_output.node_id);
                let m_scan_segmentation =
                    find_m_scan_segmentation_input(pipeline, node_output.node_id);
                Some(Self {
                    diameter: Some(*node_output),
                    b_scan_segmentation: b_scans,
                    m_scan_segmentation,
                    ..Self::new(m_scan, cache, render_state)
                })
            }
            _ => None,
        }
    }

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)> {
        [
            (InputId::MScan, Some(self.m_scan)),
            (InputId::BScanSegmentation, self.b_scan_segmentation),
            (InputId::MScanSegmentation, self.m_scan_segmentation),
            (InputId::Diameter, self.diameter),
        ]
        .into_iter()
    }

    fn changed(&self, other: &Self) -> bool {
        self.m_scan != other.m_scan
    }

    fn connect(&mut self, node_output: NodeOutput, _pipeline: &Pipeline) -> bool {
        match PipelineDataType::from(node_output.type_id) {
            PipelineDataType::MScan => {
                self.m_scan = node_output;
                self.textures_state
                    .change_target((node_output.node_id, node_output.output_id));
                true
            }
            PipelineDataType::BScanSegmentation => {
                self.b_scan_segmentation = Some(node_output);
                true
            }
            PipelineDataType::MScanSegmentation => {
                self.m_scan_segmentation = Some(node_output);
                true
            }
            PipelineDataType::Diameter => {
                self.diameter = Some(node_output);
                true
            }
            _ => false,
        }
    }

    fn disconnect(&mut self, input_id: Self::InputId) -> Existence {
        match input_id {
            InputId::MScan => Existence::Destroy,
            InputId::BScanSegmentation => {
                self.b_scan_segmentation = None;
                Existence::Keep
            }
            InputId::MScanSegmentation => {
                self.m_scan_segmentation = None;
                Existence::Keep
            }
            InputId::Diameter => {
                self.diameter = None;
                Existence::Keep
            }
        }
    }

    fn create_view_task(&mut self) -> impl DataViewTask<InputId = Self::InputId, DataView = Self> {
        let (b_scan_tx, b_scan_rx) = watch::channel(Vec::new());
        let (m_scan_tx, m_scan_rx) = watch::channel(Vec::new());
        let (diameter_tx, diameter_rx) = watch::channel(Vec::new());

        self.b_scan_segmentation_rx = Some(b_scan_rx);
        self.m_scan_segmentation_rx = Some(m_scan_rx);
        self.diameter_rx = Some(diameter_rx);

        Task {
            m_scan_in: TaskInput::default(),
            b_scan_segmentation_in: TaskInput::default(),
            m_scan_segmentation_in: TaskInput::default(),
            diameter_in: TaskInput::default(),
            textures_state: self.textures_state.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            bind_group_layout: self.bind_group_layout.clone(),
            b_scan_segmentation_tx: b_scan_tx,
            m_scan_segmentation_tx: m_scan_tx,
            diameter_tx,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let textures_state = self.textures_state.read();

        let Some(textures_state) = textures_state.as_ref() else {
            ui.ctx().request_repaint();
            ui.label("Data should be here soon");
            return;
        };

        let Some(texture_bind_group) = textures_state.bind_group.as_ref() else {
            ui.ctx().request_repaint();
            ui.label("Data should be here soon");
            return;
        };

        if let Some(b_scan_segmentation) = self.b_scan_segmentation_rx.as_mut() {
            if let Ok(true) = b_scan_segmentation.has_changed() {
                let b_scan_segmentation = b_scan_segmentation.borrow_and_update();
                if b_scan_segmentation.len() > 1 {
                    upload_b_scan_segmentation(
                        &self.device,
                        &mut self.b_scan_segmentation_buffer,
                        &self.b_scan_segmentation_bind_group_layout,
                        b_scan_segmentation.as_slice(),
                    );
                }
            }
        }

        let diameters = self.diameter_rx.as_ref().map(|rx| rx.borrow());
        let diameters = diameters.as_deref().and_then(|v| match v.len() {
            0 => None,
            _ => Some(&v[..]),
        });

        let layout = Layout {
            main_dir: egui::Direction::RightToLeft,
            cross_justify: true,
            ..*ui.layout()
        };
        let response = ui
            .with_layout(layout, |ui| {
                let b_scan_segmentation =
                    self.b_scan_segmentation_rx.as_ref().map(|rx| rx.borrow());

                let m_scan_segmentation =
                    self.m_scan_segmentation_rx.as_ref().map(|rx| rx.borrow());

                let m_scan_segmentation = m_scan_segmentation
                    .as_deref()
                    .map(|v| v.as_slice())
                    .and_then(|v| match v.len() {
                        0..=2 => None,
                        _ => Some(v),
                    });

                if let Some(b_scan_segmentation) = b_scan_segmentation {
                    if b_scan_segmentation.len() > 1 {
                        cartesian_m_scan_ui(
                            ui,
                            textures_state,
                            texture_bind_group.clone(),
                            b_scan_segmentation.as_slice(),
                            m_scan_segmentation,
                            diameters,
                            self.map_idx,
                        )
                    }
                }

                let b_scan_segmentation = self
                    .b_scan_segmentation_rx
                    .as_ref()
                    .map(|rx| rx.borrow())
                    .and_then(|b| match b.len() {
                        0..=2 => None,
                        _ => Some(b),
                    });

                if let (Some(b_scan_segmentation), Some((_, bind_group)), true) = (
                    &b_scan_segmentation,
                    &self.b_scan_segmentation_buffer,
                    self.show_side_view,
                ) {
                    side_m_scan_ui(
                        ui,
                        textures_state,
                        texture_bind_group.clone(),
                        bind_group.clone(),
                        b_scan_segmentation,
                        m_scan_segmentation,
                        self.map_idx,
                    )
                } else {
                    polar_m_scan_ui(
                        ui,
                        textures_state,
                        texture_bind_group.clone(),
                        b_scan_segmentation.as_deref().map(|rx| rx.as_slice()),
                        m_scan_segmentation,
                        self.map_idx,
                    )
                }
            })
            .inner;

        ui.allocate_ui_at_rect(response.rect.expand(-5.0), |ui| {
            ui.horizontal(|ui| {
                // If there is BScanSegmentation input
                if let Some(true) = self
                    .b_scan_segmentation_rx
                    .as_ref()
                    .map(|rx| rx.borrow().len() > 1)
                {
                    let mut selected = if self.show_side_view { 1 } else { 0 };
                    ComboBox::from_id_source(ui.id().with("view_selector")).show_index(
                        ui,
                        &mut selected,
                        2,
                        |i| match i {
                            0 => "Polar View",
                            1 => "Side View",
                            _ => unreachable!(),
                        },
                    );
                    self.show_side_view = selected == 1;
                }

                let color_maps = color_maps::get_color_map_names();

                let mut map_idx = 0;
                let (category, map) = color_maps
                    .iter()
                    .find_map(|(category, maps)| {
                        if self.map_idx < map_idx + maps.len() as u32 {
                            return Some((category, maps[(self.map_idx - map_idx) as usize]));
                        } else {
                            map_idx += maps.len() as u32;
                            None
                        }
                    })
                    .unwrap();

                ui.menu_button(format!("{category}/{map}"), |ui| {
                    let mut i = 0;
                    for (category, maps) in color_maps {
                        if !ui
                            .menu_button(*category, |ui| {
                                for map in *maps {
                                    if ui.selectable_label(self.map_idx == i, *map).clicked() {
                                        self.map_idx = i;
                                        ui.close_menu();
                                    }
                                    i += 1;
                                }
                            })
                            .response
                            .context_menu_opened()
                        {
                            i += maps.len() as u32;
                        }
                    }
                })
                .response
                .on_hover_text("All color maps from Matplotlib");
            });
        });

        if textures_state.working {
            ui.ctx().request_repaint();
        }
    }
}

fn find_m_scan_input(pipeline: &Pipeline, node_id: NodeId) -> Option<NodeOutput> {
    let mut seen_nodes = HashSet::new();

    fn find_m_scan_input(
        pipeline: &Pipeline,
        node_id: NodeId,
        seen_nodes: &mut HashSet<NodeId>,
    ) -> Option<NodeOutput> {
        if seen_nodes.contains(&node_id) {
            return None;
        }

        seen_nodes.insert(node_id);
        let node = &pipeline[node_id];

        node.inputs().iter().find_map(|(_, output)| {
            if let Some(output) = output {
                if output.type_id == PipelineDataType::MScan.into() {
                    return Some(*output);
                } else {
                    return find_m_scan_input(pipeline, output.node_id, seen_nodes);
                }
            }

            None
        })
    }

    find_m_scan_input(pipeline, node_id, &mut seen_nodes)
}

fn find_b_scan_input(pipeline: &Pipeline, node_id: NodeId) -> Option<NodeOutput> {
    let mut seen_nodes = HashSet::new();

    fn find_b_scan_input(
        pipeline: &Pipeline,
        node_id: NodeId,
        seen_nodes: &mut HashSet<NodeId>,
    ) -> Option<NodeOutput> {
        if seen_nodes.contains(&node_id) {
            return None;
        }

        seen_nodes.insert(node_id);
        let node = &pipeline[node_id];

        node.inputs().iter().find_map(|(_, output)| {
            if let Some(output) = output {
                if output.type_id == PipelineDataType::BScanSegmentation.into() {
                    return Some(*output);
                } else {
                    return find_b_scan_input(pipeline, output.node_id, seen_nodes);
                }
            }

            None
        })
    }

    find_b_scan_input(pipeline, node_id, &mut seen_nodes)
}

fn find_m_scan_segmentation_input(pipeline: &Pipeline, node_id: NodeId) -> Option<NodeOutput> {
    pipeline[node_id]
        .inputs()
        .iter()
        .find_map(|(input_id, output)| {
            output.and_then(|o| match (*input_id).into() {
                diameter::InputId::Lumen => Some(o),
                _ => None,
            })
        })
}

// MARK: Task

struct Task {
    m_scan_in: TaskInput<requests::MScan>,
    b_scan_segmentation_in: TaskInput<requests::BScanSegmentation>,
    m_scan_segmentation_in: TaskInput<requests::MScanSegmentation>,
    diameter_in: TaskInput<requests::Diameter>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    b_scan_segmentation_tx: watch::Sender<Vec<usize>>,
    m_scan_segmentation_tx: watch::Sender<Vec<usize>>,
    diameter_tx: watch::Sender<Vec<BScanDiameter>>,
}

impl DataViewTask for Task {
    type InputId = InputId;
    type DataView = View;

    fn sync_view(&mut self, view: &Self::DataView) {
        self.textures_state
            .change_target((view.m_scan.node_id, view.m_scan.output_id));
    }

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            InputId::MScan => self.m_scan_in.connect(input),
            InputId::BScanSegmentation => self.b_scan_segmentation_in.connect(input),
            InputId::MScanSegmentation => self.m_scan_segmentation_in.connect(input),
            InputId::Diameter => self.diameter_in.connect(input),
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::MScan => self.m_scan_in.disconnect(),
            InputId::BScanSegmentation => self.b_scan_segmentation_in.disconnect(),
            InputId::MScanSegmentation => self.m_scan_segmentation_in.disconnect(),
            InputId::Diameter => self.diameter_in.disconnect(),
        };
    }

    fn invalidate(&mut self, cause: InvalidationCause) {
        fn invalidate_m_scan(slf: &mut Task) {
            *slf.textures_state.write() = None;
        }
        fn invalidate_sender<T>(tx: &watch::Sender<Vec<T>>) {
            let _ = tx.send_modify(|d| {
                d.clear();
            });
        }

        match cause {
            InvalidationCause::Synced => invalidate_m_scan(self),
            InvalidationCause::InputInvalidated(input_id)
            | InvalidationCause::Connected(input_id)
            | InvalidationCause::Disconnected(input_id) => match input_id.into() {
                InputId::MScan => invalidate_m_scan(self),
                InputId::BScanSegmentation => invalidate_sender(&self.b_scan_segmentation_tx),
                InputId::MScanSegmentation => invalidate_sender(&self.m_scan_segmentation_tx),
                InputId::Diameter => invalidate_sender(&self.diameter_tx),
            },
        }
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        tokio::select! {
            biased;
            Some(res) = async {
                let has_data = self.textures_state.read().is_some();
                match has_data {
                    true => None,
                    false => self.m_scan_in.request(requests::MScan).await,
                }
            } => {
                self.get_m_scan(res).await?;
            }
            Some(res) = async {
                let is_empty = self.b_scan_segmentation_tx.borrow().is_empty();
                match is_empty {
                    false => None,
                    _ => {
                        self.b_scan_segmentation_in
                            .request(requests::BScanSegmentation)
                            .await
                    }
                }
            } => {
                self.get_b_scan_segmentation(res).await?;
            }
            Some(res) = async {
                let is_empty = self.m_scan_segmentation_tx.borrow().is_empty();
                match is_empty {
                    false => None,
                    _ => {
                        self.m_scan_segmentation_in
                            .request(requests::MScanSegmentation)
                            .await
                    }
                }
            } => {
                self.get_m_scan_segmentation(res).await?;
            }
            Some(res) = async {
                let is_empty = self.diameter_tx.borrow().is_empty();
                match is_empty {
                    false => None,
                    _ => self.diameter_in.request(requests::Diameter).await
                }
            } => {
                self.get_diameter(res).await?;
            }
            _ = future::pending() => {}
        }

        Ok(())
    }
}

impl Task {
    async fn get_b_scan_segmentation(
        &mut self,
        res: requests::StreamedResponse<usize>,
    ) -> anyhow::Result<()> {
        let Some(mut rx) = res.subscribe() else {
            return Ok(());
        };

        self.b_scan_segmentation_tx.send_modify(|d| {
            d.clear();
        });

        loop {
            let data = match rx.recv().await {
                Ok(data) => data,
                Err(RecvError::Closed) => break,
                _ => return Ok(()),
            };

            self.b_scan_segmentation_tx.send_modify(|d| {
                d.push(data);
            });
        }

        Ok(())
    }

    async fn get_m_scan_segmentation(
        &mut self,
        res: requests::StreamedResponse<Arc<DVector<u32>>>,
    ) -> anyhow::Result<()> {
        let Some(mut rx) = res.subscribe() else {
            return Ok(());
        };

        self.m_scan_segmentation_tx.send_modify(|d| {
            d.clear();
        });

        loop {
            let data = match rx.recv().await {
                Ok(data) => data,
                Err(RecvError::Closed) => break,
                _ => return Ok(()),
            };

            self.m_scan_segmentation_tx.send_modify(|d| {
                d.extend(data.iter().map(|d| *d as usize));
            });
        }

        Ok(())
    }

    async fn get_diameter(
        &mut self,
        res: requests::StreamedResponse<BScanDiameter>,
    ) -> anyhow::Result<()> {
        let Some(mut rx) = res.subscribe() else {
            return Ok(());
        };

        self.diameter_tx.send_modify(|d| {
            d.clear();
        });

        loop {
            let data = match rx.recv().await {
                Ok(data) => data,
                Err(RecvError::Closed) => break,
                _ => return Ok(()),
            };

            self.diameter_tx.send_modify(|d| {
                d.push(data);
            });
        }

        Ok(())
    }

    async fn get_m_scan(&mut self, res: requests::MScanResponse) -> anyhow::Result<()> {
        {
            let mut state = self.textures_state.write();
            if state.is_none() {
                state.replace(TexturesState {
                    uploaded: Arc::new(RwLock::new(0)),
                    textures: Vec::new(),
                    bind_group: None,
                    working: true,
                    a_scan_count: res.a_scan_count,
                    a_scan_samples: res.a_scan_samples,
                });
            }
        }

        let Some(mut rx) = res.data.subscribe() else {
            return Ok(());
        };

        let uploaded = {
            let state = self.textures_state.read();
            state.as_ref().unwrap().uploaded.clone()
        };

        let mut my_uploaded = 0;

        loop {
            let data = match rx.recv().await {
                Ok(data) => data,
                Err(RecvError::Closed) => break,
                _ => return Ok(()),
            };

            let mut uploaded = uploaded.write().await;

            my_uploaded += 1;
            if my_uploaded <= *uploaded {
                continue;
            }

            // Upload data to GPU
            let device = self.device.clone();
            let queue = self.queue.clone();
            let texture = tokio::task::spawn_blocking(move || {
                let data = data.cast_rescale_par(types::DataType::U16);

                device.create_texture_with_data(
                    &queue,
                    &wgpu::TextureDescriptor {
                        label: Some("MScan Texture"),
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R16Uint,
                        mip_level_count: 1,
                        sample_count: 1,
                        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                        size: wgpu::Extent3d {
                            width: res.a_scan_samples as u32,
                            height: data.ncols() as u32,
                            depth_or_array_layers: 1,
                        },
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    data.as_u8_slice(),
                )
            })
            .await?;

            let mut texture_state = self.textures_state.write();
            let Some(texture_state) = texture_state.as_mut() else {
                return Ok(());
            };

            let textures = &mut texture_state.textures;

            textures.push(texture.create_view(&wgpu::TextureViewDescriptor::default()));

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("MScan Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(
                        &textures
                            .iter()
                            .take(MAX_TEXTURES)
                            .map(|t| t)
                            .collect::<Vec<_>>(),
                    ),
                }],
            });

            texture_state.bind_group = Some(Arc::new(bind_group));

            *uploaded = my_uploaded;
        }

        self.textures_state
            .write()
            .as_mut()
            .map(|state| state.working = false);

        Ok(())
    }
}

// MARK: TexturesState

#[derive(Default)]
struct TexturesState {
    uploaded: Arc<RwLock<usize>>,
    textures: Vec<wgpu::TextureView>,
    bind_group: Option<Arc<wgpu::BindGroup>>,
    working: bool,
    a_scan_count: usize,
    a_scan_samples: usize,
}
