use std::{
    ops::{Deref, DerefMut},
    ptr,
};

use egui_dock::{NodeIndex, TabIndex};

use crate::view::ViewId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TabType {
    Pipeline,
    DataView(ViewId),
}

pub struct DockState(egui_dock::DockState<TabType>);

impl DockState {
    pub fn new() -> Self {
        Self(egui_dock::DockState::new(vec![TabType::Pipeline]))
    }

    pub fn add_view_tab(&mut self, view_id: ViewId) -> bool {
        if self.iter_all_tabs().any(|(_, tab)| match tab {
            TabType::DataView(id) => *id == view_id,
            _ => false,
        }) {
            return false;
        }

        if let Some((_, TabType::DataView(_))) = self.find_active_focused() {
            let (surface_id, node_id) = self.focused_leaf().unwrap();

            let node = &mut self[surface_id][node_id];

            node.append_tab(TabType::DataView(view_id));

            return true;
        } else if let Some((surf_idx, node)) = {
            let mut iter = self.iter_all_nodes_mut();
            iter.find(|(_, node)| {
                matches!(
                    node,
                    egui_dock::Node::Leaf {
                        tabs,
                        active: TabIndex(active),
                        ..
                    } if matches!(tabs[*active], TabType::DataView(_))
                )
            })
        } {
            // There is a node that has a DataView in focus

            node.append_tab(TabType::DataView(view_id));

            let node_ptr = node as *mut _;

            if let Some(node_idx) = self[surf_idx]
                .iter()
                .enumerate()
                .find(|(_, n)| ptr::eq(*n, node_ptr))
                .map(|(node_idx, _)| node_idx)
                .map(NodeIndex)
            {
                self.set_focused_node_and_surface((surf_idx, node_idx));
            }

            return true;
        }

        let surf = self.main_surface_mut();
        surf.split_above(NodeIndex::root(), 0.666, vec![TabType::DataView(view_id)]);

        true
    }

    pub fn focus_view(&mut self, view_id: ViewId) {
        if let Some((surf_idx, node_idx)) = {
            let mut iter = self.iter_all_tabs();
            iter.find_map(|(surf_node, tab)| match tab {
                TabType::DataView(id) if *id == view_id => Some(surf_node),
                _ => None,
            })
        } {
            self.set_focused_node_and_surface((surf_idx, node_idx));

            if let egui_dock::Node::Leaf { tabs, active, .. } = &mut self[surf_idx][node_idx] {
                *active = tabs
                    .iter()
                    .position(|t| matches!(t, TabType::DataView(id) if *id == view_id))
                    .map(TabIndex)
                    .expect("Tab should already be found in previous search");
            }
        }
    }
}

impl Deref for DockState {
    type Target = egui_dock::DockState<TabType>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DockState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
