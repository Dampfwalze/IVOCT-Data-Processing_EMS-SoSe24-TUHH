use egui::{emath::TSTransform, InnerResponse, LayerId, Order, Ui};

pub struct PanZoom {
    max_zoom: f32,
    min_zoom: f32,
}

impl PanZoom {
    pub fn new() -> Self {
        Self {
            max_zoom: f32::INFINITY,
            min_zoom: 0.0,
        }
    }

    pub fn max_zoom(mut self, max_zoom: f32) -> Self {
        self.max_zoom = max_zoom;
        self
    }

    pub fn min_zoom(mut self, min_zoom: f32) -> Self {
        self.min_zoom = min_zoom;
        self
    }

    pub fn show<R>(
        &mut self,
        ui: &mut Ui,
        add_content: impl FnOnce(&mut Ui, &mut TSTransform) -> R,
    ) -> InnerResponse<R> {
        let (id, rect) = ui.allocate_space(ui.available_size());

        let mut transform: TSTransform = ui
            .ctx()
            .memory(|mem| mem.data.get_temp(id))
            .unwrap_or(TSTransform::IDENTITY);

        let response = ui.interact(rect, id, egui::Sense::click_and_drag());

        let pointer = ui.ctx().input(|i| i.pointer.clone());

        if let Some(pos) = pointer.press_origin() {
            if rect.contains(pos) && pointer.middle_down() && pointer.is_decidedly_dragging() {
                transform.translation += pointer.delta();
            }
        }

        if let Some(pointer) = pointer.hover_pos() {
            if rect.contains(pointer) {
                let pointer_in_layer = transform.inverse() * pointer;
                let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
                let scroll_delta = ui.ctx().input(|i| (i.smooth_scroll_delta.y / 200.0).exp());

                // Zoom in on pointer
                let mut transform_after_scaling = transform
                    * TSTransform::from_translation(pointer_in_layer.to_vec2())
                    * TSTransform::from_scaling(zoom_delta * scroll_delta);
                transform_after_scaling.scaling = transform_after_scaling
                    .scaling
                    .clamp(self.min_zoom, self.max_zoom);
                transform = transform_after_scaling
                    * TSTransform::from_translation(-pointer_in_layer.to_vec2());
            }
        }

        let layer_id = LayerId::new(
            match pointer.middle_down() {
                true => Order::Background,
                false => Order::Middle,
            },
            id.with("pan_zoom"),
        );

        // Hack: Disable children to prevent them from being interacted with
        // when panning
        let enable_ui = !pointer.middle_down()
            || response.hovered()
            || !pointer.is_decidedly_dragging()
            || !matches!(pointer.press_origin(), Some(pos)
                    if rect.contains(pos));

        let res = ui
            .allocate_ui_at_rect(rect, |ui| {
                ui.with_layer_id(layer_id, |ui| {
                    ui.set_clip_rect(transform.inverse() * rect);

                    if !enable_ui {
                        ui.disable();
                    }

                    add_content(ui, &mut transform)
                })
                .inner
            })
            .inner;

        ui.ctx().set_transform_layer(layer_id, transform);

        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(id, transform);
        });

        InnerResponse {
            response,
            inner: res,
        }
    }
}
