mod add_node_popup;
mod frame;
mod node_graph_editor;

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use frame::NodeFrameState;
pub use node_graph_editor::*;

use egui::{pos2, Align, Color32, InnerResponse, Label, Layout, Pos2, Response, Vec2, WidgetText};

use crate::node_graph::*;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NodeGraphEditState {
    node_states: HashMap<NodeId, NodeFrameState>,
    node_order: Vec<NodeId>,
}

impl NodeGraphEditState {
    pub fn new() -> Self {
        Self {
            node_states: HashMap::new(),
            node_order: Vec::new(),
        }
    }

    pub fn sync_state(&mut self, node_ids: &[NodeId]) {
        self.node_states
            .retain(|node_id, _| node_ids.contains(node_id));

        self.node_order.retain(|node_id| node_ids.contains(node_id));

        let mut cursor = self
            .node_states
            .values()
            .map(|n| pos2(n.position.x + 230.0, n.position.y))
            .reduce(|a, b| pos2(a.x.max(b.x), a.y.min(b.y)))
            .unwrap_or_default();

        for node_id in node_ids {
            self.node_states.entry(*node_id).or_insert_with(|| {
                let state = NodeFrameState {
                    position: egui::Pos2::new(cursor.x, cursor.y),
                };
                cursor.x += 230.0;
                state
            });

            if !self.node_order.contains(node_id) {
                self.node_order.push(*node_id);
            }
        }
    }

    pub fn to_top(&mut self, node_id: NodeId) {
        self.node_order.retain(|id| id != &node_id);
        self.node_order.push(node_id);
    }
}

pub trait EditNodeGraph {
    fn get_node_ids(&self) -> Vec<NodeId>;

    fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut (dyn DynEditNode)>;

    fn remove_node(&mut self, node_id: NodeId);

    fn add_node(&mut self, path: &str) -> NodeId;

    fn addable_nodes(&self) -> Vec<&'static str>;
}

pub trait EditNode {
    type OutputId: Into<OutputId> + From<OutputId>;
    type InputId: Into<InputId> + From<InputId>;

    fn name(&self) -> &str;

    fn color(&self) -> Color32;

    fn connect(&mut self, input: Self::InputId, connection: NodeOutput);

    fn disconnect(&mut self, input: Self::InputId);

    fn ui(&mut self, ui: &mut NodeUi);
}

/// Auto-trait
pub trait DynEditNode {
    fn name(&self) -> &str;

    fn color(&self) -> Color32;

    fn connect(&mut self, input: InputId, connection: NodeOutput);

    fn disconnect(&mut self, input: InputId);

    fn ui(&mut self, ui: &mut NodeUi);
}

impl<T: EditNode> DynEditNode for T {
    fn name(&self) -> &str {
        self.name()
    }

    fn color(&self) -> Color32 {
        self.color()
    }

    fn connect(&mut self, input: InputId, connection: NodeOutput) {
        self.connect(input.into(), connection)
    }

    fn disconnect(&mut self, input: InputId) {
        self.disconnect(input.into())
    }

    fn ui(&mut self, ui: &mut NodeUi) {
        self.ui(ui)
    }
}

pub struct NodeUi<'a> {
    ui: &'a mut egui::Ui,
    inputs: &'a mut Vec<CollectedInput>,
    outputs: &'a mut Vec<CollectedOutput>,
}

pub(super) struct CollectedInput {
    pub id: InputId,
    pub pos: Pos2,
    pub color: Color32,
    pub connection: Option<NodeOutput>,
}

pub(super) struct CollectedOutput {
    pub id: OutputId,
    pub type_: TypeId,
    pub pos: Pos2,
    pub color: Color32,
}

impl NodeUi<'_> {
    pub fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    pub fn input(
        &mut self,
        id: impl Into<InputId>,
        connection: Option<NodeOutput>,
        color: impl Into<Color32>,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) {
        let InnerResponse {
            response: Response { rect, .. },
            ..
        } = self
            .ui
            .allocate_ui(Vec2::new(self.ui.available_width(), 18.0), add_contents);

        let pin_pos = rect.left_top() + Vec2::new(-10.0, 9.0);

        self.inputs.push(CollectedInput {
            id: id.into(),
            pos: pin_pos,
            color: color.into(),
            connection,
        });
    }

    pub fn output(
        &mut self,
        id: impl Into<OutputId>,
        type_: impl Into<TypeId>,
        color: impl Into<Color32>,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) {
        let rect = self
            .ui
            .allocate_ui(Vec2::new(self.ui.available_width(), 18.0), |ui| {
                ui.with_layout(
                    Layout {
                        cross_align: Align::Max,
                        ..*ui.layout()
                    },
                    add_contents,
                );
            })
            .response
            .rect;

        let pin_pos = rect.right_top() + Vec2::new(10.0, 9.0);

        self.outputs.push(CollectedOutput {
            id: id.into(),
            type_: type_.into(),
            pos: pin_pos,
            color: color.into(),
        });
    }
}

impl Deref for NodeUi<'_> {
    type Target = egui::Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl DerefMut for NodeUi<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

pub trait UiNodeExt {
    fn node_label(&mut self, text: impl Into<WidgetText>) -> Response;
}

impl UiNodeExt for egui::Ui {
    fn node_label(&mut self, text: impl Into<WidgetText>) -> Response {
        self.add_space(3.5);
        self.add(Label::new(text).selectable(false))
    }
}
