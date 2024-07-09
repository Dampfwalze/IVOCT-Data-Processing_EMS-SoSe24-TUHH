use std::{collections::HashSet, num::NonZeroU32, sync::Arc};

use crate::{
    cache::Cached,
    gui::{color_maps, widgets::PanZoomRect},
    queue_channel::error::RecvError,
};

use super::prelude::*;
use egui::{pos2, vec2, Color32, ComboBox, Layout, Rect, Response, Sense, Shape, Stroke, Vec2};
use futures::future;
use nalgebra::DVector;
use tokio::sync::{watch, RwLock};
use wgpu::{util::DeviceExt, PushConstantRange};

/// WGPU requires to specify a maximum number of textures we can bind in a
/// texture array. When we exceed this number, the remaining textures are not
/// rendered.
pub const MAX_TEXTURES: usize = 100;

pub enum InputId {
    MScan,
    BScanSegmentation,
    MScanSegmentation,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => MScan,
    1 => BScanSegmentation,
    2 => MScanSegmentation,
});

// MARK: View

pub struct View {
    m_scan: NodeOutput,
    b_scan_segmentation: Option<NodeOutput>,
    m_scan_segmentation: Option<NodeOutput>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    m_scan_segmentation_rx: Option<watch::Receiver<Vec<usize>>>,

    b_scan_segmentation_rx: Option<watch::Receiver<Vec<usize>>>,
    b_scan_segmentation_buffer: Option<(wgpu::Buffer, Arc<wgpu::BindGroup>)>,
    b_scan_segmentation_bind_group_layout: Arc<wgpu::BindGroupLayout>,

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
            _ => None,
        }
    }

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)> {
        [
            (InputId::MScan, Some(self.m_scan)),
            (InputId::BScanSegmentation, self.b_scan_segmentation),
            (InputId::MScanSegmentation, self.m_scan_segmentation),
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
        }
    }

    fn create_view_task(&mut self) -> impl DataViewTask<InputId = Self::InputId, DataView = Self> {
        let (b_scan_tx, b_scan_rx) = watch::channel(Vec::new());
        let (m_scan_tx, m_scan_rx) = watch::channel(Vec::new());

        self.b_scan_segmentation_rx = Some(b_scan_rx);
        self.m_scan_segmentation_rx = Some(m_scan_rx);

        Task {
            m_scan_in: TaskInput::default(),
            b_scan_segmentation_in: TaskInput::default(),
            m_scan_segmentation_in: TaskInput::default(),
            textures_state: self.textures_state.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            bind_group_layout: self.bind_group_layout.clone(),
            b_scan_segmentation_tx: b_scan_tx,
            m_scan_segmentation_tx: m_scan_tx,
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

fn polar_m_scan_ui(
    ui: &mut egui::Ui,
    textures_state: &TexturesState,
    texture_bind_group: Arc<wgpu::BindGroup>,
    b_scan_segmentation: Option<&[usize]>,
    m_scan_segmentation: Option<&[usize]>,
    map_idx: u32,
) -> egui::Response {
    PanZoomRect::new()
        .zoom_y(false)
        .min_zoom(1.0)
        .show(ui, |ui, viewport, n_viewport| {
            let response = ui.allocate_rect(ui.max_rect(), Sense::hover());
            let rect = response.rect;

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
                        map_idx,
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

            if let Some(m_scan_segmentation) = m_scan_segmentation {
                let points = (rect.left() as usize..=rect.right() as usize)
                    .filter_map(|global_x| {
                        let viewport_x = (global_x as f32 - viewport.min.x) / viewport.width();

                        if viewport_x < 0.0 {
                            return None;
                        }

                        let scan_idx =
                            (viewport_x * (textures_state.a_scan_count - 1) as f32) as usize;

                        if scan_idx >= m_scan_segmentation.len() {
                            return None;
                        }

                        let scan_idx = scan_idx.min(textures_state.a_scan_count - 1);

                        let seg = m_scan_segmentation[scan_idx];
                        if seg >= textures_state.a_scan_samples {
                            return None;
                        }

                        let y = seg as f32 / textures_state.a_scan_samples as f32;
                        let y = y * viewport.height() + viewport.min.y;

                        let x = (scan_idx as f32) / (textures_state.a_scan_count as f32);
                        let x = x * viewport.width() + viewport.min.x;

                        Some(pos2(x as f32, y))
                    })
                    .collect::<Vec<_>>();

                ui.painter()
                    .add(Shape::line(points, Stroke::new(2.0, Color32::RED)));
            }
        })
        .response
}

fn cartesian_m_scan_ui(
    ui: &mut egui::Ui,
    textures_state: &TexturesState,
    texture_bind_group: Arc<wgpu::BindGroup>,
    b_scan_segmentation: &[usize],
    m_scan_segmentation: Option<&[usize]>,
    map_idx: u32,
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
                map_idx,
            },
        ));

    if let Some(m_scan_segmentation) = m_scan_segmentation {
        let b_scan_start = b_scan_segmentation[current_b_scan];
        let b_scan_end = b_scan_segmentation[current_b_scan + 1];
        let b_scan_size = b_scan_end - b_scan_start;
        let radius = rect.width() / 2.0;
        let step_size = (b_scan_size / 200).max(1);

        let points = (b_scan_start..b_scan_end)
            .step_by(step_size)
            .filter_map(|i| {
                m_scan_segmentation.get(i).and_then(|&seg| {
                    if seg >= textures_state.a_scan_samples {
                        return None;
                    }
                    let alpha = (i - b_scan_start) as f32 / b_scan_size as f32;
                    let alpha = alpha * std::f32::consts::TAU;

                    let vec = seg as f32 / textures_state.a_scan_samples as f32
                        * Vec2::angled(alpha)
                        * radius;

                    Some(rect.center() + vec2(-vec.y, -vec.x))
                })
            })
            .collect::<Vec<_>>();

        ui.painter()
            .add(Shape::closed_line(points, Stroke::new(2.0, Color32::RED)));
    }

    // Draw current_rotation line
    let current_rotation = ui
        .data(|d| d.get_temp::<isize>(ui.id().with("current_rotation")))
        .unwrap_or(0) as f32
        / 100.0;

    let center = rect.center();
    let vec = rect.width() / 2.0 * Vec2::angled(current_rotation * std::f32::consts::TAU);
    let vec = vec2(vec.y, vec.x);

    ui.painter().line_segment(
        [center + vec, center + 0.8 * vec],
        Stroke::new(2.0, Color32::BLUE),
    );
    ui.painter().line_segment(
        [center - vec, center - 0.8 * vec],
        Stroke::new(2.0, Color32::BLUE),
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
    b_scan_segmentation: &[usize],
    m_scan_segmentation: Option<&[usize]>,
    map_idx: u32,
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
                map_idx,
            },
        ));

    if let Some(m_scan_segmentation) = m_scan_segmentation {
        let rect = response.rect;
        let points1 = b_scan_segmentation
            .windows(2)
            .enumerate()
            .filter_map(|(i, seg)| match seg {
                &[start, end] => {
                    let scan_idx = start + ((end - start) as f32 * current_rotation) as usize;

                    m_scan_segmentation.get(scan_idx).and_then(|&seg| {
                        if seg >= textures_state.a_scan_samples {
                            return None;
                        }

                        let y = seg as f32 / textures_state.a_scan_samples as f32;
                        let y = rect.center().y - y * rect.height() * 0.5;

                        let x = (i as f32 + 0.5) / (b_scan_segmentation.len() - 1) as f32;
                        let x = rect.left() + x * rect.width();

                        Some(pos2(x, y))
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let points2 = b_scan_segmentation
            .windows(2)
            .enumerate()
            .filter_map(|(i, seg)| match seg {
                &[start, end] => {
                    let scan_idx =
                        start + ((end - start) as f32 * ((current_rotation + 0.5) % 1.0)) as usize;

                    m_scan_segmentation.get(scan_idx).and_then(|&seg| {
                        if seg >= textures_state.a_scan_samples {
                            return None;
                        }

                        let y = seg as f32 / textures_state.a_scan_samples as f32;
                        let y = rect.center().y + y * rect.height() * 0.5;

                        let x = (i as f32 + 0.5) / (b_scan_segmentation.len() - 1) as f32;
                        let x = rect.left() + x * rect.width();

                        Some(pos2(x, y))
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        ui.painter()
            .add(Shape::line(points1, Stroke::new(2.0, Color32::RED)));

        ui.painter()
            .add(Shape::line(points2, Stroke::new(2.0, Color32::RED)));
    }

    // Draw current_b_scan line
    let current_b_scan = ui
        .data(|d| d.get_temp::<isize>(ui.id().with("current_b_scan")))
        .unwrap_or(0) as f32;

    let rect = response.rect;
    let x = rect.left()
        + rect.width() * (current_b_scan + 0.5) / (b_scan_segmentation.len() - 1) as f32;
    ui.painter().line_segment(
        [
            pos2(x, rect.top()),
            pos2(x, rect.top() + rect.height() * 0.1),
        ],
        Stroke::new(2.0, Color32::BLUE),
    );
    ui.painter().line_segment(
        [
            pos2(x, rect.bottom()),
            pos2(x, rect.bottom() - rect.height() * 0.1),
        ],
        Stroke::new(2.0, Color32::BLUE),
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
    map_idx: u32,
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

struct CartesianViewPaintCallback {
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    b_scan_start: usize,
    b_scan_end: usize,
    rect: Rect,
    map_idx: u32,
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

struct SideViewPaintCallback {
    b_scan_bind_group: Arc<wgpu::BindGroup>,
    texture_bind_group: Arc<wgpu::BindGroup>,
    texture_count: usize,
    view_rotation: f32,
    rect: Rect,
    map_idx: u32,
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

// MARK: Task

struct Task {
    m_scan_in: TaskInput<requests::MScan>,
    b_scan_segmentation_in: TaskInput<requests::BScanSegmentation>,
    m_scan_segmentation_in: TaskInput<requests::MScanSegmentation>,

    textures_state: Cached<Option<TexturesState>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,

    b_scan_segmentation_tx: watch::Sender<Vec<usize>>,
    m_scan_segmentation_tx: watch::Sender<Vec<usize>>,
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
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::MScan => self.m_scan_in.disconnect(),
            InputId::BScanSegmentation => self.b_scan_segmentation_in.disconnect(),
            InputId::MScanSegmentation => self.m_scan_segmentation_in.disconnect(),
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
        fn invalidate_m_scan_segmentation(slf: &mut Task) {
            let _ = slf.m_scan_segmentation_tx.send_modify(|d| {
                d.clear();
            });
        }

        match cause {
            InvalidationCause::Synced => invalidate_m_scan(self),
            InvalidationCause::InputInvalidated(input_id) => match input_id.into() {
                InputId::MScan => invalidate_m_scan(self),
                InputId::BScanSegmentation => invalidate_b_scan_segmentation(self),
                InputId::MScanSegmentation => invalidate_m_scan_segmentation(self),
            },
            _ => {}
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

// MARK: SharedResources

struct SharedResources {
    polar_view_pipeline: wgpu::RenderPipeline,
    cartesian_view_pipeline: wgpu::RenderPipeline,
    side_view_pipeline: wgpu::RenderPipeline,
    scan_bind_group_layout: Arc<wgpu::BindGroupLayout>,
    color_maps_bind_group: Arc<wgpu::BindGroup>,
    b_scan_segmentation_bind_group_layout: Arc<wgpu::BindGroupLayout>,
}

impl SharedResources {
    fn new(
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

        let polar_view_pipeline = Self::create_polar_view_pipeline(
            device,
            target_format,
            &[&scan_bind_group_layout, &color_maps_bind_group_layout],
        );

        let cartesian_view_pipeline = Self::create_cartesian_view_pipeline(
            device,
            target_format,
            &[&scan_bind_group_layout, &color_maps_bind_group_layout],
        );

        let side_view_pipeline = Self::create_side_view_pipeline(
            device,
            target_format,
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
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Polar Pipeline Layout"),
            bind_group_layouts,
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
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Cartesian Pipeline Layout"),
            bind_group_layouts,
            push_constant_ranges: &[
                PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..16,
                },
                PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 16..32,
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
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MScan Polar Pipeline Layout"),
            bind_group_layouts,
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
