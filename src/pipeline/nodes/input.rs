use std::path::PathBuf;

use crate::{
    node_graph::{impl_enum_from_into_id_types, OutputId},
    pipeline::PipelineDataType,
};

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
