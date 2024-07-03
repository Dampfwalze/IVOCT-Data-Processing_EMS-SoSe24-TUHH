use std::{collections::HashSet, num::NonZeroU32, sync::Arc};

use crate::{cache::Cached, gui::widgets::PanZoomRect, queue_channel::error::RecvError};

use super::prelude::*;
use egui::{pos2, Rect, Sense, Stroke, Vec2};
use futures::future;
use tokio::sync::{watch, RwLock};
use wgpu::{util::DeviceExt, PushConstantRange};

pub enum InputId {
    MScan,
    BScanSegmentation,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => MScan,
    1 => BScanSegmentation,
});

// MARK: View

#[derive(Clone)]
pub struct View {
    m_scan: NodeOutput,
    b_scan_segmentation: Option<NodeOutput>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    b_scan_segmentation_rx: Option<watch::Receiver<Vec<usize>>>,
}

impl View {
    fn new(node_output: NodeOutput, cache: &Cache, render_state: &RenderState) -> Self {
        Self {
            m_scan: node_output,
            b_scan_segmentation: None,
            textures_state: cache.get((node_output.node_id, node_output.output_id)),
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            bind_group_layout: render_state
                .renderer
                .read()
                .callback_resources
                .get::<SharedResources>()
                .unwrap()
                .bind_group_layout
                .clone(),
            b_scan_segmentation_rx: None,
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
                        PaintCallback {
                            texture_bind_group: texture_bind_group.clone(),
                            texture_count: textures_state.textures.len(),
                            a_scan_count: textures_state.a_scan_count,
                            rect: gpu_viewport,
                        },
                    ));

                if let Some(b_scan_segmentation_rx) = self.b_scan_segmentation_rx.as_ref() {
                    let b_scan_segmentation = b_scan_segmentation_rx.borrow();

                    for b_scan in b_scan_segmentation.iter() {
                        let x = (*b_scan as f32) / (textures_state.a_scan_count as f32);
                        let x = x * viewport.width() + viewport.min.x;

                        ui.painter().line_segment(
                            [pos2(x, viewport.min.y), pos2(x, viewport.max.y)],
                            Stroke::new(1.0, egui::Color32::BLUE),
                        );
                    }
                }
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

// MARK: PaintCallback

struct PaintCallback {
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    a_scan_count: usize,
    rect: Rect,
}

impl eframe::egui_wgpu::CallbackTrait for PaintCallback {
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
            rect: [f32; 4],
            texture_count: u32,
        }

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::all(),
            0,
            bytemuck::cast_slice(&[Constants {
                rect: [
                    self.rect.min.x,
                    self.rect.min.y,
                    self.rect.max.x,
                    self.rect.max.y,
                ],
                texture_count: self.texture_count as u32,
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
                        &textures.iter().map(|t| t).collect::<Vec<_>>(),
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
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,
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
                count: NonZeroU32::new(100),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[PushConstantRange {
                stages: wgpu::ShaderStages::all(),
                range: 0..20,
            }],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("m_scan.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Render Pipeline"),
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
                entry_point: "fs_main",
                targets: &[Some((*target_format).into())],
            }),
            multiview: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
        });

        Self {
            pipeline,
            bind_group_layout: Arc::new(bind_group_layout),
        }
    }
}
