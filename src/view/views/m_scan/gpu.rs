// MARK: PolarViewPaintCallback

use std::{num::NonZeroU32, sync::Arc};

use wgpu::util::DeviceExt;

use crate::gui::color_maps;

use super::MAX_TEXTURES;

pub fn upload_b_scan_segmentation(
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

pub(super) struct PolarViewPaintCallback {
    pub texture_bind_group: Arc<wgpu::BindGroup>,
    pub texture_count: usize,
    pub a_scan_count: usize,
    pub rect: egui::Rect,
    pub map_idx: u32,
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
            map_idx: u32,
            a_scan_count: u32,
        }

        render_pass.set_pipeline(&resources.polar_view_pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &resources.color_maps_bind_group, &[]);
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
                map_idx: self.map_idx,
                a_scan_count: self.a_scan_count as u32,
            }]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: CartesianViewPaintCallback

pub(super) struct CartesianViewPaintCallback {
    pub texture_bind_group: Arc<wgpu::BindGroup>,
    pub texture_count: usize,
    pub b_scan_start: usize,
    pub b_scan_end: usize,
    pub rect: egui::Rect,
    pub map_idx: u32,
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
            map_idx: u32,
            b_scan_start: u32,
            b_scan_end: u32,
        }

        render_pass.set_pipeline(&resources.cartesian_view_pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &resources.color_maps_bind_group, &[]);
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
                map_idx: self.map_idx,
                b_scan_start: self.b_scan_start as u32,
                b_scan_end: self.b_scan_end as u32,
            }]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: SideViewPaintCallback

pub(super) struct SideViewPaintCallback {
    pub b_scan_bind_group: Arc<wgpu::BindGroup>,
    pub texture_bind_group: Arc<wgpu::BindGroup>,
    pub texture_count: usize,
    pub view_rotation: f32,
    pub rect: egui::Rect,
    pub map_idx: u32,
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
            map_idx: u32,
            view_rot: f32,
        }

        render_pass.set_pipeline(&resources.side_view_pipeline);
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &resources.color_maps_bind_group, &[]);
        render_pass.set_bind_group(2, &self.b_scan_bind_group, &[]);
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
                map_idx: self.map_idx,
                view_rot: self.view_rotation,
            }]),
        );
        render_pass.draw(0..6, 0..1);
    }
}

// MARK: SharedResources

pub(super) struct SharedResources {
    pub polar_view_pipeline: wgpu::RenderPipeline,
    pub cartesian_view_pipeline: wgpu::RenderPipeline,
    pub side_view_pipeline: wgpu::RenderPipeline,
    pub scan_bind_group_layout: Arc<wgpu::BindGroupLayout>,
    pub color_maps_bind_group: Arc<wgpu::BindGroup>,
    pub b_scan_segmentation_bind_group_layout: Arc<wgpu::BindGroupLayout>,
}

impl SharedResources {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> Self {
        let (color_maps_bind_group_layout, color_maps_bind_group) =
            Self::create_color_map_bind_group(device, queue);

        let scan_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let polar_view_pipeline = Self::create_polar_view_pipeline(
            device,
            target_format,
            &shader,
            &[&scan_bind_group_layout, &color_maps_bind_group_layout],
        );

        let cartesian_view_pipeline = Self::create_cartesian_view_pipeline(
            device,
            target_format,
            &shader,
            &[&scan_bind_group_layout, &color_maps_bind_group_layout],
        );

        let side_view_pipeline = Self::create_side_view_pipeline(
            device,
            target_format,
            &shader,
            &[
                &scan_bind_group_layout,
                &color_maps_bind_group_layout,
                &b_scan_bind_group_layout,
            ],
        );

        Self {
            polar_view_pipeline,
            cartesian_view_pipeline,
            side_view_pipeline,
            scan_bind_group_layout: Arc::new(scan_bind_group_layout),
            color_maps_bind_group: Arc::new(color_maps_bind_group),
            b_scan_segmentation_bind_group_layout: Arc::new(b_scan_bind_group_layout),
        }
    }

    fn create_polar_view_pipeline(
        device: &wgpu::Device,
        target_format: &wgpu::TextureFormat,
        shader: &wgpu::ShaderModule,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Polar Pipeline Layout"),
            bind_group_layouts,
            push_constant_ranges: &[
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..28,
                },
            ],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Polar Render Pipeline"),
            layout: Some(&pipeline_layout),
            multisample: wgpu::MultisampleState::default(),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "polar_fs_main",
                targets: &[Some((*target_format).into())],
            }),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
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
        shader: &wgpu::ShaderModule,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Cartesian Pipeline Layout"),
            bind_group_layouts,
            push_constant_ranges: &[
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..32,
                },
            ],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Cartesian Render Pipeline"),
            layout: Some(&pipeline_layout),
            multisample: wgpu::MultisampleState::default(),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "cartesian_fs_main",
                targets: &[Some((*target_format).into())],
            }),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
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
        shader: &wgpu::ShaderModule,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Polar Pipeline Layout"),
            bind_group_layouts,
            push_constant_ranges: &[
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..28,
                },
            ],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MScan Polar Render Pipeline"),
            layout: Some(&pipeline_layout),
            multisample: wgpu::MultisampleState::default(),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "side_fs_main",
                targets: &[Some((*target_format).into())],
            }),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multiview: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
        })
    }

    fn create_color_map_bind_group(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let color_maps_tex = color_maps::upload_color_maps(device, queue);

        let color_maps_view = color_maps_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let color_maps_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Color Maps Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let color_maps_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Color Maps Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::ReadOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let color_map_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color Maps Bind Group"),
            layout: &color_maps_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_maps_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&color_maps_sampler),
                },
            ],
        });

        (color_maps_bind_group_layout, color_map_bind_group)
    }
}
