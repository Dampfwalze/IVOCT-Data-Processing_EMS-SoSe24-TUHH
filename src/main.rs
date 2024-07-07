mod app;
mod cache;
mod convolution;
mod gui;
#[allow(unused)]
mod node_graph;
mod pipeline;
#[allow(unused)]
mod queue_channel;
mod view;

use std::sync::Arc;

use app::*;

#[tokio::main]
async fn main() {
    eframe::run_native(
        "IVOCT Test App",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            hardware_acceleration: eframe::HardwareAcceleration::Preferred,
            wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
                supported_backends: wgpu::Backends::PRIMARY,
                power_preference: wgpu::PowerPreference::HighPerformance,
                device_descriptor: Arc::new(|_adapter| wgpu::DeviceDescriptor {
                    label: Some("egui wgpu device"),
                    required_features: wgpu::Features::TEXTURE_BINDING_ARRAY
                        | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
                        | wgpu::Features::PUSH_CONSTANTS
                        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 12000,
                        max_sampled_textures_per_shader_stage: view::views::m_scan::MAX_TEXTURES as _,
                        max_push_constant_size: 28,
                        ..Default::default()
                    },
                }),
                ..Default::default()
            },
            ..Default::default()
        },
        Box::new(|cc| {
            cc.egui_ctx.set_style(egui::Style {
                visuals: egui::Visuals::dark(),
                ..egui::Style::default()
            });

            Box::new(IVOCTTestApp::new(cc))
        }),
    )
    .unwrap();
}
