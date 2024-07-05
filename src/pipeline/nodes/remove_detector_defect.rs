use std::{
    ops::{Add, Sub},
    sync::Arc,
};

use futures::FutureExt;
use nalgebra::{DMatrix, DMatrixView, Scalar};

use crate::{pipeline::types::DataMatrix, queue_channel::error::RecvError};

use super::prelude::*;

// MARK: Node

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub upper: usize,
    pub lower: usize,

    pub m_scan: NodeInput<()>,
}

impl Node {
    pub fn new() -> Self {
        Self {
            upper: 225,
            lower: 219,
            m_scan: Default::default(),
        }
    }
}

deserialize_node!(Node, "remove_detector_defect");

impl PipelineNode for Node {
    type InputId = InputIdSingle;
    type OutputId = OutputIdSingle;

    fn slug() -> &'static str {
        "remove_detector_defect"
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, self.m_scan.connection()))
    }

    fn changed(&self, other: &Self) -> bool {
        self.upper != other.upper || self.lower != other.lower
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::MScan))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let m_scan_out = builder.output(OutputIdSingle);

        builder.task(Task {
            upper: self.upper,
            lower: self.lower,
            m_scan_out,
            m_scan_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    upper: usize,
    lower: usize,

    m_scan_out: TaskOutput<requests::MScan>,
    m_scan_in: TaskInput<requests::MScan>,
}

impl NodeTask for Task {
    type InputId = InputIdSingle;
    type PipelineNode = Node;

    fn connect(&mut self, _input_id: Self::InputId, input: &mut ConnectionHandle) {
        self.m_scan_in.connect(input);
    }

    fn disconnect(&mut self, _input_id: Self::InputId) {
        self.m_scan_in.disconnect();
    }

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.upper = node.upper;
        self.lower = node.lower;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.m_scan_out.receive().await;

        let Some(m_scan_res) = self.m_scan_in.request(requests::MScan).await else {
            return Ok(());
        };

        if let Some(mut m_scan) = m_scan_res.data.subscribe() {
            let (res, tx) = requests::StreamedResponse::new(100);

            let upper = self.upper;
            let lower = self.lower;

            self.m_scan_out.respond(requests::MScanResponse {
                data: res,
                a_scan_count: m_scan_res.a_scan_count,
                a_scan_samples: m_scan_res.a_scan_samples,
            });
            self.m_scan_out.receive().now_or_never();

            loop {
                let m_scan = match m_scan.recv().await {
                    Ok(m_scan) => m_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                let m_scan: DataMatrix =
                    tokio::task::spawn_blocking(move || match m_scan.as_ref() {
                        DataMatrix::U8(m_scan) => {
                            remove_detector_defect(m_scan.as_view(), upper, lower).into()
                        }
                        DataMatrix::U16(m_scan) => {
                            remove_detector_defect(m_scan.as_view(), upper, lower).into()
                        }
                        DataMatrix::U32(m_scan) => {
                            remove_detector_defect(m_scan.as_view(), upper, lower).into()
                        }
                        DataMatrix::U64(m_scan) => {
                            remove_detector_defect(m_scan.as_view(), upper, lower).into()
                        }
                        DataMatrix::F32(m_scan) => {
                            remove_detector_defect(m_scan.as_view(), upper, lower).into()
                        }
                        DataMatrix::F64(m_scan) => {
                            remove_detector_defect(m_scan.as_view(), upper, lower).into()
                        }
                    })
                    .await?;

                tx.send(Arc::new(m_scan));
            }
        }

        Ok(())
    }
}

// MARK: Algorithm

fn remove_detector_defect<T>(m_scan: DMatrixView<T>, upper: usize, lower: usize) -> DMatrix<T>
where
    T: Scalar + Send + Sync + Copy + Sub<T, Output = T> + Add<T, Output = T> + num_traits::NumCast,
{
    use rayon::prelude::*;

    let (upper, lower) = (upper.max(lower), upper.min(lower));

    let mut result = m_scan.clone_owned();

    let gap = (upper - lower) as f32;

    if gap == 0.0 {
        return result;
    }

    result.par_column_iter_mut().for_each(|mut col| {
        let upper_val = col[upper];
        let lower_val = col[lower];

        let diff = (upper_val - lower_val).to_f32().unwrap();

        for i in lower..upper {
            // Interpolate between upper and lower
            col[i] = lower_val + num_traits::cast(diff * (i - lower) as f32 / gap).unwrap();
        }
    });

    result
}
