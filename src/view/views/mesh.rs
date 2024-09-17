use std::sync::Arc;

use egui::Sense;
use futures::future;
use nalgebra::{Matrix4, Perspective3, Unit, Vector3};
use tokio::sync::RwLock;
use types::LumenVertex;

use crate::{cache::Cached, queue_channel::error::RecvError};

use super::prelude::*;

// MARK: View

/// Renders a [types::LumenMesh].
#[derive(Clone, Debug)]
pub struct View {
    mesh: NodeOutput,

    mesh_state: Cached<Option<MeshState>>,
    device: Arc<wgpu::Device>,

    camera: Camera,
}

impl DataView for View {
    type InputId = InputIdSingle;

    fn init_wgpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> impl std::any::Any + Send + Sync + 'static {
        SharedResources::new(device, queue, target_format)
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
        if node_output.type_id == PipelineDataType::Mesh.into() {
            Some(Self {
                mesh: node_output.clone(),
                mesh_state: cache.get(node_output),
                device: render_state.device.clone(),
                camera: Camera::new(),
            })
        } else {
            None
        }
    }

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, Some(self.mesh)))
    }

    fn changed(&self, other: &Self) -> bool {
        self.mesh != other.mesh
    }

    fn connect(&mut self, node_output: NodeOutput, _pipeline: &Pipeline) -> bool {
        if node_output.type_id == PipelineDataType::Mesh.into() {
            self.mesh = node_output;
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
            mesh: TaskInput::default(),
            mesh_state: self.mesh_state.clone(),
            device: self.device.clone(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let mesh_state = self.mesh_state.read();

        let Some(mesh_state) = mesh_state.as_ref() else {
            ui.ctx().request_repaint();
            ui.label("Data should be here soon");
            return;
        };

        let (rect, response) =
            ui.allocate_exact_size(ui.available_size_before_wrap(), Sense::drag());

        self.camera.update(ui.ctx(), response);

        ui.painter()
            .add(eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                PaintCallback {
                    buffers: mesh_state.meshes.clone(),
                    mvp_matrix: Perspective3::new(
                        rect.width() / rect.height(),
                        std::f32::consts::PI * 0.5,
                        0.001,
                        100.0,
                    )
                    .as_matrix()
                        * self.camera.view_matrix(),
                },
            ));
    }
}

// MARK: Camera

#[derive(Debug, Clone, Copy, PartialEq)]
struct Camera {
    position: Vector3<f32>,
    dir: Unit<Vector3<f32>>,
    up: Unit<Vector3<f32>>,
}

impl Camera {
    fn new() -> Self {
        Self {
            position: Vector3::new(0.0, 0.0, 0.0),
            dir: Unit::new_normalize(Vector3::new(1.0, 0.0, 0.0)),
            up: Unit::new_normalize(Vector3::new(0.0, 1.0, 0.0)),
        }
    }

    fn view_matrix(&self) -> Matrix4<f32> {
        nalgebra::Matrix4::look_at_rh(
            &(self.position).into(),
            &(self.position + self.dir.into_inner()).into(),
            &self.up,
        )
    }

    fn update(&mut self, ctx: &egui::Context, response: egui::Response) {
        if response.dragged() {
            let delta = ctx.input(|r| r.pointer.delta());

            let delta = Vector3::new(-delta.x, -delta.y, 0.0) * 0.01;

            self.dir = nalgebra::Rotation3::from_axis_angle(&self.up, delta.x)
                * nalgebra::Rotation3::from_axis_angle(
                    &Unit::new_normalize(self.dir.into_inner().cross(&self.up.into_inner())),
                    delta.y,
                )
                * self.dir;

            let (forward, backward, left, right, up, down) = ctx.input(|r| {
                (
                    r.key_down(egui::Key::W),
                    r.key_down(egui::Key::S),
                    r.key_down(egui::Key::A),
                    r.key_down(egui::Key::D),
                    r.key_down(egui::Key::Space) || r.key_down(egui::Key::E),
                    r.modifiers.shift || r.key_down(egui::Key::Q),
                )
            });

            let speed = if ctx.input(|r| r.modifiers.ctrl) {
                0.05
            } else {
                0.02
            };

            if forward {
                let dir = Vector3::new(self.dir.x, 0.0, self.dir.z).normalize();
                self.position += dir * speed;
            }
            if backward {
                let dir = Vector3::new(self.dir.x, 0.0, self.dir.z).normalize();
                self.position -= dir * speed;
            }
            if left {
                self.position -= self.dir.cross(&self.up).normalize() * speed;
            }
            if right {
                self.position += self.dir.cross(&self.up).normalize() * speed;
            }
            if up {
                self.position += self.up.into_inner() * speed;
            }
            if down {
                self.position -= self.up.into_inner() * speed;
            }

            ctx.request_repaint();
        }

        if let Some(pos) = response.hover_pos() {
            let scroll = ctx.input(|r| r.smooth_scroll_delta.x + r.smooth_scroll_delta.y);
            if response.rect.contains(pos) && scroll.abs() > 0.0 {
                let dir = self.dir.into_inner();
                self.position += dir * scroll * 0.03;
                ctx.request_repaint();
            }
        }
    }
}

// MARK: PaintCallback

struct PaintCallback {
    buffers: Vec<Arc<(wgpu::Buffer, wgpu::Buffer)>>,
    mvp_matrix: Matrix4<f32>,
}

impl eframe::egui_wgpu::CallbackTrait for PaintCallback {
    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a eframe::egui_wgpu::CallbackResources,
    ) {
        let resources = &callback_resources.get::<SharedResources>().unwrap();

        render_pass.set_pipeline(&resources.pipeline);

        for (vertex_buffer, index_buffer) in self.buffers.iter().map(Arc::as_ref) {
            let index_count = index_buffer.size() as u32 / std::mem::size_of::<u32>() as u32;
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.set_push_constants(
                wgpu::ShaderStages::VERTEX,
                0,
                bytemuck::cast_slice(self.mvp_matrix.as_slice()),
            );
            render_pass.draw_indexed(0..index_count, 0, 0..1);
        }
    }
}

// MARK: Task

struct Task {
    mesh: TaskInput<requests::Mesh>,

    mesh_state: Cached<Option<MeshState>>,
    device: Arc<wgpu::Device>,
}

impl DataViewTask for Task {
    type InputId = InputIdSingle;
    type DataView = View;

    fn sync_view(&mut self, view: &Self::DataView) {
        self.mesh_state
            .change_target((view.mesh.node_id, view.mesh.output_id));
    }

    fn connect(&mut self, _input_id: Self::InputId, input: &mut ConnectionHandle) {
        self.mesh.connect(input);
    }

    fn disconnect(&mut self, _input_id: Self::InputId) {
        self.mesh.disconnect();
    }

    fn invalidate(&mut self, _cause: InvalidationCause) {
        *self.mesh_state.write() = None;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        use wgpu::util::DeviceExt;

        if self.mesh_state.read().is_some() {
            let () = future::pending().await;
        }

        let Some(res) = self.mesh.request(requests::Mesh).await else {
            return future::pending().await;
        };

        {
            let mut state = self.mesh_state.write();
            if state.is_none() {
                state.replace(MeshState {
                    uploaded: Arc::new(RwLock::new(0)),
                    meshes: Vec::new(),
                    working: true,
                });
            }
        }

        let Some(mut rx) = res.subscribe() else {
            return future::pending().await;
        };

        let uploaded = {
            let state = self.mesh_state.read();
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

            let (vertex_buffer, index_buffer) = tokio::task::spawn_blocking(move || {
                let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh Vertex Buffer"),
                    contents: bytemuck::cast_slice(&data.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh Index Buffer"),
                    contents: bytemuck::cast_slice(&data.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                (vertex, index)
            })
            .await?;

            let mut mesh_state = self.mesh_state.write();
            let Some(mesh_state) = mesh_state.as_mut() else {
                return Ok(());
            };

            let meshes = &mut mesh_state.meshes;

            meshes.push(Arc::new((vertex_buffer, index_buffer)));

            *uploaded = my_uploaded;
        }

        self.mesh_state
            .write()
            .as_mut()
            .map(|state| state.working = false);

        Ok(())
    }
}

// MARK: MeshState

#[derive(Debug, Default)]
struct MeshState {
    uploaded: Arc<RwLock<usize>>,
    meshes: Vec<Arc<(wgpu::Buffer, wgpu::Buffer)>>,
    working: bool,
}

// MARK: SharedResources

#[derive(Debug)]
struct SharedResources {
    pipeline: wgpu::RenderPipeline,
}

impl SharedResources {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("mesh.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mesh Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..64,
            }],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Mesh Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[LumenVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some((*target_format).into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multiview: None,
            multisample: wgpu::MultisampleState::default(),
        });

        Self { pipeline }
    }
}
