use super::prelude::*;

pub enum InputId {
    RawMScan,
    Offset,
    Chirp,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => RawMScan,
    1 => Offset,
    2 => Chirp,
});

// MARK: Node

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub raw_scan: NodeInput<()>,
    pub offset: NodeInput<()>,
    pub chirp: NodeInput<()>,
}

impl PipelineNode for Node {
    type InputId = InputId;
    type OutputId = OutputIdSingle;

    fn inputs(&self) -> impl Iterator<Item = (InputId, Option<NodeOutput>)> {
        [
            (InputId::RawMScan, self.raw_scan.connection()),
            (InputId::Offset, self.offset.connection()),
            (InputId::Chirp, self.chirp.connection()),
        ]
        .into_iter()
    }

    fn changed(&self, _other: &Self) -> bool {
        false
    }

    fn create_node_task(&self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let m_scan_out = builder.output(OutputIdSingle);

        builder.task(Task {
            m_scan_out,
            raw_scan_in: TaskInput::default(),
            offset_in: TaskInput::default(),
            chirp_in: TaskInput::default(),
        });
    }
}

// MARK: NodeTask

struct Task {
    m_scan_out: TaskOutput<requests::MScan>,

    raw_scan_in: TaskInput<requests::RawMScan>,
    offset_in: TaskInput<requests::VectorData>,
    chirp_in: TaskInput<requests::VectorData>,
}

impl NodeTask for Task {
    type InputId = InputId;
    type PipelineNode = Node;

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            InputId::RawMScan => self.raw_scan_in.connect(input),
            InputId::Offset => self.offset_in.connect(input),
            InputId::Chirp => self.chirp_in.connect(input),
        }
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::RawMScan => self.raw_scan_in.disconnect(),
            InputId::Offset => self.offset_in.disconnect(),
            InputId::Chirp => self.chirp_in.disconnect(),
        }
    }

    fn invalidate(&mut self) {
        println!("Invalidated ProcessRawMScanNodeTask");
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let raw_scan = self.raw_scan_in.request(requests::RawMScan);
        let offset = self.offset_in.request(requests::VectorData);
        let chirp = self.chirp_in.request(requests::VectorData);

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

        Ok(())
    }
}
