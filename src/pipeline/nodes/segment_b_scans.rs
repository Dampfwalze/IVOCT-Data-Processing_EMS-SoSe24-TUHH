use std::{
    borrow::Cow,
    cmp::Ordering,
    ops::{Add, AddAssign, Div, Sub, SubAssign},
    sync::{Arc, Mutex},
};

use futures::FutureExt;
use nalgebra::{ClosedMulAssign, DMatrix, DMatrixView, Scalar};
use tokio::sync::watch;

use crate::{
    pipeline::types::{DataMatrix, DataType},
    queue_channel::error::RecvError,
};

use super::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub neighbor_count: usize,
    pub neighborhood_width: usize,
    pub search_range_start: usize,
    pub search_range_end: usize,
    pub offset: usize,
}

// MARK: Node

#[derive(Debug, Clone)]
pub struct Node {
    pub settings: Settings,
    pub progress_rx: Option<watch::Receiver<Option<f32>>>,
    pub m_scan: NodeInput<()>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            settings: Settings {
                neighbor_count: 3,
                neighborhood_width: 50,
                search_range_start: 12000,
                search_range_end: 18000,
                offset: 0,
            },
            progress_rx: None,
            m_scan: Default::default(),
        }
    }
}

impl PipelineNode for Node {
    type InputId = InputIdSingle;
    type OutputId = OutputIdSingle;

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, self.m_scan.connection()))
    }

    fn changed(&self, other: &Self) -> bool {
        self.settings != other.settings
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::BScanSegmentation))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let m_scan_out = builder.output(OutputIdSingle);

        let (progress_tx, progress_rx) = watch::channel(None);

        self.progress_rx = Some(progress_rx);

        builder.task(Task {
            settings: self.settings.clone(),
            progress_tx,
            m_scan_out,
            m_scan_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    settings: Settings,

    progress_tx: watch::Sender<Option<f32>>,

    m_scan_out: TaskOutput<requests::BScanSegmentation>,
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
        self.settings = node.settings.clone();
    }

    fn invalidate(&mut self, _cause: InvalidationCause) {
        let _ = self.progress_tx.send(None);
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.m_scan_out.receive().await;

        let Some(m_scan_res) = self.m_scan_in.request(requests::MScan).await else {
            return Ok(());
        };

        if let Some(mut m_scan_rx) = m_scan_res.data.subscribe() {
            let _ = self.progress_tx.send(Some(0.0));

            let (res, tx) = requests::StreamedResponse::new(100);

            self.m_scan_out.respond(res);
            self.m_scan_out.receive().now_or_never();

            struct Shared {
                m_scan: DMatrix<f32>,
                current_end: usize,
                current_start: usize,
            }

            let shared = Arc::new(Mutex::new(Shared {
                m_scan: DMatrix::zeros(m_scan_res.a_scan_samples, m_scan_res.a_scan_count),
                current_end: 0,
                current_start: self.settings.offset,
            }));

            tx.send(self.settings.offset);

            loop {
                let m_scan_chunk = match m_scan_rx.recv().await {
                    Ok(m_scan) => m_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                let settings = self.settings.clone();
                let shared = shared.clone();

                let borders = tokio::task::spawn_blocking(move || {
                    let mut shared = shared.lock().unwrap();
                    let shared = &mut *shared;

                    let m_scan_chunk = if let DataType::F32 = m_scan_chunk.data_type() {
                        Cow::Borrowed(m_scan_chunk.as_ref())
                    } else {
                        Cow::Owned(m_scan_chunk.cast_rescale_par(DataType::F32))
                    };
                    let m_scan_chunk = match m_scan_chunk {
                        Cow::Borrowed(DataMatrix::F32(matrix)) => Cow::Borrowed(matrix),
                        Cow::Owned(DataMatrix::F32(matrix)) => Cow::Owned(matrix),
                        _ => unreachable!(),
                    };

                    shared
                        .m_scan
                        .columns_range_mut(
                            shared.current_end..shared.current_end + m_scan_chunk.ncols(),
                        )
                        .copy_from(&m_scan_chunk);

                    shared.current_end += m_scan_chunk.ncols();

                    let mut borders = Vec::new();

                    while shared.current_start
                        + settings.search_range_end
                        + settings.neighborhood_width
                        <= shared.current_end
                    {
                        // We have enough data for next segmentation

                        let border = segment_b_scan(
                            shared.m_scan.as_view(),
                            &settings,
                            shared.current_start,
                        );

                        shared.current_start = border;

                        borders.push(border);
                    }
                    borders
                })
                .await?;

                if let Some(border) = borders.last() {
                    let _ = self
                        .progress_tx
                        .send(Some(*border as f32 / m_scan_res.a_scan_count as f32));
                }

                for border in borders {
                    tx.send(border);
                }
            }

            let _ = self.progress_tx.send(None);
        }

        Ok(())
    }
}

// MARK: Algorithm

/// Only works with integers, when:
///
/// `T::MAX` > `max_value`² * `a_scan_samples` * `neighbor_count`
///
/// `max_value` being the maximum value in the data.
fn segment_b_scan<T>(m_scan: DMatrixView<T>, settings: &Settings, start: usize) -> usize
where
    T: Scalar
        + Send
        + Sync
        + Copy
        + Sub<Output = T>
        + Add<Output = T>
        + Div<Output = T>
        + SubAssign
        + AddAssign
        + PartialOrd
        + num_traits::NumCast
        + num_traits::Zero
        + ClosedMulAssign,
{
    use rayon::prelude::*;

    let m_scan = m_scan.columns_range(start..);

    let step_size = settings.neighborhood_width / settings.neighbor_count;

    let (index, _min_distance) = (settings.search_range_start
        ..settings
            .search_range_end
            .min(m_scan.ncols() - settings.neighborhood_width))
        .into_par_iter()
        .map(|i| {
            (
                i,
                calculate_distance_sq_of_neighborhood(
                    m_scan,
                    (i..i + settings.neighborhood_width).step_by(step_size),
                    step_size,
                ),
            )
        })
        .min_by(|(_, a), (_, b)| {
            a.partial_cmp(b).unwrap_or_else(|| {
                // Check which is NaN
                if a.partial_cmp(&T::zero()).is_none() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            })
        })
        .unwrap();

    start + index
}

fn calculate_distance_sq_of_neighborhood<T>(
    m_scan: DMatrixView<T>,
    range: impl Iterator<Item = usize>,
    step_size: usize,
) -> T
where
    T: Scalar
        + Send
        + Sync
        + Copy
        + Sub<Output = T>
        + Add<Output = T>
        + Div<Output = T>
        + SubAssign
        + AddAssign
        + num_traits::NumCast
        + num_traits::Zero
        + ClosedMulAssign,
{
    let mut sum = T::zero();
    let mut count = 0;

    for (i, col) in range.enumerate() {
        let diff = m_scan.column(i * step_size) - m_scan.column(col);

        // Square vector and sum together
        sum += diff.dot(&diff);
        count += 1;
    }

    sum / num_traits::cast::<_, T>(count).unwrap()
}
