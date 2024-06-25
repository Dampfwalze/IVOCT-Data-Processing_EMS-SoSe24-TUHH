use std::path::PathBuf;

use crate::{
    node_graph::{impl_enum_from_into_id_types, InputIdNone, NodeOutput, OutputId},
    pipeline::{
        execution::{ConnectionHandle, NodeTask, NodeTaskBuilder, Request, TaskOutput},
        PipelineDataType,
    },
};

use super::PipelineNode;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BinaryInputType {
    #[default]
    RawMScan,
    DataVector,
}

#[derive(Debug, Clone)]
pub struct BinaryInputNode {
    pub path: PathBuf,
    pub data_type: BinaryInputType,
    pub a_scan_length: usize,
}

impl BinaryInputNode {
    pub fn m_scan(path: PathBuf, a_scan_length: Option<usize>) -> Self {
        Self {
            path,
            data_type: BinaryInputType::RawMScan,
            a_scan_length: a_scan_length.unwrap_or(1024),
        }
    }

    pub fn data_vector(path: PathBuf) -> Self {
        Self {
            path,
            data_type: BinaryInputType::DataVector,
            a_scan_length: 1024,
        }
    }
}

impl Default for BinaryInputNode {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            data_type: BinaryInputType::RawMScan,
            a_scan_length: 1024,
        }
    }
}

impl_enum_from_into_id_types!(BinaryInputType, [OutputId], {
    0 => RawMScan,
    1 => DataVector,
});

impl BinaryInputType {
    pub fn data_type(&self) -> PipelineDataType {
        match self {
            BinaryInputType::RawMScan => PipelineDataType::RawMScan,
            BinaryInputType::DataVector => PipelineDataType::DataVector,
        }
    }
}

impl PipelineNode for BinaryInputNode {
    type InputId = InputIdNone;
    type OutputId = BinaryInputType;

    fn inputs(&self) -> impl Iterator<Item = (InputIdNone, Option<NodeOutput>)> {
        std::iter::empty()
    }

    fn changed(&self, other: &Self) -> bool {
        self.path != other.path
            || self.data_type != other.data_type
            || self.a_scan_length != other.a_scan_length
    }

    fn create_node_task(&self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let raw_scan_out = builder.output(BinaryInputType::RawMScan);
        let data_vector_out = builder.output(BinaryInputType::DataVector);

        builder.task(BinaryInputNodeTask {
            raw_scan_out,
            data_vector_out,

            a_scan_length: self.a_scan_length,
            data_type: self.data_type,
            path: self.path.clone(),
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RawMScanRequest;

impl Request for RawMScanRequest {
    type Response = String;
}

#[derive(Debug, Clone, Copy)]
pub struct VectorDataRequest;

impl Request for VectorDataRequest {
    type Response = String;
}

struct BinaryInputNodeTask {
    raw_scan_out: TaskOutput<RawMScanRequest>,
    data_vector_out: TaskOutput<VectorDataRequest>,

    path: PathBuf,
    data_type: BinaryInputType,
    a_scan_length: usize,
}

impl NodeTask for BinaryInputNodeTask {
    type InputId = InputIdNone;
    type PipelineNode = BinaryInputNode;

    fn connect(&mut self, _input_id: InputIdNone, _input: &mut ConnectionHandle) {}

    fn disconnect(&mut self, _input_id: InputIdNone) {}

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.path = node.path.clone();
        self.data_type = node.data_type;
        self.a_scan_length = node.a_scan_length;
    }

    fn invalidate(&mut self) {
        println!("Invalidated BinaryInputNodeTask");
    }

    async fn run(&mut self) {
        println!("Running BinaryInputNodeTask");
        match self.data_type {
            BinaryInputType::RawMScan => {
                tokio::select! {
                    _req = self.raw_scan_out.receive() => {
                        let output = format!("RawMScan: {:?} {:?}", self.path, self.a_scan_length);
                        self.raw_scan_out.respond(output).await;
                    }
                }
            }
            BinaryInputType::DataVector => {
                tokio::select! {
                    _req = self.data_vector_out.receive() => {
                        let output = format!("DataVector: {:?}", self.path);
                        self.data_vector_out.respond(output).await;
                    }
                }
            }
        }
    }
}
