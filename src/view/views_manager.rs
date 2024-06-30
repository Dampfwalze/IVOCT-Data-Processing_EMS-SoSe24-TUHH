use crate::{
    gui::dock_state::{DockState, TabType},
    node_graph::{NodeId, NodeOutput},
    pipeline::{self, Pipeline},
};

use super::{
    views::{DynDataView, Existence},
    DataView, DataViewsState, ViewId,
};

pub struct DataViewsManager {
    view_factories: Vec<Box<dyn Fn(&NodeOutput, &Pipeline) -> Option<Box<dyn DynDataView>>>>,
    last_focused_view: Option<ViewId>,
}

impl DataViewsManager {
    pub fn new() -> Self {
        Self {
            view_factories: Vec::new(),
            last_focused_view: None,
        }
    }

    pub fn with_view<T: DataView>(mut self) -> Self {
        self.view_factories.push(Box::new(|o, p| {
            T::from_node_output(o, p).map(|v| Box::new(v) as Box<dyn DynDataView>)
        }));
        self
    }

    pub fn update(
        &mut self,
        state: &mut DataViewsState,
        pipeline: &mut pipeline::Pipeline,
        dock_state: &mut DockState,
        interacted_node: Option<NodeId>,
        ctrl_pressed: bool,
    ) {
        // Remove closed views
        state.views.retain(|id, _| {
            dock_state.iter_all_tabs().any(|(_, tab)| match tab {
                TabType::DataView(view_id) => *view_id == *id,
                _ => false,
            })
        });

        // Disconnect from removed nodes
        let mut to_destroy = Vec::new();
        for (view_id, view) in state.views.iter_mut() {
            for (input_id, node_id) in view
                .inputs()
                .iter()
                .filter_map(|(id, out)| out.map(|o| (*id, o.node_id)))
            {
                if !pipeline.nodes.contains_key(&node_id) {
                    if let Existence::Destroy = view.disconnect(input_id) {
                        to_destroy.push(*view_id);
                        break;
                    }
                }
            }
        }
        for view_id in to_destroy {
            state.views.remove(&view_id);
        }

        // Track last focused view
        if let Some((_, TabType::DataView(view_id))) = dock_state.find_active_focused() {
            self.last_focused_view = Some(*view_id);
        }

        if let Some(interacted_node_id) = interacted_node {
            let Some((output_id, type_id)) =
                pipeline[interacted_node_id].get_output_for_view_request()
            else {
                return;
            };

            let node_output = NodeOutput {
                node_id: interacted_node_id,
                output_id,
                type_id,
            };

            if ctrl_pressed {
                self.create_and_open_view(state, dock_state, &node_output, pipeline);
            } else if let Some(view_id) =
                self.try_connect_view(state, dock_state, node_output, pipeline)
            {
                dock_state.focus_view(view_id);
            } else {
                self.create_and_open_view(state, dock_state, &node_output, pipeline);
            }
        }
    }

    fn create_and_open_view(
        &self,
        state: &mut DataViewsState,
        dock_state: &mut DockState,
        node_output: &NodeOutput,
        pipeline: &Pipeline,
    ) {
        if let Some(view) = self.create_view(node_output, pipeline) {
            let view_id = state.add_view(view);

            dock_state.add_view_tab(view_id);
        }
    }

    fn create_view(
        &self,
        node_output: &NodeOutput,
        pipeline: &Pipeline,
    ) -> Option<Box<dyn DynDataView>> {
        for factory in &self.view_factories {
            if let Some(view) = factory(node_output, pipeline) {
                return Some(view);
            }
        }

        None
    }

    fn try_connect_view(
        &self,
        state: &mut DataViewsState,
        dock_state: &mut DockState,
        node_output: NodeOutput,
        pipeline: &Pipeline,
    ) -> Option<ViewId> {
        if let Some(view_id) = self.last_focused_view {
            // Test focused view
            if let Some(view) = state.get_mut(view_id) {
                if view.connect(node_output, pipeline) {
                    return Some(view_id);
                }
            }

            // Test remaining views in node
            if let Some((surf_idx, node_idx)) =
                dock_state
                    .iter_all_tabs()
                    .find_map(|(surf_node, tab)| match tab {
                        TabType::DataView(id) if *id == view_id => Some(surf_node),
                        _ => None,
                    })
            {
                let node = &dock_state[surf_idx][node_idx];

                for view_id in node.iter_tabs().filter_map(|t| match t {
                    TabType::DataView(id) => Some(*id),
                    _ => None,
                }) {
                    if let Some(view) = state.get_mut(view_id) {
                        if view.connect(node_output, pipeline) {
                            return Some(view_id);
                        }
                    }
                }
            }
        }

        // Test all views
        for (view_id, view) in state.views.iter_mut() {
            if view.connect(node_output, pipeline) {
                return Some(*view_id);
            }
        }

        None
    }
}
