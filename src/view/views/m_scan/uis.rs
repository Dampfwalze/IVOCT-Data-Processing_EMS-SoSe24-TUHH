use std::sync::Arc;

use egui::*;
use nalgebra::Vector2;

use crate::gui::widgets::PanZoomRect;

use super::{
    gpu::{CartesianViewPaintCallback, PolarViewPaintCallback, SideViewPaintCallback},
    types::BScanDiameter,
    TexturesState,
};

pub fn polar_m_scan_ui(
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

pub fn cartesian_m_scan_ui(
    ui: &mut egui::Ui,
    textures_state: &TexturesState,
    texture_bind_group: Arc<wgpu::BindGroup>,
    b_scan_segmentation: &[usize],
    m_scan_segmentation: Option<&[usize]>,
    diameters: Option<&[BScanDiameter]>,
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

    if let Some(diameter) = diameters.and_then(|d| d.get(current_b_scan)) {
        let line_at_rot = |[p1, p2]: [Vector2<f32>; 2], diameter, stroke: Stroke| {
            let factor = rect.width() / 2.0 / textures_state.a_scan_samples as f32;
            let center = rect.center();
            let p1 = center + vec2(-p1.y, -p1.x) * factor;
            let p2 = center + vec2(-p2.y, -p2.x) * factor;
            let text_pos = p1.lerp(p2, if p1.y < p2.y { 0.1 } else { 0.9 });
            if p1.y < p2.y {
                p1.lerp(p2, 0.1)
            } else {
                p2.lerp(p1, 0.1)
            };

            ui.painter().line_segment([p1, p2], stroke);

            ui.painter().text(
                text_pos,
                Align2::LEFT_BOTTOM,
                format!("{:.2} mm", diameter),
                FontId::default(),
                stroke.color,
            );
        };

        line_at_rot(
            diameter.max_points,
            diameter.max,
            Stroke::new(2.0, Color32::GREEN),
        );
        line_at_rot(
            diameter.min_points,
            diameter.min,
            Stroke::new(2.0, Color32::YELLOW),
        );
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

pub fn side_m_scan_ui(
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
