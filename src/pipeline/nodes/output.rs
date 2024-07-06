use std::{path::PathBuf, sync::Arc};

use anyhow::anyhow;
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{watch, Notify},
};

use crate::{pipeline::types::DataType, queue_channel::error::RecvError};

use super::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Progress {
    Idle,
    Working(Option<f32>),
}

// MARK: Node

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub path: PathBuf,
    pub input_type: PipelineDataType,
    pub scan_data_type: DataType,
    #[serde(skip)]
    pub notify: Arc<Notify>,

    pub input: NodeInput<()>,

    #[serde(skip)]
    pub progress_rx: Option<watch::Receiver<Progress>>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            input_type: PipelineDataType::RawMScan,
            scan_data_type: DataType::U16,
            input: NodeInput::default(),
            notify: Arc::new(Notify::new()),
            progress_rx: None,
        }
    }
}

impl Node {
    pub fn save(&mut self) {
        self.notify.notify_waiters();
    }
}

deserialize_node!(Node, "output");

impl PipelineNode for Node {
    type InputId = InputIdSingle;
    type OutputId = OutputIdNone;

    fn slug() -> &'static str {
        "output"
    }

    fn changed(&self, other: &Self) -> bool {
        self.path != other.path
            || self.input_type != other.input_type
            || self.scan_data_type != other.scan_data_type
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, self.input.connection()))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let (progress_tx, progress_rx) = watch::channel(Progress::Idle);

        self.progress_rx = Some(progress_rx);

        builder.task(Task {
            path: self.path.clone(),
            scan_data_type: self.scan_data_type,
            notifier: self.notify.clone(),
            progress_tx,
            input: match self.input_type {
                PipelineDataType::RawMScan => TaskInputType::RawMScan(TaskInput::default()),
                PipelineDataType::DataVector => TaskInputType::DataVector(TaskInput::default()),
                PipelineDataType::MScan => TaskInputType::MScan(TaskInput::default()),
                PipelineDataType::BScanSegmentation => {
                    TaskInputType::BScanSegmentation(TaskInput::default())
                }
                PipelineDataType::MScanSegmentation => {
                    TaskInputType::MScanSegmentation(TaskInput::default())
                }
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
    BScanSegmentation(TaskInput<requests::BScanSegmentation>),
    MScanSegmentation(TaskInput<requests::MScanSegmentation>),
}

impl TaskInputType {
    pub fn disconnect(&mut self) {
        match self {
            TaskInputType::RawMScan(input) => input.disconnect(),
            TaskInputType::DataVector(input) => input.disconnect(),
            TaskInputType::MScan(input) => input.disconnect(),
            TaskInputType::BScanSegmentation(input) => input.disconnect(),
            TaskInputType::MScanSegmentation(input) => input.disconnect(),
        }
    }
}

struct Task {
    path: PathBuf,
    scan_data_type: DataType,
    notifier: Arc<Notify>,

    input: TaskInputType,

    progress_tx: watch::Sender<Progress>,
}

impl NodeTask for Task {
    type InputId = InputIdSingle;
    type PipelineNode = Node;

    fn invalidate(&mut self, _cause: InvalidationCause) {
        let _ = self.progress_tx.send(Progress::Idle);
    }

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
                PipelineDataType::BScanSegmentation => {
                    let mut task_input = TaskInput::<requests::BScanSegmentation>::default();
                    if task_input.connect(input) {
                        resulting = Some(TaskInputType::BScanSegmentation(task_input));
                        break;
                    }
                }
                PipelineDataType::MScanSegmentation => {
                    let mut task_input = TaskInput::<requests::MScanSegmentation>::default();
                    if task_input.connect(input) {
                        resulting = Some(TaskInputType::MScanSegmentation(task_input));
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
        self.scan_data_type = node.scan_data_type;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        self.notifier.notified().await;

        match &mut self.input {
            TaskInputType::RawMScan(input) => {
                let mut file = fs::File::create(&self.path).await?;

                let Some(res) = input.request(requests::RawMScan).await else {
                    return Ok(());
                };

                let Some(mut rx) = res.data.subscribe() else {
                    return Err(anyhow!("Failed to subscribe to RawMScan"));
                };

                let _ = self.progress_tx.send(Progress::Working(None));

                let mut a_scan_count = 0;

                loop {
                    let scan = match rx.recv().await {
                        Err(RecvError::Closed) => break,
                        Err(e) => Err(e)?,
                        Ok(scan) => scan,
                    };

                    let scan = scan.cast_rescale_par(self.scan_data_type);

                    file.write_all(scan.as_u8_slice()).await?;

                    a_scan_count += scan.ncols();
                    let _ = self.progress_tx.send(Progress::Working(Some(
                        a_scan_count as f32 / res.a_scan_count as f32,
                    )));
                }
                let _ = self.progress_tx.send(Progress::Idle);
            }
            TaskInputType::DataVector(input) => {
                let Some(data) = input.request(requests::VectorData).await else {
                    return Ok(());
                };

                let mut file = fs::File::create(&self.path).await?;

                file.write_all(data.as_u8_slice()).await?;
            }
            TaskInputType::MScan(input) => {
                let mut file = fs::File::create(&self.path).await?;

                let Some(res) = input.request(requests::MScan).await else {
                    return Ok(());
                };

                let Some(mut rx) = res.data.subscribe() else {
                    return Err(anyhow!("Failed to subscribe to MScan"));
                };

                let _ = self.progress_tx.send(Progress::Working(None));

                let mut a_scan_count = 0;

                loop {
                    let scan = match rx.recv().await {
                        Err(RecvError::Closed) => break,
                        Err(e) => Err(e)?,
                        Ok(scan) => scan,
                    };

                    let scan = scan.cast_rescale_par(self.scan_data_type);

                    file.write_all(scan.as_u8_slice()).await?;

                    a_scan_count += scan.ncols();
                    let _ = self.progress_tx.send(Progress::Working(Some(
                        a_scan_count as f32 / res.a_scan_count as f32,
                    )));
                }
                let _ = self.progress_tx.send(Progress::Idle);
            }
            TaskInputType::BScanSegmentation(input) => {
                let mut file = fs::File::create(&self.path).await?;

                let Some(res) = input.request(requests::BScanSegmentation).await else {
                    return Ok(());
                };

                let Some(mut rx) = res.subscribe() else {
                    return Err(anyhow!("Failed to subscribe to BScanSegmentation"));
                };

                let _ = self.progress_tx.send(Progress::Working(None));

                loop {
                    let value = match rx.recv().await {
                        Err(RecvError::Closed) => break,
                        Err(e) => Err(e)?,
                        Ok(scan) => scan,
                    };

                    file.write_all(bytemuck::cast_slice(&[value as u32]))
                        .await?;
                }
                let _ = self.progress_tx.send(Progress::Idle);
            }
            TaskInputType::MScanSegmentation(input) => {
                let mut file = fs::File::create(&self.path).await?;

                let Some(res) = input.request(requests::MScanSegmentation).await else {
                    return Ok(());
                };

                let Some(mut rx) = res.subscribe() else {
                    return Err(anyhow!("Failed to subscribe to MScanSegmentation"));
                };

                let _ = self.progress_tx.send(Progress::Working(None));

                loop {
                    let value = match rx.recv().await {
                        Err(RecvError::Closed) => break,
                        Err(e) => Err(e)?,
                        Ok(scan) => scan,
                    };

                    file.write_all(bytemuck::cast_slice(value.as_slice()))
                        .await?;
                }
                let _ = self.progress_tx.send(Progress::Idle);
            }
        }

        Ok(())
    }
}
