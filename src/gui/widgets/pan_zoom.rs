use egui::{emath::TSTransform, InnerResponse, LayerId, Order, Ui};

pub struct PanZoom;

impl PanZoom {
    pub fn new() -> Self {
        Self
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
            let pointer_in_layer = transform.inverse() * pointer;
            let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
            let scroll_delta = ui.ctx().input(|i| (i.smooth_scroll_delta.y / 200.0).exp());

            // Zoom in on pointer
            transform = transform
                * TSTransform::from_translation(pointer_in_layer.to_vec2())
                * TSTransform::from_scaling(zoom_delta * scroll_delta)
                * TSTransform::from_translation(-pointer_in_layer.to_vec2());
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
                    ui.set_enabled(enable_ui);
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
