use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::FutureExt;
use tokio::{fs, io::AsyncReadExt, sync::watch};

use crate::pipeline::types::{DataMatrix, DataType, DataVector};

use super::prelude::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OutputId {
    #[default]
    RawMScan,
    MScan,
    DataVector,
}

impl OutputId {
    pub const VALUES: [OutputId; 3] = [OutputId::RawMScan, OutputId::MScan, OutputId::DataVector];
}

impl_enum_from_into_id_types!(OutputId, [graph::OutputId], {
    0 => RawMScan,
    1 => MScan,
    2 => DataVector,
});

impl OutputId {
    pub fn data_type(&self) -> PipelineDataType {
        match self {
            OutputId::RawMScan => PipelineDataType::RawMScan,
            OutputId::MScan => PipelineDataType::MScan,
            OutputId::DataVector => PipelineDataType::DataVector,
        }
    }
}

// MARK: Node

#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    pub input_type: OutputId,
    pub data_type: DataType,
    pub a_scan_length: usize,

    pub progress_rx: Option<watch::Receiver<Option<f32>>>,
}

impl Node {
    pub fn raw_m_scan(path: PathBuf, a_scan_length: Option<usize>) -> Self {
        Self {
            path,
            input_type: OutputId::RawMScan,
            data_type: DataType::U16,
            a_scan_length: a_scan_length.unwrap_or(1024),
            progress_rx: None,
        }
    }

    pub fn m_scan(path: PathBuf, a_scan_length: Option<usize>) -> Self {
        Self {
            path,
            input_type: OutputId::MScan,
            data_type: DataType::U16,
            a_scan_length: a_scan_length.unwrap_or(512),
            progress_rx: None,
        }
    }

    pub fn data_vector(path: PathBuf) -> Self {
        Self {
            path,
            input_type: OutputId::DataVector,
            data_type: DataType::F64,
            a_scan_length: 1024,
            progress_rx: None,
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            input_type: OutputId::RawMScan,
            data_type: DataType::U16,
            a_scan_length: 1024,
            progress_rx: None,
        }
    }
}

impl PipelineNode for Node {
    type InputId = InputIdNone;
    type OutputId = OutputId;

    fn inputs(&self) -> impl Iterator<Item = (InputIdNone, Option<NodeOutput>)> {
        std::iter::empty()
    }

    fn changed(&self, other: &Self) -> bool {
        self.path != other.path
            || self.input_type != other.input_type
            || self.a_scan_length != other.a_scan_length
            || self.data_type != other.data_type
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputId, impl Into<TypeId>)> {
        Some((self.input_type, self.input_type.data_type()))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let raw_scan_out = builder.output(OutputId::RawMScan);
        let m_scan_out = builder.output(OutputId::MScan);
        let data_vector_out = builder.output(OutputId::DataVector);

        let (progress_tx, progress_rx) = watch::channel(None);

        self.progress_rx = Some(progress_rx);

        builder.task(Task {
            raw_scan_out,
            m_scan_out,
            data_vector_out,
            path: self.path.clone(),
            input_type: self.input_type,
            data_type: self.data_type,
            a_scan_length: self.a_scan_length,
            progress_tx,
        });
    }
}

// MARK: NodeTask

struct Task {
    raw_scan_out: TaskOutput<requests::RawMScan>,
    m_scan_out: TaskOutput<requests::MScan>,
    data_vector_out: TaskOutput<requests::VectorData>,

    path: PathBuf,
    input_type: OutputId,
    data_type: DataType,
    a_scan_length: usize,

    progress_tx: watch::Sender<Option<f32>>,
}

impl NodeTask for Task {
    type InputId = InputIdNone;
    type PipelineNode = Node;

    fn connect(&mut self, _input_id: InputIdNone, _input: &mut ConnectionHandle) {}

    fn disconnect(&mut self, _input_id: InputIdNone) {}

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.path = node.path.clone();
        self.input_type = node.input_type;
        self.data_type = node.data_type;
        self.a_scan_length = node.a_scan_length;
    }

    fn invalidate(&mut self, _cause: InvalidationCause) {
        let _ = self.progress_tx.send(None);
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        match self.input_type {
            OutputId::RawMScan => {
                tokio::select! {
                    _req = self.raw_scan_out.receive() => {
                        self.respond_to_raw_m_scan().await?;
                    }
                }
            }
            OutputId::MScan => {
                tokio::select! {
                    _req = self.m_scan_out.receive() => {
                        self.respond_to_m_scan().await?;
                    }
                }
            }
            OutputId::DataVector => {
                tokio::select! {
                    _req = self.data_vector_out.receive() => {
                        self.respond_to_data_vector().await?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl Task {
    async fn respond_to_data_vector(&mut self) -> anyhow::Result<()> {
        let mut file = fs::File::open(&self.path).await?;

        let mut buf = Vec::new();

        file.read_to_end(&mut buf).await?;

        let mut data =
            DataVector::from_data_type(self.data_type, buf.len() / self.data_type.size());

        data.as_mut_u8_slice().copy_from_slice(&buf);

        self.data_vector_out.respond(Arc::new(data));

        Ok(())
    }

    async fn respond_to_raw_m_scan(&mut self) -> anyhow::Result<()> {
        Self::respond_streamed(
            &mut self.progress_tx,
            &self.path,
            self.data_type,
            self.a_scan_length,
            |resp, a_scan_count| {
                self.raw_scan_out.respond(requests::RawMScanResponse {
                    data: resp,
                    a_scan_samples: self.a_scan_length,
                    a_scan_count,
                });
                self.raw_scan_out.receive().now_or_never();
            },
        )
        .await
    }

    async fn respond_to_m_scan(&mut self) -> anyhow::Result<()> {
        Self::respond_streamed(
            &mut self.progress_tx,
            &self.path,
            self.data_type,
            self.a_scan_length,
            |resp, a_scan_count| {
                self.m_scan_out.respond(requests::MScanResponse {
                    data: resp,
                    a_scan_samples: self.a_scan_length,
                    a_scan_count,
                });
                self.m_scan_out.receive().now_or_never();
            },
        )
        .await
    }

    async fn respond_streamed(
        progress_tx: &mut watch::Sender<Option<f32>>,
        path: &Path,
        data_type: DataType,
        a_scan_length: usize,
        respond: impl FnOnce(requests::StreamedResponse<Arc<DataMatrix>>, usize),
    ) -> anyhow::Result<()> {
        const CHUNK_SIZE: usize = 12000;

        let mut file = fs::File::open(path).await?;

        let (output, tx) = requests::StreamedResponse::new(200);

        let _ = progress_tx.send(Some(0.0));

        let file_len = file.metadata().await?.len() as usize;

        respond(output, file_len / a_scan_length / data_type.size());

        let mut bytes_read = 0;

        loop {
            let mut data = DataMatrix::from_data_type(data_type, a_scan_length, CHUNK_SIZE);

            let mut index = 0;

            loop {
                match file.read(&mut data.as_mut_u8_slice()[index..]).await? {
                    0 => break,
                    len => index += len,
                }
            }
            let ncols = index / a_scan_length / data_type.size();

            if ncols < CHUNK_SIZE {
                if index == 0 {
                    break;
                }
                let data = data.resize_horizontally(ncols);
                tx.send(Arc::new(data));
                break;
            }

            bytes_read += index;
            let _ = progress_tx.send(Some(bytes_read as f32 / file_len as f32));

            tx.send(Arc::new(data));
        }

        let _ = progress_tx.send(None);

        Ok(())
    }
}
