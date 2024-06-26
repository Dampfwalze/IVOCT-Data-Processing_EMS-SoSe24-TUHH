use std::path::PathBuf;

use super::prelude::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OutputId {
    #[default]
    RawMScan,
    DataVector,
}

impl_enum_from_into_id_types!(OutputId, [graph::OutputId], {
    0 => RawMScan,
    1 => DataVector,
});

impl OutputId {
    pub fn data_type(&self) -> PipelineDataType {
        match self {
            OutputId::RawMScan => PipelineDataType::RawMScan,
            OutputId::DataVector => PipelineDataType::DataVector,
        }
    }
}

// MARK: Node

#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    pub data_type: OutputId,
    pub a_scan_length: usize,
}

impl Node {
    pub fn m_scan(path: PathBuf, a_scan_length: Option<usize>) -> Self {
        Self {
            path,
            data_type: OutputId::RawMScan,
            a_scan_length: a_scan_length.unwrap_or(1024),
        }
    }

    pub fn data_vector(path: PathBuf) -> Self {
        Self {
            path,
            data_type: OutputId::DataVector,
            a_scan_length: 1024,
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            data_type: OutputId::RawMScan,
            a_scan_length: 1024,
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
            || self.data_type != other.data_type
            || self.a_scan_length != other.a_scan_length
    }
    fn create_node_task(&self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let raw_scan_out = builder.output(OutputId::RawMScan);
        let data_vector_out = builder.output(OutputId::DataVector);

        builder.task(Task {
            raw_scan_out,
            data_vector_out,
            path: self.path.clone(),
            data_type: self.data_type.clone(),
            a_scan_length: self.a_scan_length.clone(),
        });
    }
}

// MARK: NodeTask

struct Task {
    raw_scan_out: TaskOutput<requests::RawMScan>,
    data_vector_out: TaskOutput<requests::VectorData>,

    path: PathBuf,
    data_type: OutputId,
    a_scan_length: usize,
}

impl NodeTask for Task {
    type InputId = InputIdNone;
    type PipelineNode = Node;

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
            OutputId::RawMScan => {
                tokio::select! {
                    _req = self.raw_scan_out.receive() => {
                        let output = format!("RawMScan: {:?} {:?}", self.path, self.a_scan_length);
                        self.raw_scan_out.respond(output).await;
                    }
                }
            }
            OutputId::DataVector => {
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
