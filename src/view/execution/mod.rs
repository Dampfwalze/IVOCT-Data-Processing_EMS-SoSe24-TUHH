pub mod executor;

use futures::{future::BoxFuture, Future};

use crate::{node_graph::InputId, pipeline::execution::ConnectionHandle};

use super::views::{DataView, DynDataView};

pub trait DataViewTask: Send + Sync + 'static {
    type InputId: From<InputId> + Into<InputId>;
    type DataView: DataView;

    fn sync_node(&mut self, _node: &Self::DataView) {}

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle);

    fn disconnect(&mut self, input_id: Self::InputId);

    fn invalidate(&mut self) {}

    fn run(&mut self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

pub trait DynDataViewTask: Send + Sync {
    fn sync_view(&mut self, node: &dyn DynDataView);

    fn connect(&mut self, input_id: InputId, input: &mut ConnectionHandle);

    fn disconnect(&mut self, input_id: InputId);

    fn invalidate(&mut self);

    fn run(&mut self) -> BoxFuture<'_, anyhow::Result<()>>;
}

impl<T: DataViewTask> DynDataViewTask for T {
    fn sync_view(&mut self, node: &dyn DynDataView) {
        let node = node
            .as_any()
            .downcast_ref::<T::DataView>()
            .expect("node should be of type T::DataView");
        self.sync_node(node);
    }

    fn connect(&mut self, input_id: InputId, input: &mut ConnectionHandle) {
        self.connect(input_id.into(), input)
    }

    fn disconnect(&mut self, input_id: InputId) {
        self.disconnect(input_id.into())
    }

    fn invalidate(&mut self) {
        self.invalidate()
    }

    fn run(&mut self) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(self.run())
    }
}
