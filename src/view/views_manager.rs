use egui_dock::TabIndex;

use crate::{
    gui::dock_state::{DockState, TabType},
    node_graph::{NodeId, NodeOutput},
    pipeline::{self, Pipeline},
};

use super::{DataView, DataViewsState, ViewId};

pub struct DataViewsManager {
    view_factories: Vec<Box<dyn Fn(&NodeOutput, &Pipeline) -> Option<Box<dyn DataView>>>>,
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
            T::from_node_output(o, p).map(|v| Box::new(v) as Box<dyn DataView>)
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
                let Some(view) = self.create_view(&node_output, pipeline) else {
                    return;
                };

                let view_id = state.add_view(view);

                dock_state.add_view_tab(view_id);
            } else {
                if let Some(view_id) =
                    self.try_connect_views(state, dock_state, node_output, pipeline)
                {
                    if let Some((surf_idx, node_idx)) = {
                        let mut iter = dock_state.iter_all_tabs();
                        iter.find_map(|(surf_node, tab)| match tab {
                            TabType::DataView(id) if *id == view_id => Some(surf_node),
                            _ => None,
                        })
                    } {
                        dock_state.set_focused_node_and_surface((surf_idx, node_idx));

                        // Focus tab inside node
                        if let egui_dock::Node::<TabType>::Leaf { tabs, active, .. } =
                            &mut dock_state[surf_idx][node_idx]
                        {
                            *active = tabs
                                .iter()
                                .position(|t| matches!(t, TabType::DataView(id) if *id == view_id))
                                .map(TabIndex)
                                .unwrap();
                        }
                    }
                }
            }
        }
    }

    fn create_view(
        &self,
        node_output: &NodeOutput,
        pipeline: &Pipeline,
    ) -> Option<Box<dyn DataView>> {
        for factory in &self.view_factories {
            if let Some(view) = factory(node_output, pipeline) {
                return Some(view);
            }
        }

        None
    }

    fn try_connect_views(
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
