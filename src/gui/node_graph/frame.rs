use egui::{
    emath::TSTransform, epaint::TextShape, Color32, Id, InnerResponse, Margin, Pos2, Rect,
    Response, Rounding, Sense, Shape, Stroke, TextStyle, Ui, Vec2, WidgetText,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeFrameState {
    #[serde(with = "Pos2Def")]
    pub position: Pos2,
}

/// A mirror for `egui::Pos2`, because it does not implement serde traits.
#[derive(Serialize, Deserialize)]
#[serde(remote = "Pos2")]
struct Pos2Def {
    x: f32,
    y: f32,
}

pub struct NodeFrame<'a> {
    state: Option<&'a mut NodeFrameState>,

    id: Id,
    color: egui::Color32,
    name: WidgetText,
    selected: bool,
    sense: Sense,
    follow_mouse: bool,
}

impl<'a> NodeFrame<'a> {
    pub fn new(id: impl Into<Id>, name: impl Into<WidgetText>) -> Self {
        Self {
            state: None,
            id: id.into(),
            color: Color32::from_rgb(255, 0, 0),
            name: name.into(),
            selected: false,
            sense: Sense::drag(),
            follow_mouse: false,
        }
    }

    pub fn state(mut self, state: &'a mut NodeFrameState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = color;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense.union(Sense::drag());
        self
    }

    pub fn follow_mouse(mut self, follow_mouse: bool) -> Self {
        self.follow_mouse = follow_mouse;
        self
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        origin: Vec2,
        add_contents: impl FnOnce(&mut Ui),
    ) -> Response {
        // Whe have to pass in 'origin', because the ui rect might expand into
        // negative space, which would cause the origin to change for subsequent
        // frames.

        let padding = Margin::symmetric(10.0, 5.0);
        let rounding = 5.0;

        let shadow = ui.style().visuals.popup_shadow;

        let mut state = match self.state {
            Some(ref state) => **state,
            None => ui
                .memory_mut(|mem| mem.data.get_temp(self.id.with("state")))
                .unwrap_or_default(),
        };

        let follow_mouse = ui.memory(|mem| mem.data.get_temp::<bool>(self.id.with("follow_mouse")));

        let follow_mouse = if let Some(follow) = follow_mouse {
            follow
        } else if self.follow_mouse {
            ui.memory_mut(|mem| mem.data.insert_temp(self.id.with("follow_mouse"), true));
            true
        } else {
            ui.memory_mut(|mem| mem.data.remove::<bool>(self.id.with("follow_mouse")));
            false
        };

        let follow_mouse = if follow_mouse {
            if ui.ctx().is_using_pointer() {
                ui.data_mut(|d| d.remove::<bool>(self.id.with("follow_mouse")));
                false
            } else {
                true
            }
        } else {
            false
        };

        if follow_mouse {
            let transform = ui
                .ctx()
                .memory_mut(|mem| mem.layer_transforms.get(&ui.layer_id()).copied())
                .unwrap_or(TSTransform::IDENTITY);

            if let Some(mouse_pos) = ui.ctx().pointer_hover_pos() {
                state.position = transform.inverse() * mouse_pos - origin;
            }
        }

        let interact_rect = ui
            .memory(|mem| mem.data.get_temp(self.id.with("rect")))
            .unwrap_or_else(|| Rect::from_min_size(state.position, Vec2::new(200.0, 0.0)));

        let rect = Rect::from_min_size(state.position + origin, Vec2::new(200.0, 0.0));

        // We don't know the shape yet, but to have the frame be painted behind
        // the content, whe must insert the operations now.
        let shadow_op = ui.painter().add(Shape::Noop);
        let bg_op = ui.painter().add(Shape::Noop);
        let bar_op = ui.painter().add(Shape::Noop);
        let outline_op = ui.painter().add(Shape::Noop);

        let response = ui.interact(interact_rect, self.id, self.sense);

        let InnerResponse {
            response: Response { rect, .. },
            inner: title_size,
            ..
        } = ui.allocate_ui_at_rect(rect, |ui| {
            let res = ui.allocate_ui_at_rect(ui.max_rect() - padding, |ui| {
                ui.with_layout(ui.layout().clone().with_cross_justify(true), |ui| {
                    // Draw text not as label to make it non interactive
                    let galley =
                        self.name
                            .clone()
                            .into_galley(ui, None, f32::INFINITY, TextStyle::Button);

                    let title_size = galley.size();
                    let (_, rect) = ui.allocate_space(title_size);

                    ui.painter()
                        .add(TextShape::new(rect.left_top(), galley, Color32::WHITE));

                    ui.allocate_space(Vec2::new(0.0, padding.top));

                    // Workaround for: https://github.com/emilk/egui/pull/2262
                    // Replace with ui.push_id when fixed
                    let mut _ui = Ui::new(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        self.id.with("content"),
                        ui.available_rect_before_wrap(),
                        ui.clip_rect(),
                        egui::UiStackInfo::default(),
                    );
                    _ui.with_layout(*ui.layout(), add_contents);
                    ui.allocate_rect(_ui.min_rect(), Sense::hover());

                    // ui.push_id(self.id.with("content"), add_contents);

                    title_size
                })
                .inner
            });

            // Adjust for missing padding on right and bottom
            ui.allocate_space(Vec2::new(
                res.response.rect.right() - rect.left() + padding.right,
                padding.bottom - 2.0,
            ));

            res.inner
        });

        ui.painter()
            .set(shadow_op, Shape::from(shadow.as_shape(rect, rounding)));

        ui.painter().set(
            bg_op,
            Shape::Rect(egui::epaint::RectShape::new(
                rect,
                rounding,
                ui.style().visuals.window_fill(),
                Stroke::NONE,
            )),
        );

        let mut top_rect = rect;
        top_rect.set_height(title_size.y + 2.0 * padding.top);
        ui.painter().set(
            bar_op,
            Shape::Rect(egui::epaint::RectShape::new(
                top_rect,
                Rounding {
                    nw: rounding,
                    ne: rounding,
                    sw: 0.0,
                    se: 0.0,
                },
                self.color,
                Stroke::NONE,
            )),
        );

        ui.painter().set(
            outline_op,
            Shape::Rect(egui::epaint::RectShape::new(
                rect,
                rounding,
                Color32::TRANSPARENT,
                match self.selected {
                    true => Stroke::new(1.0, Color32::WHITE),
                    false => ui.style().visuals.window_stroke(),
                },
            )),
        );

        if response.dragged() {
            state.position += response.drag_delta();
        }

        ui.memory_mut(|mem| mem.data.insert_temp(self.id.with("rect"), rect));

        match self.state {
            Some(ref mut state_) => **state_ = state,
            None => ui.memory_mut(|mem| mem.data.insert_temp(self.id.with("state"), state)),
        }

        response
    }
}
