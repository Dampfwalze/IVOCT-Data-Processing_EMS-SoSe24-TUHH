use crate::{
    node_graph::{impl_enum_from_into_id_types, InputId, NodeInput, NodeOutput, OutputIdSingle},
    pipeline::execution::{ConnectionHandle, NodeTask, Request, TaskInput, TaskOutput},
};

use super::{PipelineNode, RawMScanRequest, VectorDataRequest};

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

impl PipelineNode for ProcessRawMScanNode {
    type InputId = ProcessRawMScanInputId;
    type OutputId = OutputIdSingle;

    fn inputs(&self) -> impl Iterator<Item = (ProcessRawMScanInputId, Option<NodeOutput>)> {
        [
            (ProcessRawMScanInputId::RawMScan, self.raw_scan.connection()),
            (ProcessRawMScanInputId::Offset, self.offset.connection()),
            (ProcessRawMScanInputId::Chirp, self.chirp.connection()),
        ]
        .into_iter()
    }

    fn changed(&self, _other: &Self) -> bool {
        false
    }

    fn create_node_task(
        &self,
        builder: &mut impl crate::pipeline::execution::NodeTaskBuilder<PipelineNode = Self>,
    ) {
        let m_scan_out = builder.output(OutputIdSingle);

        builder.task(ProcessRawMScanNodeTask {
            m_scan_out,
            raw_scan_in: TaskInput::default(),
            offset_in: TaskInput::default(),
            chirp_in: TaskInput::default(),
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MScanRequest;

impl Request for MScanRequest {
    type Response = String;
}

struct ProcessRawMScanNodeTask {
    m_scan_out: TaskOutput<MScanRequest>,

    raw_scan_in: TaskInput<RawMScanRequest>,
    offset_in: TaskInput<VectorDataRequest>,
    chirp_in: TaskInput<VectorDataRequest>,
}

impl NodeTask for ProcessRawMScanNodeTask {
    type InputId = ProcessRawMScanInputId;
    type PipelineNode = ProcessRawMScanNode;

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            ProcessRawMScanInputId::RawMScan => self.raw_scan_in.connect(input),
            ProcessRawMScanInputId::Offset => self.offset_in.connect(input),
            ProcessRawMScanInputId::Chirp => self.chirp_in.connect(input),
        }
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            ProcessRawMScanInputId::RawMScan => self.raw_scan_in.disconnect(),
            ProcessRawMScanInputId::Offset => self.offset_in.disconnect(),
            ProcessRawMScanInputId::Chirp => self.chirp_in.disconnect(),
        }
    }

    fn invalidate(&mut self) {
        println!("Invalidated ProcessRawMScanNodeTask");
    }

    async fn run(&mut self) {
        let raw_scan = self.raw_scan_in.request(RawMScanRequest);
        let offset = self.offset_in.request(VectorDataRequest);
        let chirp = self.chirp_in.request(VectorDataRequest);

        let (raw_scan, offset, chirp) = tokio::join!(raw_scan, offset, chirp);

        if let Some(raw_scan) = raw_scan {
            let result = format!(
                "Processed RawMScan: {{ {} }}, Offset: {{ {:?} }}, Chirp: {{ {:?} }}",
                raw_scan, offset, chirp
            );

            // self.m_scan_out.respond(result).await;
            println!("{result}");
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
