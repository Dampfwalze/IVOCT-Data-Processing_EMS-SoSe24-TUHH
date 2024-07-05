pub mod execution;
pub mod views;
pub mod views_manager;

use core::fmt;
use std::collections::HashMap;

use views::{DataView, DynDataView};

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
    views: HashMap<ViewId, Box<dyn DynDataView>>,
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

    fn add_view(&mut self, view: Box<dyn DynDataView>) -> ViewId {
        let id = self.get_new_view_id();
        self.views.insert(id, view);
        id
    }

    pub fn get(&self, view_id: ViewId) -> Option<&dyn DynDataView> {
        self.views.get(&view_id).map(|v| v.as_ref())
    }

    pub fn get_mut(&mut self, view_id: ViewId) -> Option<&mut dyn DynDataView> {
        self.views.get_mut(&view_id).map(|v| v.as_mut())
    }

    pub fn clear(&mut self) {
        self.views.clear();
    }
}

impl fmt::Debug for DataViewsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataViewsState")
            .field("views", &self.views.keys().collect::<Vec<_>>())
            .finish()
    }
}
