use egui::{InnerResponse, Rect, Ui};

use super::PanZoom;

pub struct PanZoomRect {
    zoom_x: bool,
    zoom_y: bool,
    max_zoom: f32,
    min_zoom: f32,
}

#[allow(unused)]
impl PanZoomRect {
    pub fn new() -> Self {
        Self {
            zoom_x: true,
            zoom_y: true,
            max_zoom: f32::INFINITY,
            min_zoom: 0.0,
        }
    }

    pub fn zoom_x(mut self, zoom_x: bool) -> Self {
        self.zoom_x = zoom_x;
        self
    }

    pub fn zoom_y(mut self, zoom_y: bool) -> Self {
        self.zoom_y = zoom_y;
        self
    }

    pub fn max_zoom(mut self, max_zoom: f32) -> Self {
        self.max_zoom = max_zoom;
        self
    }

    pub fn min_zoom(mut self, min_zoom: f32) -> Self {
        self.min_zoom = min_zoom;
        self
    }
}

impl PanZoomRect {
    pub fn show<R>(
        &mut self,
        ui: &mut Ui,
        add_content: impl FnOnce(&mut Ui, Rect, Rect) -> R,
    ) -> InnerResponse<R> {
        let InnerResponse {
            inner: (transform, mut inner_rect),
            response,
            ..
        } = PanZoom::new()
            .max_zoom(self.max_zoom)
            .min_zoom(self.min_zoom)
            .show(ui, |ui, transform| (*transform, ui.max_rect()));

        let rect = response.rect;

        let inner_viewport = transform.inverse() * rect;

        if !self.zoom_x {
            inner_rect.min.x = inner_viewport.min.x;
            inner_rect.max.x = inner_viewport.max.x;
        }

        if !self.zoom_y {
            inner_rect.min.y = inner_viewport.min.y;
            inner_rect.max.y = inner_viewport.max.y;
        }

        let viewport = transform * inner_rect;

        let normalized_viewport = Rect::from_min_size(
            ((viewport.min - rect.min) / rect.size()).to_pos2(),
            viewport.size() / rect.size(),
        );

        let inner = ui
            .allocate_ui_at_rect(rect, |ui| {
                ui.set_clip_rect(rect);

                add_content(ui, viewport, normalized_viewport)
            })
            .inner;

        InnerResponse { inner, response }
    }
}
