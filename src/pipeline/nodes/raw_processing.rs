use crate::node_graph::{impl_enum_from_into_id_types, InputId, NodeInput};

pub enum ProcessRawMScanInputId {
    RawMScan,
    Offset,
    Chirp,
}

#[derive(Debug, Default, Clone)]
pub struct ProcessRawMScanNode {
    pub raw_scan: NodeInput<()>,
    pub offset: NodeInput<()>,
    pub chirp: NodeInput<()>,
}

impl_enum_from_into_id_types!(ProcessRawMScanInputId, [InputId], {
    0 => RawMScan,
    1 => Offset,
    2 => Chirp,
});
