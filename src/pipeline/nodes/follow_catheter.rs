use std::{
    fmt::{Debug, Display},
    ops::{Mul, Sub},
    sync::Arc,
};

use futures::FutureExt;
use nalgebra::{DMatrixView, DVector};
use num_traits::Zero;

use crate::{pipeline::types::DataMatrix, queue_channel::error::RecvError};

use super::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub start_height: u32,
    pub window_extend: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_height: 100,
            window_extend: 7,
        }
    }
}

pub enum InputId {
    MScan,
    BScanSegmentation,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => MScan,
    1 => BScanSegmentation,
});

// MARK: Node

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(default)]
    pub settings: Settings,

    pub m_scan: NodeInput<()>,
    pub b_scan_segmentation: NodeInput<()>,
}

deserialize_node!(Node, "follow_catheter");

impl PipelineNode for Node {
    type InputId = InputId;
    type OutputId = OutputIdSingle;

    fn slug() -> &'static str {
        "follow_catheter"
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        [
            (InputId::MScan, self.m_scan.connection()),
            (
                InputId::BScanSegmentation,
                self.b_scan_segmentation.connection(),
            ),
        ]
        .into_iter()
    }

    fn changed(&self, other: &Self) -> bool {
        self.settings != other.settings
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::MScanSegmentation))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let segmentation_out = builder.output(OutputIdSingle);

        builder.task(Task {
            settings: self.settings,
            segmentation_out,
            m_scan_in: TaskInput::default(),
            b_scan_segmentation_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    settings: Settings,

    segmentation_out: TaskOutput<requests::MScanSegmentation>,
    m_scan_in: TaskInput<requests::MScan>,
    b_scan_segmentation_in: TaskInput<requests::BScanSegmentation>,
}

impl NodeTask for Task {
    type InputId = InputId;
    type PipelineNode = Node;

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            InputId::MScan => self.m_scan_in.connect(input),
            InputId::BScanSegmentation => self.b_scan_segmentation_in.connect(input),
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::MScan => self.m_scan_in.disconnect(),
            InputId::BScanSegmentation => self.b_scan_segmentation_in.disconnect(),
        };
    }

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.settings = node.settings;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.segmentation_out.receive().await;

        let Some(m_scan_res) = self.m_scan_in.request(requests::MScan).await else {
            return Ok(());
        };

        if let Some(mut m_scan) = m_scan_res.data.subscribe() {
            let (res, tx) = requests::StreamedResponse::new(100);

            self.segmentation_out.respond(res);
            self.segmentation_out.receive().now_or_never();

            let settings = self.settings;

            let mut start_height = None;

            loop {
                let m_scan = match m_scan.recv().await {
                    Ok(m_scan) => m_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                if start_height.is_none() {
                    let h = match m_scan.as_ref() {
                        DataMatrix::U8(m_scan) => {
                            find_start_height(m_scan.as_view(), self.settings.start_height)
                        }
                        DataMatrix::U16(m_scan) => {
                            find_start_height(m_scan.as_view(), self.settings.start_height)
                        }
                        DataMatrix::U32(m_scan) => {
                            find_start_height(m_scan.as_view(), self.settings.start_height)
                        }
                        DataMatrix::U64(m_scan) => {
                            find_start_height(m_scan.as_view(), self.settings.start_height)
                        }
                        DataMatrix::F32(m_scan) => {
                            find_start_height(m_scan.as_view(), self.settings.start_height)
                        }
                        DataMatrix::F64(m_scan) => {
                            find_start_height(m_scan.as_view(), self.settings.start_height)
                        }
                    };
                    start_height = Some(h);
                }

                let catheter_line = tokio::task::spawn_blocking(move || match m_scan.as_ref() {
                    DataMatrix::U8(m_scan) => {
                        follow_catheter(m_scan.as_view(), start_height.unwrap(), &settings)
                    }
                    DataMatrix::U16(m_scan) => {
                        follow_catheter(m_scan.as_view(), start_height.unwrap(), &settings)
                    }
                    DataMatrix::U32(m_scan) => {
                        follow_catheter(m_scan.as_view(), start_height.unwrap(), &settings)
                    }
                    DataMatrix::U64(m_scan) => {
                        follow_catheter(m_scan.as_view(), start_height.unwrap(), &settings)
                    }
                    DataMatrix::F32(m_scan) => {
                        follow_catheter(m_scan.as_view(), start_height.unwrap(), &settings)
                    }
                    DataMatrix::F64(m_scan) => {
                        follow_catheter(m_scan.as_view(), start_height.unwrap(), &settings)
                    }
                })
                .await?;

                start_height = Some(catheter_line[catheter_line.len() - 1]);

                tx.send(Arc::new(catheter_line));
            }
        }

        Ok(())
    }
}

fn find_start_height<T>(m_scan: DMatrixView<T>, start_height: u32) -> u32
where
    T: nalgebra::Scalar
        + Clone
        + Copy
        + PartialOrd
        + Zero
        + Mul<Output = T>
        + num_traits::NumCast
        + Display,
{
    let start_height = (start_height as usize).min(m_scan.nrows() - 1);

    let a_scan = m_scan.get((..start_height, 0)).unwrap();

    let min = a_scan
        .iter()
        .copied()
        .reduce(|a, b| if a < b { a } else { b })
        .unwrap_or(T::zero());

    let max = a_scan
        .iter()
        .copied()
        .reduce(|a, b| if a > b { a } else { b })
        .unwrap_or(T::zero());

    let threshold =
        min + num_traits::cast((max.to_f64().unwrap() - min.to_f64().unwrap()) * 0.2).unwrap();

    for (i, &v) in a_scan.iter().enumerate().rev() {
        if v > threshold {
            return i as u32;
        }
    }

    start_height as u32
}

fn follow_catheter<T>(m_scan: DMatrixView<T>, start_height: u32, st: &Settings) -> DVector<u32>
where
    T: nalgebra::Scalar
        + Clone
        + Copy
        + PartialOrd
        + Zero
        + Sub<Output = T>
        + Mul<Output = T>
        + num_traits::NumCast,
{
    let mut catheter_line = DVector::zeros(m_scan.ncols());

    let mut height = start_height as usize;

    let threshold = num_traits::cast(0.1).unwrap();

    for i in 0..m_scan.ncols() {
        let window_start = height.saturating_sub(st.window_extend);
        let window_end = (height + st.window_extend).min(m_scan.nrows() - 1);

        let window = m_scan.get((window_start..=window_end, i)).unwrap();

        let mut max_index = st.window_extend;
        for (i, value) in window.iter().copied().enumerate().rev() {
            let value =
                value.to_f64().unwrap() * hann(i as f64 / ((st.window_extend * 2 + 1) as f64));
            if value > threshold {
                max_index = i;
                break;
            }
        }

        height = (window_start + max_index).min(m_scan.nrows() - 1);

        catheter_line[i] = height as u32;
    }

    catheter_line
}

fn hann(x: f64) -> f64 {
    0.5 * (1.0 - (2.0 * std::f64::consts::PI * x).cos())
}
