pub mod views;
pub mod views_manager;

use std::collections::HashMap;

use crate::{node_graph::NodeOutput, pipeline::Pipeline};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId(usize);

impl From<usize> for ViewId {
    fn from(id: usize) -> Self {
        ViewId(id)
    }
}

impl Into<usize> for ViewId {
    fn into(self) -> usize {
        self.0
    }
}

pub struct DataViewsState {
    views: HashMap<ViewId, Box<dyn DataView>>,
}

impl DataViewsState {
    pub fn new() -> Self {
        Self {
            views: HashMap::new(),
        }
    }

    pub fn get_new_view_id(&self) -> ViewId {
        self.views
            .keys()
            .max()
            .map_or(ViewId::from(0), |&id| (Into::<usize>::into(id) + 1).into())
    }

    fn add_view(&mut self, view: Box<dyn DataView>) -> ViewId {
        let id = self.get_new_view_id();
        self.views.insert(id, view);
        id
    }

    pub fn get(&self, view_id: ViewId) -> Option<&dyn DataView> {
        self.views.get(&view_id).map(|v| v.as_ref())
    }

    pub fn get_mut(&mut self, view_id: ViewId) -> Option<&mut dyn DataView> {
        self.views.get_mut(&view_id).map(|v| v.as_mut())
    }
}

pub trait DataView: 'static {
    fn from_node_output(node_output: &NodeOutput, pipeline: &Pipeline) -> Option<Self>
    where
        Self: Sized;

    fn connect(&mut self, node_output: NodeOutput, pipeline: &Pipeline) -> bool;

    fn ui(&mut self, ui: &mut egui::Ui);
}
