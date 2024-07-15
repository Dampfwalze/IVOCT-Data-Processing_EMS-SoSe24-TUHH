use std::collections::HashMap;

use egui::{
    epaint::PathStroke, Color32, DragAndDrop, InnerResponse, Key, PointerButton, Pos2, Rect,
    Response, Sense, Shape, Stroke, Vec2,
};

use crate::gui::widgets::PanZoom;

use super::{
    add_node_popup::AddNodePopup, draw_cut::DrawCut, frame::NodeFrame, EditNodeGraph, InputId,
    NodeGraphEditState, NodeId, NodeOutput, NodeUi, OutputId, TypeId,
};

pub struct NodeGraphResponse {
    pub selected: Option<NodeId>,
    pub activated: Option<NodeId>,
}

pub struct NodeGraphEditor<'a> {
    pipeline: &'a mut dyn EditNodeGraph,
    state: &'a mut NodeGraphEditState,
}

impl<'a> NodeGraphEditor<'a> {
    pub fn new(pipeline: &'a mut impl EditNodeGraph, state: &'a mut NodeGraphEditState) -> Self {
        Self {
            pipeline: pipeline as &mut dyn EditNodeGraph,
            state,
        }
    }

    fn get_pipeline_state_mut(&mut self) -> (&mut dyn EditNodeGraph, &mut NodeGraphEditState) {
        (self.pipeline, self.state)
    }

    fn sense_pin_drag(ui: &mut egui::Ui, pos: Pos2, to_left: bool) -> Response {
        let mut rect = Rect::from_center_size(pos, Vec2::splat(18.0));
        match to_left {
            true => rect.min.x -= 8.0,
            false => rect.max.x += 8.0,
        }
        ui.allocate_rect(rect, Sense::click_and_drag())
    }

    #[allow(non_upper_case_globals)]
    pub fn show(&mut self, ui: &mut egui::Ui) -> NodeGraphResponse {
        const line_with: f32 = 2.0;
        const pin_hover_point_radius: f32 = 2.0;
        const pin_radius: f32 = 4.0;
        const pin_stroke: Stroke = Stroke {
            width: 0.8,
            color: Color32::BLACK,
        };

        let selected_id = ui.id().with("selected");
        let mut selected: Option<NodeId> = ui.data(|d| d.get_temp(selected_id)).unwrap_or_default();

        let mut activated = None;

        let following_id = ui.id().with("following_node");
        let following_node: Option<NodeId> = ui
            .data_mut(|d| d.remove_temp::<usize>(following_id))
            .map(|id| id.into());

        let anything_focused = ui.ctx().memory(|mem| mem.focused()).is_some();

        let (pipeline, state) = self.get_pipeline_state_mut();

        let InnerResponse {
            response,
            inner: (connections, transform),
            ..
        } = PanZoom::new().show(ui, |ui, transform| {
            let node_ids = pipeline.get_node_ids();

            state.sync_state(&node_ids);

            // Common origin, because ui rect might expand into negative space
            // between drawing of nodes
            let origin = ui.min_rect().min.to_vec2();

            let bg_op = ui.painter().add(Shape::Noop);

            let mut connections = Vec::<(Pos2, NodeOutput, NodeId, InputId)>::new();
            let mut output_positions = HashMap::<NodeOutput, Pos2>::new();

            let mut pending_connection_end = None;

            let mut to_top = None;

            let to_delete_id = ui.id().with("to_delete");

            let delete = !anything_focused
                && ui
                    .ctx()
                    .input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace));
            let to_delete = if delete {
                selected
            } else {
                ui.data_mut(|d| {
                    d.get_temp::<NodeId>(to_delete_id).map(|id| {
                        d.remove::<NodeId>(to_delete_id);
                        id
                    })
                })
            };

            for node_id in &state.node_order {
                let node = pipeline.get_node_mut(*node_id);
                let Some(node) = node else {
                    eprintln!("Node not found: {:?}", node_id);
                    continue;
                };

                let (mut inputs, mut outputs) = (Vec::new(), Vec::new());

                let response = NodeFrame::new(ui.id().with(node_id), node.name())
                    .state(state.node_states.get_mut(node_id).unwrap())
                    .color(node.color())
                    .selected(matches!(selected, Some(id) if id == *node_id))
                    .sense(Sense::click_and_drag())
                    .follow_mouse(matches!(following_node, Some(id) if id == *node_id))
                    .show(ui, origin, |ui| {
                        node.ui(&mut NodeUi {
                            ui,
                            inputs: &mut inputs,
                            outputs: &mut outputs,
                        });
                    });

                if response.double_clicked() {
                    activated = Some(*node_id);
                }

                response.context_menu(|ui| {
                    ui.label("Node");
                    if ui.button("Delete").clicked() {
                        ui.close_menu();
                        ui.data_mut(|d| d.insert_temp(to_delete_id, *node_id))
                    }
                });

                // Remove all connections to the outputs of the node that is
                // being deleted
                if let Some(to_delete) = to_delete {
                    if to_delete == *node_id {
                        continue;
                    } else {
                        for input in inputs.iter() {
                            if let Some(connection) = input.connection {
                                if connection.node_id == to_delete {
                                    node.disconnect(input.id);
                                }
                            }
                        }
                    }
                }

                // Node should get focus
                if !anything_focused
                    && response.is_pointer_button_down_on()
                    && ui.input(|i| i.pointer.primary_pressed())
                {
                    to_top = Some(*node_id);
                    selected = Some(*node_id);
                }

                for input in inputs.iter() {
                    let response = Self::sense_pin_drag(ui, input.pos, true);

                    if response.dragged_by(PointerButton::Primary) {
                        response.dnd_set_drag_payload(DragPayload(
                            input.pos,
                            *node_id,
                            PayloadPin::Input(input.id),
                        ));
                    } else {
                        if let Some(payload) = response.dnd_hover_payload() {
                            if let DragPayload(_, _, PayloadPin::Output(_, _)) = *payload {
                                pending_connection_end = Some(input.pos);
                            }
                        }
                        if let Some(payload) = response.dnd_release_payload() {
                            if let DragPayload(_, node_id, PayloadPin::Output(output_id, type_id)) =
                                *payload
                            {
                                node.connect(
                                    input.id,
                                    NodeOutput::new(node_id, output_id, type_id),
                                );
                            }
                        }
                    }

                    if let Some(connection) = input.connection {
                        connections.push((input.pos, connection, *node_id, input.id));
                    }

                    ui.painter()
                        .circle(input.pos, pin_radius, input.color, pin_stroke);
                    if response.hovered() {
                        ui.painter().circle_filled(
                            input.pos,
                            pin_hover_point_radius,
                            Color32::WHITE,
                        );
                    }
                }

                for output in outputs.iter() {
                    let response = Self::sense_pin_drag(ui, output.pos, false);

                    if response.dragged_by(PointerButton::Primary) {
                        response.dnd_set_drag_payload(DragPayload(
                            output.pos,
                            *node_id,
                            PayloadPin::Output(output.id, output.type_),
                        ));
                    } else {
                        if let Some(payload) = response.dnd_hover_payload() {
                            if let DragPayload(_, _, PayloadPin::Input(_)) = *payload {
                                pending_connection_end = Some(output.pos);
                            }
                        }
                        if let Some(payload) = response.dnd_release_payload() {
                            if let DragPayload(_, other_node_id, PayloadPin::Input(input_id)) =
                                *payload
                            {
                                match pipeline.get_node_mut(other_node_id) {
                                    Some(node) => node.connect(
                                        input_id,
                                        NodeOutput::new(*node_id, output.id, output.type_),
                                    ),
                                    None => eprintln!("Node not found: {:?}", other_node_id),
                                }
                            }
                        }
                    }

                    output_positions.insert(
                        NodeOutput::new(*node_id, output.id, output.type_),
                        output.pos,
                    );

                    ui.painter()
                        .circle(output.pos, pin_radius, output.color, pin_stroke);
                    if response.hovered() {
                        ui.painter().circle_filled(
                            output.pos,
                            pin_hover_point_radius,
                            Color32::WHITE,
                        );
                    }
                }
            }

            if let Some(node_id) = to_top {
                state.to_top(node_id);
            }

            if let Some(to_delete) = to_delete {
                pipeline.remove_node(to_delete);
                selected = None;
            }

            // Draw connection that the user is currently creating
            if let Some(payload) = DragAndDrop::payload(ui.ctx()) {
                let DragPayload(start_pos, _, _) = *payload;

                let end_pos = match pending_connection_end {
                    Some(pos) => Some(pos),
                    None => ui
                        .ctx()
                        .input(|i| i.pointer.hover_pos())
                        .map(|pos| transform.inverse() * pos),
                };

                if let Some(end_pos) = end_pos {
                    ui.painter()
                        .line_segment([start_pos, end_pos], Stroke::new(line_with, Color32::WHITE));
                    ui.painter()
                        .circle_filled(start_pos, pin_hover_point_radius, Color32::WHITE);
                    ui.painter()
                        .circle_filled(end_pos, pin_hover_point_radius, Color32::WHITE);
                }
            }

            // Join connections with their output positions
            let connections = connections
                .iter()
                .filter_map(|(input_pos, output, node_id, input_id)| {
                    output_positions
                        .get(output)
                        .map(|output_pos| (*input_pos, *output_pos, *node_id, *input_id))
                })
                .collect::<Vec<_>>();

            // Draw existing connections
            let shapes = connections
                .iter()
                .map(|(input_pos, output_pos, _, _)| Shape::LineSegment {
                    points: [*input_pos, *output_pos],
                    stroke: PathStroke::new(line_with, Color32::WHITE),
                })
                .collect::<Vec<_>>();

            ui.painter().set(bg_op, Shape::Vec(shapes));

            (connections, *transform)
        });

        if let (_, Some(line)) = DrawCut.ui(ui) {
            let line = line
                .iter()
                .map(|pos| transform.inverse() * *pos)
                .collect::<Vec<_>>();

            for (p1, p2, node_id, input_id) in connections.iter() {
                for (start, end) in line.iter().zip(line.iter().skip(1)) {
                    if line_intersects(*p1, *p2, *start, *end).is_some() {
                        pipeline
                            .get_node_mut(*node_id)
                            .map(|node| node.disconnect(*input_id));
                        break;
                    }
                }
            }
        }

        response.context_menu(|ui| {
            if let Some(path) = AddNodePopup::new(&pipeline.addable_nodes()).show(ui) {
                ui.close_menu();
                let node_id = pipeline.add_node(path);
                selected = Some(node_id);

                ui.data_mut(|d| {
                    d.insert_temp::<usize>(following_id, node_id.into());
                });
            }
        });

        // Mass select
        // if response.dragged_by(PointerButton::Primary) {
        //     if let (Some(start), Some(end)) = (
        //         ui.ctx().input(|i| i.pointer.press_origin()),
        //         ui.ctx().input(|i| i.pointer.hover_pos()),
        //     ) {
        //         // let start = transform.inverse() * start;
        //         // let end = transform.inverse() * end;

        //         ui.painter().circle_filled(start, 3.0, Color32::RED);
        //         ui.painter().circle_filled(end, 3.0, Color32::RED);
        //     }
        // }

        // User clicked on background
        if response.is_pointer_button_down_on()
            && ui.input(|mem| mem.pointer.primary_pressed())
            && !anything_focused
        {
            selected = None;
        }

        ui.memory_mut(|mem| mem.data.insert_temp(selected_id, selected));

        NodeGraphResponse {
            selected,
            activated,
        }
    }
}

#[derive(Debug, Clone)]
struct DragPayload(Pos2, NodeId, PayloadPin);

#[derive(Debug, Clone)]
enum PayloadPin {
    Output(OutputId, TypeId),
    Input(InputId),
}

fn line_intersects(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> Option<Pos2> {
    let enu = (a.x - c.x) * (c.y - d.y) - (a.y - c.y) * (c.x - d.x);
    let den = (a.x - b.x) * (c.y - d.y) - (a.y - b.y) * (c.x - d.x);
    let t = enu / den;

    let enu = (a.x - b.x) * (a.y - c.y) - (a.y - b.y) * (a.x - c.x);
    let den = (a.x - b.x) * (c.y - d.y) - (a.y - b.y) * (c.x - d.x);
    let u = enu / den;

    if t < 0.0 || t > 1.0 || u < -0.1 || u > 1.1 {
        return None;
    }

    Some(a + t * (b - a))
}
