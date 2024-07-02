use std::{num::NonZeroU32, sync::Arc};

use crate::{cache::Cached, queue_channel::error::RecvError};

use super::prelude::*;
use egui::Sense;
use futures::future;
use tokio::sync::RwLock;
use wgpu::{util::DeviceExt, PushConstantRange};

// MARK: View

#[derive(Clone)]
pub struct View {
    input: NodeOutput,
    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,
}

impl DataView for View {
    type InputId = InputIdSingle;

    fn init_wgpu(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> impl std::any::Any {
        SharedResources::new(device, target_format)
    }

    fn from_node_output(
        node_output: &NodeOutput,
        _pipeline: &Pipeline,
        cache: &Cache,
        render_state: &RenderState,
    ) -> Option<Self>
    where
        Self: Sized,
    {
        if node_output.type_id == PipelineDataType::MScan.into() {
            Some(Self {
                input: *node_output,
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
            })
        } else {
            None
        }
    }

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, Some(self.input)))
    }

    fn changed(&self, other: &Self) -> bool {
        self.input != other.input
    }

    fn connect(&mut self, node_output: NodeOutput, _pipeline: &Pipeline) -> bool {
        if node_output.type_id == PipelineDataType::MScan.into() {
            self.input = node_output;
            true
        } else {
            false
        }
    }

    fn disconnect(&mut self, _input_id: Self::InputId) -> Existence {
        Existence::Destroy
    }

    fn create_view_task(&mut self) -> impl DataViewTask<InputId = Self::InputId, DataView = Self> {
        Task {
            input: TaskInput::default(),
            textures_state: self.textures_state.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            bind_group_layout: self.bind_group_layout.clone(),
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

        let response = ui.allocate_rect(ui.max_rect(), Sense::hover());

        ui.painter()
            .add(eframe::egui_wgpu::Callback::new_paint_callback(
                response.rect,
                PaintCallback {
                    texture_bind_group: texture_bind_group.clone(),
                    texture_count: textures_state.textures.len(),
                    a_scan_count: textures_state.a_scan_count,
                },
            ));

        if textures_state.working {
            ui.ctx().request_repaint();
        }
    }
}

// MARK: PaintCallback

struct PaintCallback {
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    a_scan_count: usize,
}

impl eframe::egui_wgpu::CallbackTrait for PaintCallback {
    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a eframe::egui_wgpu::CallbackResources,
    ) {
        let resources = callback_resources.get::<SharedResources>().unwrap();

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_push_constants(
            wgpu::ShaderStages::all(),
            0,
            bytemuck::cast_slice(&[self.texture_count as u32]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: Task

struct Task {
    input: TaskInput<requests::MScan>,
    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,
}

impl DataViewTask for Task {
    type InputId = InputIdSingle;
    type DataView = View;

    fn sync_view(&mut self, view: &Self::DataView) {
        self.textures_state
            .change_target((view.input.node_id, view.input.output_id));
    }

    fn connect(&mut self, _input_id: Self::InputId, input: &mut ConnectionHandle) {
        self.input.connect(input);
    }

    fn disconnect(&mut self, _input_id: Self::InputId) {
        self.input.disconnect();
    }

    fn invalidate(&mut self, cause: InvalidationCause) {
        match cause {
            InvalidationCause::InputInvalidated | InvalidationCause::Synced => {
                *self.textures_state.write() = None;
            }
            _ => {}
        }
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        if self.textures_state.read().is_some() {
            let () = future::pending().await;
        }

        let Some(res) = self.input.request(requests::MScan).await else {
            return Err(anyhow::anyhow!("No MScan data"));
        };

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
                range: 0..4,
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
