use std::sync::Arc;

use futures::FutureExt;
use nalgebra::{DMatrix, Vector2};
use tokio::sync::watch;

use crate::{
    convolution::convolve_par,
    pipeline::types::{self, DataMatrix},
    queue_channel::error::RecvError,
};

use super::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Gaussian,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GaussSettings {
    pub kernel_size: Vector2<usize>,
    pub sigma: f32,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub filter_type: FilterType,

    pub gauss_settings: GaussSettings,

    pub progress_rx: Option<watch::Receiver<Option<f32>>>,

    pub input: NodeInput<()>,
}

impl Node {
    pub fn gaussian() -> Self {
        Self::new(FilterType::Gaussian)
    }

    pub fn new(filter_type: FilterType) -> Self {
        Self {
            filter_type,
            ..Default::default()
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            filter_type: FilterType::Gaussian,
            gauss_settings: GaussSettings {
                kernel_size: Vector2::new(3, 3),
                sigma: 1.0,
            },
            progress_rx: None,
            input: Default::default(),
        }
    }
}

impl PipelineNode for Node {
    type InputId = InputIdSingle;
    type OutputId = OutputIdSingle;

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, self.input.connection()))
    }

    fn changed(&self, other: &Self) -> bool {
        self.filter_type != other.filter_type
            || match self.filter_type {
                FilterType::Gaussian => self.gauss_settings != other.gauss_settings,
            }
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::MScan))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let m_scan_out = builder.output(OutputIdSingle);

        let (progress_tx, progress_rx) = watch::channel(None);

        self.progress_rx = Some(progress_rx);

        builder.task(Task {
            filter_type: self.filter_type,
            gauss_settings: self.gauss_settings,
            progress_tx: progress_tx,
            m_scan_out,
            m_scan_in: TaskInput::default(),
        });
    }
}

struct Task {
    filter_type: FilterType,

    gauss_settings: GaussSettings,

    progress_tx: watch::Sender<Option<f32>>,

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
        self.filter_type = node.filter_type;
        self.gauss_settings = node.gauss_settings;
    }

    fn invalidate(&mut self, _cause: InvalidationCause) {
        let _ = self.progress_tx.send(None);
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.m_scan_out.receive().await;

        let Some(m_scan_res) = self.m_scan_in.request(requests::MScan).await else {
            return Ok(());
        };

        if let Some(mut m_scan) = m_scan_res.data.subscribe() {
            let _ = self.progress_tx.send(Some(0.0));

            let gauss_settings = self.gauss_settings;
            let filter_type = self.filter_type;

            let (res, tx) = requests::StreamedResponse::new(100);

            self.m_scan_out.respond(requests::MScanResponse {
                data: res,
                a_scan_count: m_scan_res.a_scan_count,
                a_scan_samples: m_scan_res.a_scan_samples,
            });
            self.m_scan_out.receive().now_or_never();

            let kernel = gauss_kernel(gauss_settings.sigma, gauss_settings.kernel_size);

            let mut processed_a_scans = 0;

            loop {
                let m_scan = match m_scan.recv().await {
                    Ok(m_scan) => m_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                let kernel = kernel.clone();

                let m_scan: DataMatrix = tokio::task::spawn_blocking(move || match filter_type {
                    FilterType::Gaussian => {
                        let m_scan = if m_scan.data_type().is_integer() {
                            m_scan.cast_rescale_par(types::DataType::F32)
                        } else {
                            m_scan.as_ref().clone()
                        };

                        match m_scan {
                            DataMatrix::F32(matrix) => convolve_par(&matrix, &kernel).into(),
                            DataMatrix::F64(matrix) => convolve_par(&matrix, &kernel.cast()).into(),
                            _ => unreachable!(),
                        }
                    }
                })
                .await?;

                processed_a_scans += m_scan.ncols();
                let _ = self.progress_tx.send(Some(
                    processed_a_scans as f32 / m_scan_res.a_scan_count as f32,
                ));

                tx.send(Arc::new(m_scan));
            }

            let _ = self.progress_tx.send(None);
        }

        Ok(())
    }
}

fn gauss_kernel(sigma: f32, kernel_size: Vector2<usize>) -> DMatrix<f32> {
    let mut kernel = DMatrix::zeros(kernel_size.x, kernel_size.y);

    let center = kernel_size.cast::<f32>() / 2.0;

    let sigma_sq = sigma * sigma;

    let mut sum = 0.0;

    for i in 0..kernel_size.x {
        for j in 0..kernel_size.y {
            let x = i as f32;
            let y = j as f32;

            let x_diff = x - center.x;
            let y_diff = y - center.y;

            let value = (-0.5 * (x_diff * x_diff + y_diff * y_diff) / sigma_sq).exp();

            sum += value;

            kernel[(i, j)] = value;
        }
    }

    kernel /= sum;

    kernel
}
