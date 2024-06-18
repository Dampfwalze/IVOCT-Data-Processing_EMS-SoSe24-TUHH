use crate::{
    gui::node_graph::{NodeGraphEditState, NodeGraphEditor},
    pipeline,
};

pub struct IVOCTTestApp {
    pipeline: pipeline::Pipeline,
    pipeline_edit_state: NodeGraphEditState,
}

impl IVOCTTestApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> IVOCTTestApp {
        IVOCTTestApp {
            pipeline: pipeline::Pipeline::new(),
            pipeline_edit_state: NodeGraphEditState::new(),
        }
    }
}

impl eframe::App for IVOCTTestApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            NodeGraphEditor::new(&mut self.pipeline, &mut self.pipeline_edit_state).show(ui);
        });
    }
}

// mod test {
//     use std::collections::HashMap;

//     use egui::{Color32, DragValue, Label};

//     use crate::{
//         gui::node_graph::{DynEditNode, EditNode, EditNodeGraph, NodeUi, UiNodeExt},
//         node_graph::*,
//     };

//     pub struct Pipeline {
//         pub nodes: HashMap<NodeId, Box<dyn DynEditNode>>,
//     }

//     impl Pipeline {
//         pub fn new() -> Self {
//             let mut nodes = HashMap::<NodeId, Box<dyn DynEditNode>>::new();

//             nodes.insert(0.into(), Box::new(TestNode::default()));
//             nodes.insert(1.into(), Box::new(TestNode::default()));

//             Self { nodes }
//         }
//     }

//     impl EditNodeGraph for Pipeline {
//         fn get_node_ids(&self) -> Vec<NodeId> {
//             self.nodes.keys().copied().collect()
//         }

//         fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut (dyn DynEditNode)> {
//             match self.nodes.get_mut(&node_id) {
//                 Some(node) => Some(node.as_mut()),
//                 None => None,
//             }
//         }

//         fn remove_node(&mut self, node_id: NodeId) {
//             self.nodes.remove(&node_id);
//         }

//         fn add_node(&mut self, path: &str) -> NodeId {
//             let id: usize = self.nodes.keys().copied().max().unwrap_or(0.into()).into();
//             let id = id + 1;

//             match path {
//                 "Test/Test Node 1" => {}
//                 "Test/Test Node 2" => {}
//                 "Test 2/Test Node 1" => {}
//                 "Test 2/Test Node 2" => {}
//                 _ => eprintln!("Invalid path: {}", path),
//             }

//             self.nodes.insert(id.into(), Box::new(TestNode::default()));

//             id.into()
//         }

//         fn addable_nodes(&self) -> Vec<&'static str> {
//             vec![
//                 "Test/Test Node 1",
//                 "Test/Test Node 2",
//                 "Test 2/Test Node 1",
//                 "Test 2/Test Node 2",
//             ]
//         }
//     }

//     #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
//     enum MyTypeId {
//         Float,
//         Int,
//         String,
//     }

//     impl Into<TypeId> for MyTypeId {
//         fn into(self) -> TypeId {
//             match self {
//                 MyTypeId::Float => 0.into(),
//                 MyTypeId::Int => 1.into(),
//                 MyTypeId::String => 2.into(),
//             }
//         }
//     }

//     impl From<TypeId> for MyTypeId {
//         fn from(t: TypeId) -> Self {
//             match t.into() {
//                 0 => MyTypeId::Float,
//                 1 => MyTypeId::Int,
//                 2 => MyTypeId::String,
//                 _ => panic!("Invalid type id"),
//             }
//         }
//     }

//     #[derive(Default)]
//     pub struct TestNode {
//         value1: NodeInput<f32>,
//         value2: NodeInput<i32>,
//     }

//     impl EditNode for TestNode {
//         type OutputId = OutputId;
//         type InputId = InputId;

//         fn name(&self) -> &str {
//             "Test Node"
//         }

//         fn color(&self) -> Color32 {
//             Color32::from_rgb(0, 0, 0)
//         }

//         fn connect(&mut self, input: InputId, connection: NodeOutput) {
//             match (input.into(), connection.type_id.into()) {
//                 (0, MyTypeId::Float) => self.value1.connect(connection),
//                 (1, MyTypeId::Int) => self.value2.connect(connection),
//                 (i, t) => println!("Invalid connection: {i} -> {t:?}"),
//             }
//         }

//         fn disconnect(&mut self, input: InputId) {
//             match input.into() {
//                 0 => self.value1.disconnect(),
//                 1 => self.value2.disconnect(),
//                 _ => panic!("Invalid input id"),
//             }
//         }

//         fn ui(&mut self, ui: &mut NodeUi) {
//             ui.output(0, MyTypeId::Float, Color32::YELLOW, |ui| {
//                 ui.node_label("Output 1");
//             });
//             ui.output(1, MyTypeId::Int, Color32::BLUE, |ui| {
//                 ui.node_label("Output 2");
//             });

//             ui.input(0, self.value1.connection(), Color32::RED, |ui| {
//                 match self.value1.connection() {
//                     Some(_) => {
//                         ui.node_label("Input 1");
//                     }
//                     None => {
//                         ui.add(DragValue::new(self.value1.value_mut()).prefix("Value 1: "));
//                     }
//                 }
//             });
//             ui.input(1, self.value2.connection(), Color32::GREEN, |ui| match self
//                 .value2
//                 .connection()
//             {
//                 Some(_) => {
//                     ui.node_label("Input 2");
//                 }
//                 None => {
//                     ui.add(DragValue::new(self.value2.value_mut()).prefix("Value 2: "));
//                 }
//             });
//         }
//     }
// }
