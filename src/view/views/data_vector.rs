use std::sync::Arc;

use anyhow::anyhow;
use egui_plot::{Line, Plot, PlotPoints};
use tokio::sync::watch;

use super::prelude::*;
use types::DataVector;

#[derive(Clone)]
pub struct View {
    input: NodeOutput,

    data_rx: Option<watch::Receiver<Option<Arc<DataVector>>>>,
}

impl DataView for View {
    type InputId = InputIdSingle;

    fn from_node_output(node_output: &NodeOutput, _pipeline: &Pipeline) -> Option<Self> {
        if node_output.type_id == PipelineDataType::DataVector.into() {
            Some(Self {
                input: *node_output,
                data_rx: None,
            })
        } else {
            None
        }
    }

    fn changed(&self, _other: &Self) -> bool {
        false
    }

    fn inputs(&self) -> impl Iterator<Item = (Self::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, Some(self.input)))
    }

    fn connect(&mut self, node_output: NodeOutput, _pipeline: &Pipeline) -> bool {
        if node_output.type_id == PipelineDataType::DataVector.into() {
            self.input = node_output;
            true
        } else {
            false
        }
    }

    fn disconnect(&mut self, _input_id: Self::InputId) -> Existence {
        Existence::Destroy
    }

    fn create_view_task(&mut self) -> impl DataViewTask<InputId = Self::InputId, DataView = Self> {
        let (data_tx, data_rx) = watch::channel(None);

        self.data_rx = Some(data_rx);

        Task {
            input: TaskInput::default(),
            data_tx,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some(data_rx) = &mut self.data_rx {
            let changed = data_rx.has_changed().unwrap_or(false);

            if let Some(data) = data_rx.borrow_and_update().as_ref() {
                let mut plot = Plot::new("Data Vector").allow_scroll(false);

                if changed {
                    plot = plot.reset();
                }

                plot.show(ui, |plot_ui| {
                    plot_ui.line(Line::new(PlotPoints::from_ys_f32(
                        data.as_ref().clone().cast().as_slice(),
                    )));
                });
            } else {
                ui.label("No data available");
            }
        } else {
            ui.label("No data receiver available");
        }
    }
}

struct Task {
    input: TaskInput<requests::VectorData>,

    data_tx: watch::Sender<Option<Arc<DataVector>>>,
}

impl DataViewTask for Task {
    type InputId = InputIdSingle;
    type DataView = View;

    fn connect(&mut self, _input_id: Self::InputId, input: &mut ConnectionHandle) {
        self.input.connect(input);
    }

    fn disconnect(&mut self, _input_id: Self::InputId) {
        self.input.disconnect();
    }

    fn invalidate(&mut self) {
        self.data_tx.send_if_modified(|v| match v {
            Some(_) => {
                *v = None;
                true
            }
            None => false,
        });
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        if self.input.is_connected() && self.data_tx.borrow().is_none() {
            let Some(data) = self.input.request(requests::VectorData).await else {
                return Err(anyhow!("No input data available"));
            };

            self.data_tx.send(Some(data))?;
        }

        Ok(())
    }
}
