pub mod data_vector;
pub mod m_scan;
pub mod mesh;

use std::any::{self, Any};

use eframe::egui_wgpu::RenderState;

use crate::{
    cache::Cache,
    node_graph::{InputId, NodeOutput},
    pipeline::Pipeline,
};

use super::execution::{DataViewTask, DynDataViewTask};

#[allow(unused_imports)]
mod prelude {
    pub(crate) use crate::{
        cache::Cache,
        node_graph::{InputIdSingle, NodeId, NodeOutput, TypeId},
        pipeline::{
            execution::{ConnectionHandle, InvalidationCause, Request, TaskInput},
            requests, types, Pipeline, PipelineDataType,
        },
        view::execution::DataViewTask,
    };

    pub(crate) use eframe::egui_wgpu::RenderState;

    pub(crate) use super::{DataView, Existence};

    pub(crate) use graph::*;

    pub(crate) mod graph {
        pub(crate) use crate::node_graph::{
            impl_enum_from_into_id_types, InputId, InputIdNone, InputIdSingle, NodeId, NodeInput,
            NodeOutput, OutputId, OutputIdNone, OutputIdSingle, TypeId,
        };
    }
}

pub enum Existence {
    Destroy,
    Keep,
}

pub trait DataView: Send + Sync + Clone + 'static {
    type InputId: From<InputId> + Into<InputId>;

    fn init_wgpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: &wgpu::TextureFormat,
    ) -> impl Any + Send + Sync + 'static {
        let _ = device;
        let _ = queue;
        let _ = target_format;
    }

    fn from_node_output(
        node_output: &NodeOutput,
        pipeline: &Pipeline,
        cache: &Cache,
        render_state: &RenderState,
    ) -> Option<Self>
    where
        Self: Sized;

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)>;

    fn changed(&self, other: &Self) -> bool;

    fn connect(&mut self, node_output: NodeOutput, pipeline: &Pipeline) -> bool;

    fn disconnect(&mut self, input_id: Self::InputId) -> Existence;

    fn create_view_task(&mut self) -> impl DataViewTask<InputId = Self::InputId, DataView = Self>;

    fn ui(&mut self, ui: &mut egui::Ui);
}

pub trait DynDataView: Send + Sync + 'static {
    fn as_any(&self) -> &dyn any::Any;

    fn clone_boxed(&self) -> Box<dyn DynDataView>;

    fn inputs(&self) -> Vec<(InputId, Option<NodeOutput>)>;

    fn changed(&self, other: &dyn DynDataView) -> bool;

    fn connect(&mut self, node_output: NodeOutput, pipeline: &Pipeline) -> bool;

    fn disconnect(&mut self, input_id: InputId) -> Existence;

    fn create_view_task(&mut self) -> Box<dyn DynDataViewTask>;

    fn ui(&mut self, ui: &mut egui::Ui);
}

impl<T: DataView> DynDataView for T {
    fn as_any(&self) -> &dyn any::Any {
        self
    }

    fn clone_boxed(&self) -> Box<dyn DynDataView> {
        Box::new(self.clone())
    }

    fn inputs(&self) -> Vec<(InputId, Option<NodeOutput>)> {
        self.inputs().map(|(id, conn)| (id.into(), conn)).collect()
    }

    fn changed(&self, other: &dyn DynDataView) -> bool {
        match other.as_any().downcast_ref::<T>() {
            Some(other) => self.changed(other),
            None => false,
        }
    }

    fn connect(&mut self, node_output: NodeOutput, pipeline: &Pipeline) -> bool {
        self.connect(node_output, pipeline)
    }

    fn disconnect(&mut self, input_id: InputId) -> Existence {
        self.disconnect(input_id.into())
    }

    fn create_view_task(&mut self) -> Box<dyn DynDataViewTask> {
        Box::new(self.create_view_task())
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        self.ui(ui)
    }
}
