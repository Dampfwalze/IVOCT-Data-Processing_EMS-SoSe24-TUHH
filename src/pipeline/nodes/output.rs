use std::{path::PathBuf, sync::Arc};

use anyhow::anyhow;
use tokio::{fs, io::AsyncWriteExt, sync::Notify};

use crate::queue_channel::error::RecvError;

use super::prelude::*;

// MARK: Node

#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    pub input_type: PipelineDataType,
    pub notify: Arc<Notify>,

    pub input: NodeInput<()>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            input_type: PipelineDataType::RawMScan,
            input: NodeInput::default(),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl Node {
    pub fn save(&mut self) {
        self.notify.notify_waiters();
    }
}

impl PipelineNode for Node {
    type InputId = InputIdSingle;
    type OutputId = OutputIdNone;

    fn changed(&self, other: &Self) -> bool {
        self.path != other.path || self.input_type != other.input_type
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, self.input.connection()))
    }

    fn create_node_task(&self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        builder.task(Task {
            path: self.path.clone(),
            notifier: self.notify.clone(),
            input: match self.input_type {
                PipelineDataType::RawMScan => TaskInputType::RawMScan(TaskInput::default()),
                PipelineDataType::DataVector => TaskInputType::DataVector(TaskInput::default()),
                PipelineDataType::MScan => TaskInputType::MScan(TaskInput::default()),
            },
        });
    }
}

// MARK: NodeTask

#[derive(Debug)]
enum TaskInputType {
    RawMScan(TaskInput<requests::RawMScan>),
    DataVector(TaskInput<requests::VectorData>),
    MScan(TaskInput<requests::MScan>),
}

impl TaskInputType {
    pub fn disconnect(&mut self) {
        match self {
            TaskInputType::RawMScan(input) => input.disconnect(),
            TaskInputType::DataVector(input) => input.disconnect(),
            TaskInputType::MScan(input) => input.disconnect(),
        }
    }
}

struct Task {
    path: PathBuf,
    notifier: Arc<Notify>,

    input: TaskInputType,
}

impl NodeTask for Task {
    type InputId = InputIdSingle;
    type PipelineNode = Node;

    fn connect(&mut self, _input_id: Self::InputId, input: &mut ConnectionHandle) {
        let mut resulting = None;

        for t in PipelineDataType::VALUES.iter() {
            match t {
                PipelineDataType::RawMScan => {
                    let mut task_input = TaskInput::<requests::RawMScan>::default();
                    if task_input.connect(input) {
                        resulting = Some(TaskInputType::RawMScan(task_input));
                        break;
                    }
                }
                PipelineDataType::DataVector => {
                    let mut task_input = TaskInput::<requests::VectorData>::default();
                    if task_input.connect(input) {
                        resulting = Some(TaskInputType::DataVector(task_input));
                        break;
                    }
                }
                PipelineDataType::MScan => {
                    let mut task_input = TaskInput::<requests::MScan>::default();
                    if task_input.connect(input) {
                        resulting = Some(TaskInputType::MScan(task_input));
                        break;
                    }
                }
            }
        }

        if let Some(resulting) = resulting {
            self.input = resulting;
        } else {
            self.input.disconnect();
        }
    }

    fn disconnect(&mut self, _input_id: Self::InputId) {
        self.input.disconnect();
    }

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.path = node.path.clone();
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        self.notifier.notified().await;

        match &mut self.input {
            TaskInputType::RawMScan(input) => {
                let Some(res) = input.request(requests::RawMScan).await else {
                    return Ok(());
                };

                let mut file = fs::File::create(&self.path).await?;

                let Some(mut rx) = res.data.subscribe() else {
                    return Err(anyhow!("Failed to subscribe to RawMScan"));
                };

                loop {
                    let scan = match rx.recv().await {
                        Err(RecvError::Closed) => break,
                        Err(e) => Err(e)?,
                        Ok(scan) => scan,
                    };

                    file.write_all(scan.as_u8_slice()).await?;
                }
            }
            TaskInputType::DataVector(input) => {
                let Some(data) = input.request(requests::VectorData).await else {
                    return Ok(());
                };

                let mut file = fs::File::create(&self.path).await?;

                file.write_all(data.as_u8_slice()).await?;
            }
            TaskInputType::MScan(input) => {
                let Some(res) = input.request(requests::MScan).await else {
                    return Ok(());
                };

                let Some(mut rx) = res.data.subscribe() else {
                    return Err(anyhow!("Failed to subscribe to RawMScan"));
                };

                let mut file = fs::File::create(&self.path).await?;

                loop {
                    let scan = match rx.recv().await {
                        Err(RecvError::Closed) => break,
                        Err(e) => Err(e)?,
                        Ok(scan) => scan,
                    };

                    file.write_all(scan.as_u8_slice()).await?;
                }
            }
        }

        Ok(())
    }
}
