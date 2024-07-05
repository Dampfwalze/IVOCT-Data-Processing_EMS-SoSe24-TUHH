use std::{collections::HashSet, num::NonZeroU32, sync::Arc};

use crate::{cache::Cached, gui::widgets::PanZoomRect, queue_channel::error::RecvError};

use super::prelude::*;
use egui::{pos2, vec2, Color32, ComboBox, Layout, Rect, Response, Sense, Stroke, Vec2};
use futures::future;
use tokio::sync::{watch, RwLock};
use wgpu::{util::DeviceExt, PushConstantRange};

/// WGPU requires to specify a maximum number of textures we can bind in a
/// texture array. When we exceed this number, the remaining textures are not
/// rendered.
pub const MAX_TEXTURES: usize = 100;

pub enum InputId {
    MScan,
    BScanSegmentation,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => MScan,
    1 => BScanSegmentation,
});

// MARK: View

pub struct View {
    m_scan: NodeOutput,
    b_scan_segmentation: Option<NodeOutput>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    b_scan_segmentation_rx: Option<watch::Receiver<Vec<usize>>>,
    b_scan_segmentation_buffer: Option<(wgpu::Buffer, Arc<wgpu::BindGroup>)>,
    b_scan_segmentation_bind_group_layout: Arc<wgpu::BindGroupLayout>,

    show_side_view: bool,
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
            textures_state: cache.get((node_output.node_id, node_output.output_id)),
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            bind_group_layout: resources.bind_group_layout.clone(),
            b_scan_segmentation_rx: None,
            b_scan_segmentation_buffer: None,
            b_scan_segmentation_bind_group_layout: resources
                .b_scan_segmentation_bind_group_layout
                .clone(),
            show_side_view: false,
        }
    }
}

impl Clone for View {
    fn clone(&self) -> Self {
        Self {
            m_scan: self.m_scan.clone(),
            b_scan_segmentation: self.b_scan_segmentation.clone(),
            textures_state: self.textures_state.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            bind_group_layout: self.bind_group_layout.clone(),
            b_scan_segmentation_rx: self.b_scan_segmentation_rx.clone(),
            b_scan_segmentation_buffer: None,
            b_scan_segmentation_bind_group_layout: self
                .b_scan_segmentation_bind_group_layout
                .clone(),
            show_side_view: self.show_side_view.clone(),
        }
    }
}

impl DataView for View {
    type InputId = InputId;

    fn init_wgpu(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> impl std::any::Any {
        SharedResources::new(device, target_format)
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
            _ => None,
        }
    }

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)> {
        [
            (InputId::MScan, Some(self.m_scan)),
            (InputId::BScanSegmentation, self.b_scan_segmentation),
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
        }
    }

    fn create_view_task(&mut self) -> impl DataViewTask<InputId = Self::InputId, DataView = Self> {
        let (b_scan_tx, b_scan_rx) = watch::channel(Vec::new());

        self.b_scan_segmentation_rx = Some(b_scan_rx);

        Task {
            m_scan_in: TaskInput::default(),
            b_scan_segmentation_in: TaskInput::default(),
            textures_state: self.textures_state.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            bind_group_layout: self.bind_group_layout.clone(),
            b_scan_segmentation_tx: b_scan_tx,
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

        let layout = Layout {
            main_dir: egui::Direction::RightToLeft,
            cross_justify: true,
            ..*ui.layout()
        };
        let response = ui
            .with_layout(layout, |ui| {
                let b_scan_segmentation =
                    self.b_scan_segmentation_rx.as_ref().map(|rx| rx.borrow());

                if let Some(b_scan_segmentation) = b_scan_segmentation {
                    if b_scan_segmentation.len() > 1 {
                        cartesian_m_scan_ui(
                            ui,
                            textures_state,
                            texture_bind_group.clone(),
                            b_scan_segmentation.as_slice(),
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
                        b_scan_segmentation.len(),
                    )
                } else {
                    polar_m_scan_ui(
                        ui,
                        textures_state,
                        texture_bind_group.clone(),
                        b_scan_segmentation.as_deref().map(|rx| rx.as_slice()),
                    )
                }
            })
            .inner;

        // If there is BScanSegmentation input
        if let Some(true) = self
            .b_scan_segmentation_rx
            .as_ref()
            .map(|rx| rx.borrow().len() > 1)
        {
            ui.allocate_ui_at_rect(response.rect.expand(-5.0), |ui| {
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
            });
        }

        if textures_state.working {
            ui.ctx().request_repaint();
        }
    }
}

fn polar_m_scan_ui(
    ui: &mut egui::Ui,
    textures_state: &TexturesState,
    texture_bind_group: Arc<wgpu::BindGroup>,
    b_scan_segmentation: Option<&[usize]>,
) -> egui::Response {
    PanZoomRect::new()
        .zoom_y(false)
        .min_zoom(1.0)
        .show(ui, |ui, viewport, n_viewport| {
            let response = ui.allocate_rect(ui.max_rect(), Sense::hover());

            let gpu_viewport = Rect::from_min_max(
                n_viewport.min * 2.0 - Vec2::splat(1.0),
                n_viewport.max * 2.0 - Vec2::splat(1.0),
            );

            ui.painter()
                .add(eframe::egui_wgpu::Callback::new_paint_callback(
                    response.rect,
                    PolarViewPaintCallback {
                        texture_bind_group,
                        texture_count: textures_state.textures.len(),
                        a_scan_count: textures_state.a_scan_count,
                        rect: gpu_viewport,
                    },
                ));

            if let Some(b_scan_segmentation) = b_scan_segmentation {
                for b_scan in b_scan_segmentation {
                    let x = (*b_scan as f32) / (textures_state.a_scan_count as f32);
                    let x = x * viewport.width() + viewport.min.x;

                    ui.painter().line_segment(
                        [pos2(x, viewport.min.y), pos2(x, viewport.max.y)],
                        Stroke::new(1.0, egui::Color32::BLUE),
                    );
                }
            }
        })
        .response
}

fn cartesian_m_scan_ui(
    ui: &mut egui::Ui,
    textures_state: &TexturesState,
    texture_bind_group: Arc<wgpu::BindGroup>,
    b_scan_segmentation: &[usize],
) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(ui.available_height().min(ui.available_width())),
        Sense::hover(),
    );

    let current_b_scan = get_scroll_value::<true>(
        ui,
        "current_b_scan",
        &response,
        b_scan_segmentation.len() as isize,
    );

    ui.painter()
        .add(eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            CartesianViewPaintCallback {
                texture_bind_group,
                texture_count: textures_state.textures.len(),
                b_scan_start: b_scan_segmentation[current_b_scan],
                b_scan_end: b_scan_segmentation[current_b_scan + 1],
                rect: Rect::from_min_max(Vec2::splat(-1.0).to_pos2(), Vec2::splat(1.0).to_pos2()),
            },
        ));

    // Draw current_rotation line
    let current_rotation = ui
        .data(|d| d.get_temp::<isize>(ui.id().with("current_rotation")))
        .unwrap_or(0) as f32
        / 100.0;

    let center = rect.center();
    let vec = rect.width() / 2.0 * Vec2::angled(current_rotation * std::f32::consts::TAU);
    let vec = vec2(-vec.y, vec.x);

    ui.painter().line_segment(
        [center + vec, center + 0.8 * vec],
        Stroke::new(1.0, Color32::BLUE),
    );
    ui.painter().line_segment(
        [center - vec, center - 0.8 * vec],
        Stroke::new(1.0, Color32::BLUE),
    );
}

fn get_scroll_value<const CLAMP: bool>(
    ui: &mut egui::Ui,
    id: &str,
    response: &Response,
    count: isize,
) -> usize {
    if count <= 2 {
        return 0;
    }

    let id = ui.id().with(id);

    let mut current = ui.data(|d| d.get_temp::<isize>(id)).unwrap_or(0);

    if response.hovered() {
        let scroll_delta = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match *e {
                    egui::Event::MouseWheel { delta, .. } => Some((delta.x + delta.y) as isize),
                    _ => None,
                })
                .sum::<isize>()
        });

        current += scroll_delta;
    }

    if CLAMP {
        current = current.clamp(0, count - 2);
    } else {
        current = (current + count) % count;
    }

    ui.data_mut(|d| d.insert_temp(id, current));

    current as usize
}

fn side_m_scan_ui(
    ui: &mut egui::Ui,
    textures_state: &TexturesState,
    texture_bind_group: Arc<wgpu::BindGroup>,
    b_scan_bind_group: Arc<wgpu::BindGroup>,
    b_scan_count: usize,
) -> egui::Response {
    let response = ui.allocate_response(ui.available_size(), Sense::hover());

    let current_rotation =
        get_scroll_value::<false>(ui, "current_rotation", &response, 100) as f32 / 100.0;

    ui.painter()
        .add(eframe::egui_wgpu::Callback::new_paint_callback(
            response.rect,
            SideViewPaintCallback {
                b_scan_bind_group,
                texture_bind_group,
                texture_count: textures_state.textures.len(),
                rect: Rect::from_min_max(Vec2::splat(-1.0).to_pos2(), Vec2::splat(1.0).to_pos2()),
                view_rotation: current_rotation,
            },
        ));

    // Draw current_b_scan line
    let current_b_scan = ui
        .data(|d| d.get_temp::<isize>(ui.id().with("current_b_scan")))
        .unwrap_or(0) as f32;

    let rect = response.rect;
    let x = rect.left() + rect.width() * (current_b_scan + 0.5) / (b_scan_count - 1) as f32;
    ui.painter().line_segment(
        [
            pos2(x, rect.top()),
            pos2(x, rect.top() + rect.height() * 0.1),
        ],
        Stroke::new(1.0, Color32::BLUE),
    );
    ui.painter().line_segment(
        [
            pos2(x, rect.bottom()),
            pos2(x, rect.bottom() - rect.height() * 0.1),
        ],
        Stroke::new(1.0, Color32::BLUE),
    );

    response
}

fn upload_b_scan_segmentation(
    device: &wgpu::Device,
    buffer: &mut Option<(wgpu::Buffer, Arc<wgpu::BindGroup>)>,
    bind_group_layout: &wgpu::BindGroupLayout,
    data: &[usize],
) {
    let data = data.iter().map(|d| *d as u32).collect::<Vec<_>>();

    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("BScan Segmentation Buffer"),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("BScan Segmentation Bind Group"),
        layout: bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buf,
                offset: 0,
                size: None,
            }),
        }],
    });

    *buffer = Some((buf, Arc::new(bind_group)));
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

// MARK: PolarViewPaintCallback

struct PolarViewPaintCallback {
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    a_scan_count: usize,
    rect: Rect,
}

impl eframe::egui_wgpu::CallbackTrait for PolarViewPaintCallback {
    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a eframe::egui_wgpu::CallbackResources,
    ) {
        let resources = callback_resources.get::<SharedResources>().unwrap();

        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        #[repr(C)]
        struct Constants {
            tex_count: u32,
            a_scan_count: u32,
        }

        render_pass.set_pipeline(&resources.polar_view_pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX,
            0,
            bytemuck::cast_slice(&[
                self.rect.min.x,
                self.rect.min.y,
                self.rect.max.x,
                self.rect.max.y,
            ]),
        );
        render_pass.set_push_constants(
            wgpu::ShaderStages::FRAGMENT,
            16,
            bytemuck::cast_slice(&[Constants {
                tex_count: self.texture_count.min(MAX_TEXTURES) as u32,
                a_scan_count: self.a_scan_count as u32,
            }]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: CartesianViewPaintCallback

struct CartesianViewPaintCallback {
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    b_scan_start: usize,
    b_scan_end: usize,
    rect: Rect,
}

impl eframe::egui_wgpu::CallbackTrait for CartesianViewPaintCallback {
    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a eframe::egui_wgpu::CallbackResources,
    ) {
        let resources = callback_resources.get::<SharedResources>().unwrap();

        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        #[repr(C)]
        struct Constants {
            tex_count: u32,
            b_scan_start: u32,
            b_scan_end: u32,
        }

        render_pass.set_pipeline(&resources.cartesian_view_pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX,
            0,
            bytemuck::cast_slice(&[
                self.rect.min.x,
                self.rect.min.y,
                self.rect.max.x,
                self.rect.max.y,
            ]),
        );
        render_pass.set_push_constants(
            wgpu::ShaderStages::FRAGMENT,
            16,
            bytemuck::cast_slice(&[Constants {
                tex_count: self.texture_count.min(MAX_TEXTURES) as u32,
                b_scan_start: self.b_scan_start as u32,
                b_scan_end: self.b_scan_end as u32,
            }]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: SideViewPaintCallback

struct SideViewPaintCallback {
    b_scan_bind_group: Arc<wgpu::BindGroup>,
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    view_rotation: f32,
    rect: Rect,
}

impl eframe::egui_wgpu::CallbackTrait for SideViewPaintCallback {
    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a eframe::egui_wgpu::CallbackResources,
    ) {
        let resources = callback_resources.get::<SharedResources>().unwrap();

        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        #[repr(C)]
        struct Constants {
            tex_count: u32,
            view_rot: f32,
        }

        render_pass.set_pipeline(&resources.side_view_pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &self.b_scan_bind_group, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX,
            0,
            bytemuck::cast_slice(&[
                self.rect.min.x,
                self.rect.min.y,
                self.rect.max.x,
                self.rect.max.y,
            ]),
        );
        render_pass.set_push_constants(
            wgpu::ShaderStages::FRAGMENT,
            16,
            bytemuck::cast_slice(&[Constants {
                tex_count: self.texture_count.min(MAX_TEXTURES) as u32,
                view_rot: self.view_rotation,
            }]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: Task

struct Task {
    m_scan_in: TaskInput<requests::MScan>,
    b_scan_segmentation_in: TaskInput<requests::BScanSegmentation>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    b_scan_segmentation_tx: watch::Sender<Vec<usize>>,
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
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::MScan => self.m_scan_in.disconnect(),
            InputId::BScanSegmentation => self.b_scan_segmentation_in.disconnect(),
        };
    }

    fn invalidate(&mut self, cause: InvalidationCause) {
        fn invalidate_m_scan(slf: &mut Task) {
            *slf.textures_state.write() = None;
        }
        fn invalidate_b_scan_segmentation(slf: &mut Task) {
            let _ = slf.b_scan_segmentation_tx.send_modify(|d| {
                d.clear();
            });
        }

        match cause {
            InvalidationCause::Synced => invalidate_m_scan(self),
            InvalidationCause::InputInvalidated(input_id) => match input_id.into() {
                InputId::MScan => invalidate_m_scan(self),
                InputId::BScanSegmentation => invalidate_b_scan_segmentation(self),
            },
            _ => {}
        }
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        tokio::select! {
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
}

// MARK: SharedResources

struct SharedResources {
    polar_view_pipeline: wgpu::RenderPipeline,
    cartesian_view_pipeline: wgpu::RenderPipeline,
    side_view_pipeline: wgpu::RenderPipeline,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,
    b_scan_segmentation_bind_group_layout: Arc<wgpu::BindGroupLayout>,
}

impl SharedResources {
    fn new(device: &wgpu::Device, target_format: &wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("MScan Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: NonZeroU32::new(MAX_TEXTURES as u32),
            }],
        });

        let b_scan_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("BScan Segmentation Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let polar_view_pipeline =
            Self::create_polar_view_pipeline(device, target_format, &bind_group_layout);

        let cartesian_view_pipeline =
            Self::create_cartesian_view_pipeline(device, target_format, &bind_group_layout);

        let side_view_pipeline = Self::create_side_view_pipeline(
            device,
            target_format,
            &bind_group_layout,
            &b_scan_bind_group_layout,
        );

        Self {
            polar_view_pipeline,
            cartesian_view_pipeline,
            side_view_pipeline,
            bind_group_layout: Arc::new(bind_group_layout),
            b_scan_segmentation_bind_group_layout: Arc::new(b_scan_bind_group_layout),
        }
    }

    fn create_polar_view_pipeline(
        device: &wgpu::Device,
        target_format: &wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Polar Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[
                PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..24,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("m_scan.wgsl"));

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Polar Render Pipeline"),
            layout: Some(&pipeline_layout),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "polar_fs_main",
                targets: &[Some((*target_format).into())],
            }),
            multiview: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
        })
    }

    fn create_cartesian_view_pipeline(
        device: &wgpu::Device,
        target_format: &wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Cartesian Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[
                PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..28,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("m_scan.wgsl"));

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Cartesian Render Pipeline"),
            layout: Some(&pipeline_layout),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "cartesian_fs_main",
                targets: &[Some((*target_format).into())],
            }),
            multiview: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
        })
    }

    fn create_side_view_pipeline(
        device: &wgpu::Device,
        target_format: &wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
        b_scan_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Polar Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &b_scan_bind_group_layout],
            push_constant_ranges: &[
                PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..24,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("m_scan.wgsl"));

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Polar Render Pipeline"),
            layout: Some(&pipeline_layout),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "side_fs_main",
                targets: &[Some((*target_format).into())],
            }),
            multiview: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
        })
    }
}
