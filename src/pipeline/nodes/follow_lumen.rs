use std::{
    iter::Sum,
    ops::{DerefMut, Mul, Sub},
    sync::{Arc, Mutex},
};

use futures::FutureExt;
use nalgebra::{DMatrixView, DVector};
use num_traits::Zero;

use crate::{pipeline::types::DataMatrix, queue_channel::error::RecvError};

use super::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub window_extend_up: usize,
    pub window_extend_down: usize,
    pub threshold: f64,
    pub check_artifact: bool,
    pub artifact_threshold: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            window_extend_up: 7,
            window_extend_down: 100,
            threshold: 0.2,
            check_artifact: true,
            artifact_threshold: 0.4,
        }
    }
}

pub enum InputId {
    MScan,
    CatheterSegmentation,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => MScan,
    1 => CatheterSegmentation,
});

// MARK: Node

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Node {
    pub settings: Settings,

    pub m_scan: NodeInput<()>,
    pub catheter_segmentation: NodeInput<()>,
}

deserialize_node!(Node, "follow_lumen");

impl PipelineNode for Node {
    type InputId = InputId;
    type OutputId = OutputIdSingle;

    fn slug() -> &'static str {
        "follow_lumen"
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        [
            (InputId::MScan, self.m_scan.connection()),
            (
                InputId::CatheterSegmentation,
                self.catheter_segmentation.connection(),
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
            catheter_segmentation_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    settings: Settings,

    segmentation_out: TaskOutput<requests::MScanSegmentation>,
    m_scan_in: TaskInput<requests::MScan>,
    catheter_segmentation_in: TaskInput<requests::MScanSegmentation>,
}

impl NodeTask for Task {
    type InputId = InputId;
    type PipelineNode = Node;

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            InputId::MScan => self.m_scan_in.connect(input),
            InputId::CatheterSegmentation => self.catheter_segmentation_in.connect(input),
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::MScan => self.m_scan_in.disconnect(),
            InputId::CatheterSegmentation => self.catheter_segmentation_in.disconnect(),
        };
    }

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.settings = node.settings;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.segmentation_out.receive().await;

        let (Some(m_scan_res), Some(catheter_segmentation_res)) = futures::join!(
            self.m_scan_in.request(requests::MScan),
            self.catheter_segmentation_in
                .request(requests::MScanSegmentation),
        ) else {
            return Ok(());
        };

        let (Some(mut m_scan), Some(mut catheter_segmentation)) = (
            m_scan_res.data.subscribe(),
            catheter_segmentation_res.subscribe(),
        ) else {
            return Ok(());
        };

        let (res, tx) = requests::StreamedResponse::new(100);

        self.segmentation_out.respond(res);
        self.segmentation_out.receive().now_or_never();

        let settings = self.settings;

        let mut start_height = None;

        let mut catheter_seg = Vec::new();

        let mut processed_a_scans = 0;

        let mut chunk_ends = Vec::new();
        let mut chunks_send = 0;

        struct Shared {
            lumen_from_start: Vec<u32>,
            interpolated_til: usize,
        }

        let shared = Arc::new(Mutex::new(Shared {
            lumen_from_start: Vec::new(),
            interpolated_til: 0,
        }));

        loop {
            let m_scan = match m_scan.recv().await {
                Ok(m_scan) => m_scan,
                Err(RecvError::Closed) => break,
                Err(e) => Err(e)?,
            };

            let m_scan_count = m_scan.ncols();
            chunk_ends.push(processed_a_scans + m_scan_count);

            while catheter_seg.len() <= processed_a_scans + m_scan_count {
                let catheter_segmentation = match catheter_segmentation.recv().await {
                    Ok(catheter_segmentation) => catheter_segmentation,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                catheter_seg.extend(catheter_segmentation.iter().copied());
            }

            if start_height.is_none() {
                let h = match m_scan.as_ref() {
                    DataMatrix::U8(m_scan) => {
                        find_start_height(m_scan.as_view(), *catheter_seg.first().unwrap())
                    }
                    DataMatrix::U16(m_scan) => {
                        find_start_height(m_scan.as_view(), *catheter_seg.first().unwrap())
                    }
                    DataMatrix::U32(m_scan) => {
                        find_start_height(m_scan.as_view(), *catheter_seg.first().unwrap())
                    }
                    DataMatrix::U64(m_scan) => {
                        find_start_height(m_scan.as_view(), *catheter_seg.first().unwrap())
                    }
                    DataMatrix::F32(m_scan) => {
                        find_start_height(m_scan.as_view(), *catheter_seg.first().unwrap())
                    }
                    DataMatrix::F64(m_scan) => {
                        find_start_height(m_scan.as_view(), *catheter_seg.first().unwrap())
                    }
                };
                start_height = Some(h);
            }

            let catheter_seg = catheter_seg.clone();

            let end_height = tokio::task::spawn_blocking({
                let shared = shared.clone();

                move || {
                    let (lumen_line, end_height) = match m_scan.as_ref() {
                        DataMatrix::U8(m_scan) => follow_lumen(
                            m_scan.as_view(),
                            catheter_seg,
                            start_height.unwrap(),
                            processed_a_scans,
                            &settings,
                        ),
                        DataMatrix::U16(m_scan) => follow_lumen(
                            m_scan.as_view(),
                            catheter_seg,
                            start_height.unwrap(),
                            processed_a_scans,
                            &settings,
                        ),
                        DataMatrix::U32(m_scan) => follow_lumen(
                            m_scan.as_view(),
                            catheter_seg,
                            start_height.unwrap(),
                            processed_a_scans,
                            &settings,
                        ),
                        DataMatrix::U64(m_scan) => follow_lumen(
                            m_scan.as_view(),
                            catheter_seg,
                            start_height.unwrap(),
                            processed_a_scans,
                            &settings,
                        ),
                        DataMatrix::F32(m_scan) => follow_lumen(
                            m_scan.as_view(),
                            catheter_seg,
                            start_height.unwrap(),
                            processed_a_scans,
                            &settings,
                        ),
                        DataMatrix::F64(m_scan) => follow_lumen(
                            m_scan.as_view(),
                            catheter_seg,
                            start_height.unwrap(),
                            processed_a_scans,
                            &settings,
                        ),
                    };

                    let mut shared = shared.lock().unwrap();
                    let shared = shared.deref_mut();

                    shared.lumen_from_start.extend(lumen_line.iter().copied());

                    interpolate_lumen(&mut shared.lumen_from_start, &mut shared.interpolated_til);

                    end_height
                }
            })
            .await?;

            let mut shared = shared.lock().unwrap();
            let shared = shared.deref_mut();

            while chunk_ends
                .get(chunks_send)
                .map_or(false, |&end| shared.interpolated_til >= end)
            {
                let chunk_start = chunk_ends.get(chunks_send - 1).copied().unwrap_or(0);
                let lumen_line = &shared.lumen_from_start[chunk_start..chunk_ends[chunks_send]];

                tx.send(Arc::new(DVector::from_column_slice(lumen_line)));

                chunks_send += 1;
            }

            start_height = Some(end_height);

            processed_a_scans += m_scan_count;
        }

        let mut shared = shared.lock().unwrap();
        let shared = shared.deref_mut();

        if shared.interpolated_til < processed_a_scans {
            let chunk_start = chunk_ends.get(chunks_send - 1).copied().unwrap_or(0);
            let lumen_line = &shared.lumen_from_start[chunk_start..];

            tx.send(Arc::new(DVector::from_column_slice(lumen_line)));
        }

        Ok(())
    }
}

fn find_start_height<T>(m_scan: DMatrixView<T>, start_height: u32) -> u32
where
    T: nalgebra::Scalar + Clone + Copy + PartialOrd + Zero + Mul<Output = T> + num_traits::NumCast,
{
    let start_height = (start_height as usize + 20).min(m_scan.nrows() - 1);

    let a_scan = m_scan.get((start_height.., 0)).unwrap();

    let min = a_scan
        .iter()
        .copied()
        .reduce(|a, b| if a < b { a } else { b })
        .unwrap_or(T::zero());

    let min = if min < T::zero() { T::zero() } else { min };

    let max = a_scan
        .iter()
        .copied()
        .reduce(|a, b| if a > b { a } else { b })
        .unwrap_or(T::zero());

    let threshold =
        min + num_traits::cast((max.to_f64().unwrap() - min.to_f64().unwrap()) * 0.5).unwrap();

    for (i, &v) in a_scan.iter().enumerate() {
        if v > threshold {
            return (start_height + i) as u32;
        }
    }

    start_height as u32
}

fn follow_lumen<T>(
    m_scan: DMatrixView<T>,
    catheter_seg: Vec<u32>,
    start_height: u32,
    m_scan_offset: usize,
    st: &Settings,
) -> (DVector<u32>, u32)
where
    T: nalgebra::Scalar
        + Clone
        + Copy
        + PartialOrd
        + Zero
        + Sum
        + Sub<Output = T>
        + Mul<Output = T>
        + num_traits::NumCast,
{
    let mut lumen_line = DVector::zeros(m_scan.ncols());

    let mut height = start_height as usize;

    for i in 0..m_scan.ncols() {
        let window_start = height
            .saturating_sub(st.window_extend_up)
            .max(catheter_seg[m_scan_offset + i] as usize);
        let window_end = (height + st.window_extend_down).min(m_scan.nrows() - 1);

        let window = m_scan.get((window_start..=window_end, i)).unwrap();

        let mut max_index = usize::MAX;
        for (i, value) in window.iter().copied().enumerate() {
            let value = value.to_f64().unwrap()
                * hann(i as f64 / ((st.window_extend_up + st.window_extend_down + 1) as f64));
            if value > st.threshold {
                max_index = i;
                break;
            }
        }

        if max_index != usize::MAX {
            height = (window_start + max_index).min(m_scan.nrows() - 1);

            lumen_line[i] = height as u32;
        } else {
            lumen_line[i] = u32::MAX;
        }
    }

    let end_height = height as u32;

    if st.check_artifact {
        // Remove where there is artifact above
        for i in 0..m_scan.ncols() {
            if lumen_line[i] == u32::MAX {
                continue;
            }

            let window_start = (catheter_seg[m_scan_offset + i] + 10).min(lumen_line[i]) as usize;
            let Some(window) = m_scan.get((window_start..lumen_line[i] as usize, i)) else {
                continue;
            };

            if window.len() < 5 {
                continue;
            }

            for &value in window.iter() {
                if value.to_f64().unwrap() > st.artifact_threshold {
                    lumen_line[i] = u32::MAX;
                    break;
                }
            }
        }
    }

    (lumen_line, end_height)
}

fn hann(x: f64) -> f64 {
    0.5 * (1.0 - (2.0 * std::f64::consts::PI * x).cos())
}

fn interpolate_lumen(lumen: &mut Vec<u32>, til: &mut usize) {
    if *til == 0 && lumen[0] == u32::MAX {
        let mut next_known = 1;
        while next_known < lumen.len() && lumen[next_known] == u32::MAX {
            next_known += 1;
        }
        lumen[0] = lumen[next_known];
        *til += 1;
    }

    let mut i = *til;

    while i < lumen.len() {
        if lumen[i] == u32::MAX {
            let mut j = i + 1;
            while j < lumen.len() && lumen[j] == u32::MAX {
                j += 1;
            }

            if j < lumen.len() {
                let start = lumen[i - 1];
                let end = lumen[j];

                let diff = end as i32 - start as i32;
                let len = j - i;

                for k in 0..len {
                    lumen[i + k] = (start as i32 + diff * k as i32 / len as i32) as u32;
                }

                i = j + 1;
            } else {
                *til = i;
                break;
            }
        } else {
            i += 1;
        }
    }

    *til = lumen.len();
}
