use egui::{Color32, Pos2, Sense, Shape, Stroke};

/// Widget the user can draw a line on, by holding right click. This line is
/// used to determine which connections to cut in a node graph.
pub struct DrawCut;

impl DrawCut {
    pub fn ui(self, ui: &mut egui::Ui) -> (egui::Response, Option<Vec<Pos2>>) {
        let line_id = ui.id().with("line");
        let mut line: Vec<_> = ui.data(|d| d.get_temp(line_id)).unwrap_or_default();

        let mut response = ui.allocate_rect(ui.max_rect(), Sense::hover());
        let painter = ui
            .painter_at(response.rect)
            .with_layer_id(egui::LayerId::new(egui::Order::Foreground, line_id));

        let pointer = ui.ctx().input(|i| i.pointer.clone());

        if let (Some(origin), Some(pos)) = (pointer.press_origin(), pointer.interact_pos()) {
            if response.rect.contains(origin)
                && pointer.secondary_down()
                && pointer.is_decidedly_dragging()
            {
                if line.last().cloned() != Some(pos) {
                    line.push(pos);
                    response.mark_changed();
                }
            }
        } else if !line.is_empty() {
            ui.data_mut(|d| d.remove::<Vec<Pos2>>(line_id));
            response.mark_changed();
            return (response, Some(line));
        }

        let shape = Shape::dashed_line(&line, Stroke::new(1.0, Color32::WHITE), 5.0, 5.0);

        painter.add(shape);

        ui.data_mut(|d| d.insert_temp(line_id, line));

        (response, None)
    }
}
