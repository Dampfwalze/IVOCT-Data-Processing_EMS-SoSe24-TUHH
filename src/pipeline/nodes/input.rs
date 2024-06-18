use std::path::PathBuf;

use crate::{
    node_graph::{impl_enum_from_into_id_types, OutputId},
    pipeline::PipelineDataType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryInputType {
    RawMScan,
    DataVector,
}

#[derive(Debug)]
pub struct BinaryInputNode {
    pub path: PathBuf,
    pub data_type: BinaryInputType,
}

impl BinaryInputNode {
    pub fn new(path: PathBuf, data_type: BinaryInputType) -> Self {
        Self { path, data_type }
    }
}

impl Default for BinaryInputNode {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            data_type: BinaryInputType::RawMScan,
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
