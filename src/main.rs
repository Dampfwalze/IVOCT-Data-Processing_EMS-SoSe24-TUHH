mod app;
mod gui;
#[allow(unused)]
mod node_graph;
mod pipeline;
#[allow(unused)]
mod queue_channel;

use app::*;

#[tokio::main]
async fn main() {
    eframe::run_native(
        "IVOCT Test App",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            hardware_acceleration: eframe::HardwareAcceleration::Preferred,
            wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
                supported_backends: eframe::wgpu::Backends::PRIMARY,
                power_preference: eframe::wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            },
            ..Default::default()
        },
        Box::new(|cc| Box::new(IVOCTTestApp::new(cc))),
    )
    .unwrap();
}
