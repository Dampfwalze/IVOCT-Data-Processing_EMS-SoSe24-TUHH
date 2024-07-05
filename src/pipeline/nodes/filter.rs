use std::{
    borrow::Cow,
    fmt::Display,
    iter::Sum,
    ops::{AddAssign, Div, MulAssign},
    sync::Arc,
};

use futures::FutureExt;
use nalgebra::{DMatrix, DMatrixView, Matrix3, Scalar, Vector2};
use num_traits::Float;
use simba::scalar::SupersetOf;
use tokio::sync::watch;

use crate::{
    convolution::convolve_par,
    pipeline::types::{self, DataMatrix},
    queue_channel::error::RecvError,
};

use super::prelude::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    #[default]
    Gaussian,
    Median,
    AlignBrightness,
    Wiener,
    Prewitt,
}

impl FilterType {
    pub const VALUES: [FilterType; 5] = [
        FilterType::Gaussian,
        FilterType::Median,
        FilterType::AlignBrightness,
        FilterType::Wiener,
        FilterType::Prewitt,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GaussSettings {
    pub kernel_size: Vector2<usize>,
    pub sigma: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MedianSettings {
    pub size: Vector2<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WienerSettings {
    pub neighborhood_size: Vector2<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PrewittSettings {
    pub threshold: f32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Node {
    pub filter_type: FilterType,

    #[serde(default)]
    pub gauss_settings: GaussSettings,
    #[serde(default)]
    pub median_settings: MedianSettings,
    #[serde(default)]
    pub wiener_settings: WienerSettings,
    #[serde(default)]
    pub prewitt_settings: PrewittSettings,

    #[serde(skip)]
    pub progress_rx: Option<watch::Receiver<Option<f32>>>,

    pub input: NodeInput<()>,
}

impl Node {
    pub fn gaussian() -> Self {
        Self::new(FilterType::Gaussian)
    }

    pub fn median() -> Self {
        Self::new(FilterType::Median)
    }

    pub fn align_brightness() -> Self {
        Self::new(FilterType::AlignBrightness)
    }

    pub fn wiener() -> Self {
        Self::new(FilterType::Wiener)
    }

    pub fn prewitt() -> Self {
        Self::new(FilterType::Prewitt)
    }

    pub fn new(filter_type: FilterType) -> Self {
        Self {
            filter_type,
            ..Default::default()
        }
    }
}

impl Default for GaussSettings {
    fn default() -> Self {
        Self {
            kernel_size: Vector2::new(3, 3),
            sigma: 1.0,
        }
    }
}

impl Default for MedianSettings {
    fn default() -> Self {
        Self {
            size: Vector2::new(3, 3),
        }
    }
}

impl Default for WienerSettings {
    fn default() -> Self {
        Self {
            neighborhood_size: Vector2::new(3, 3),
        }
    }
}

impl Default for PrewittSettings {
    fn default() -> Self {
        Self { threshold: 0.0 }
    }
}

deserialize_node!(Node, "filter");

impl PipelineNode for Node {
    type InputId = InputIdSingle;
    type OutputId = OutputIdSingle;

    fn slug() -> &'static str {
        "filter"
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        std::iter::once((InputIdSingle, self.input.connection()))
    }

    fn changed(&self, other: &Self) -> bool {
        self.filter_type != other.filter_type
            || match self.filter_type {
                FilterType::Gaussian => self.gauss_settings != other.gauss_settings,
                FilterType::Median => self.median_settings != other.median_settings,
                FilterType::AlignBrightness => false,
                FilterType::Wiener => self.wiener_settings != other.wiener_settings,
                FilterType::Prewitt => self.prewitt_settings != other.prewitt_settings,
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
            median_settings: self.median_settings,
            wiener_settings: self.wiener_settings,
            prewitt_settings: self.prewitt_settings,
            progress_tx: progress_tx,
            m_scan_out,
            m_scan_in: TaskInput::default(),
        });
    }
}

struct Task {
    filter_type: FilterType,

    gauss_settings: GaussSettings,
    median_settings: MedianSettings,
    wiener_settings: WienerSettings,
    prewitt_settings: PrewittSettings,

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
        self.median_settings = node.median_settings;
        self.wiener_settings = node.wiener_settings;
        self.prewitt_settings = node.prewitt_settings;
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
            let median_settings = self.median_settings;
            let wiener_settings = self.wiener_settings;
            let prewitt_settings = self.prewitt_settings;
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
                    FilterType::Median => match m_scan.as_ref() {
                        DataMatrix::U8(matrix) => {
                            compute_median_par(matrix.as_view(), median_settings.size).into()
                        }
                        DataMatrix::U16(matrix) => {
                            compute_median_par(matrix.as_view(), median_settings.size).into()
                        }
                        DataMatrix::U32(matrix) => {
                            compute_median_par(matrix.as_view(), median_settings.size).into()
                        }
                        DataMatrix::U64(matrix) => {
                            compute_median_par(matrix.as_view(), median_settings.size).into()
                        }
                        DataMatrix::F32(matrix) => {
                            compute_median_par(matrix.as_view(), median_settings.size).into()
                        }
                        DataMatrix::F64(matrix) => {
                            compute_median_par(matrix.as_view(), median_settings.size).into()
                        }
                    },
                    FilterType::AlignBrightness => {
                        let m_scan: Cow<DataMatrix> = if m_scan.data_type().is_integer() {
                            Cow::Owned(m_scan.cast_rescale_par(types::DataType::F32))
                        } else {
                            Cow::Borrowed(m_scan.as_ref())
                        };

                        match m_scan.as_ref() {
                            DataMatrix::F32(matrix) => {
                                compute_align_brightness_par(matrix.as_view()).into()
                            }
                            DataMatrix::F64(matrix) => {
                                compute_align_brightness_par(matrix.as_view()).into()
                            }
                            _ => unreachable!(),
                        }
                    }
                    FilterType::Wiener => {
                        let m_scan: Cow<DataMatrix> = if m_scan.data_type().is_integer() {
                            Cow::Owned(m_scan.cast_rescale_par(types::DataType::F32))
                        } else {
                            Cow::Borrowed(m_scan.as_ref())
                        };

                        match m_scan.as_ref() {
                            DataMatrix::F32(matrix) => {
                                compute_wiener_par(matrix.as_view(), &wiener_settings).into()
                            }
                            DataMatrix::F64(matrix) => {
                                compute_wiener_par(matrix.as_view(), &wiener_settings).into()
                            }
                            _ => unreachable!(),
                        }
                    }
                    FilterType::Prewitt => {
                        let m_scan: Cow<DataMatrix> = if m_scan.data_type().is_integer() {
                            Cow::Owned(m_scan.cast_rescale_par(types::DataType::F32))
                        } else {
                            Cow::Borrowed(m_scan.as_ref())
                        };

                        match m_scan.as_ref() {
                            DataMatrix::F32(matrix) => {
                                compute_prewitt_par(matrix.as_view(), &prewitt_settings).into()
                            }
                            DataMatrix::F64(matrix) => {
                                compute_prewitt_par(matrix.as_view(), &prewitt_settings).into()
                            }
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

fn compute_median_par<T>(matrix: DMatrixView<T>, size: Vector2<usize>) -> DMatrix<T>
where
    T: Scalar + Send + Sync + Copy + PartialOrd + 'static,
{
    use rayon::prelude::*;

    assert!(size.x <= matrix.nrows());
    assert!(size.y <= matrix.ncols());

    let mut result = matrix.clone_owned();

    let bucket_center = (size.x * size.y) / 2 + ((size.x * size.y) % 2);
    let bucket_center = bucket_center.min(size.x * size.y - 1);

    result
        .par_column_iter_mut()
        .enumerate()
        .for_each(|(col, mut col_data)| {
            let mut bucket = Vec::with_capacity(size.x * size.y);

            for (row, value) in col_data.iter_mut().enumerate() {
                let start_col = col as isize - (size.x / 2) as isize;
                let start_row = row as isize - (size.y / 2) as isize;

                bucket.clear();

                for k_col in 0..size.x as isize {
                    for k_row in 0..size.y as isize {
                        let m_row = (start_row + k_row).abs() as usize;
                        let m_col = (start_col + k_col).abs() as usize;

                        // Mirror upper
                        let m_row = if m_row >= matrix.nrows() {
                            matrix.nrows() - (m_row - matrix.nrows()) - 2
                        } else {
                            m_row
                        };
                        let m_col = if m_col >= matrix.ncols() {
                            matrix.ncols() - (m_col - matrix.ncols()) - 2
                        } else {
                            m_col
                        };

                        bucket.push(matrix[(m_row, m_col)]);
                    }
                }

                bucket.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

                *value = bucket[bucket_center];
            }
        });

    result
}

fn compute_align_brightness_par<T>(matrix: DMatrixView<T>) -> DMatrix<T>
where
    T: Scalar
        + Float
        + Send
        + Sync
        + Copy
        + Sum
        + Div<Output = T>
        + MulAssign
        + num_traits::NumCast
        + 'static,
{
    use rayon::prelude::*;

    let mut result = matrix.clone_owned();

    let means = matrix
        .par_column_iter()
        .map(|col| col.iter().copied().sum::<T>() / num_traits::cast(col.len()).unwrap())
        .collect::<Vec<_>>();

    let mean = means.iter().copied().sum::<T>() / num_traits::cast(means.len()).unwrap();

    result
        .par_column_iter_mut()
        .zip(means.into_par_iter())
        .for_each(|(mut col, col_mean)| {
            let factor = mean / col_mean;
            col.iter_mut().for_each(|value| {
                *value *= factor;
            });
        });
    result
}

fn compute_wiener_par<T>(matrix: DMatrixView<T>, settings: &WienerSettings) -> DMatrix<T>
where
    T: Scalar
        + Float
        + Send
        + Sync
        + Copy
        + Sum
        + Div<Output = T>
        + AddAssign
        + MulAssign
        + num_traits::NumCast
        + PartialOrd
        + 'static,
{
    use rayon::prelude::*;

    let size = settings.neighborhood_size;

    let mut result = matrix.clone_owned();

    let inverse_size = T::one() / num_traits::cast(size.x * size.y).unwrap();

    let mut temp = DMatrix::<(T, T)>::from_fn(matrix.nrows(), matrix.ncols(), |_, _| {
        (T::zero(), T::zero())
    });

    temp.par_column_iter_mut()
        .enumerate()
        .for_each(|(col, mut col_data)| {
            for (row, value) in col_data.iter_mut().enumerate() {
                let start_col = col as isize - (size.x / 2) as isize;
                let start_row = row as isize - (size.y / 2) as isize;

                let get_row_col = |k_row: isize, k_col: isize| {
                    let m_row = (start_row + k_row).abs() as usize;
                    let m_col = (start_col + k_col).abs() as usize;

                    // Mirror upper
                    let m_row = if m_row >= matrix.nrows() {
                        matrix.nrows() - (m_row - matrix.nrows()) - 2
                    } else {
                        m_row
                    };
                    let m_col = if m_col >= matrix.ncols() {
                        matrix.ncols() - (m_col - matrix.ncols()) - 2
                    } else {
                        m_col
                    };

                    (m_row, m_col)
                };

                let mut sum = T::zero();

                for k_col in 0..size.x as isize {
                    for k_row in 0..size.y as isize {
                        let (m_row, m_col) = get_row_col(k_row, k_col);

                        sum += matrix[(m_row, m_col)];
                    }
                }

                let mean = sum * inverse_size;
                let mean_sq = mean * mean;

                let mut sum = T::zero();

                for k_col in 0..size.x as isize {
                    for k_row in 0..size.y as isize {
                        let (m_row, m_col) = get_row_col(k_row, k_col);

                        let val = matrix[(m_row, m_col)];
                        sum += val * val - mean_sq;
                    }
                }

                let local_variance = sum * inverse_size;

                *value = (mean, local_variance);
            }
        });

    // Use mean of all local variances as noise variance
    let noise_variance = temp
        .par_column_iter()
        .map(|col| col.iter().map(|(_, lv)| *lv).sum::<T>() / num_traits::cast(col.len()).unwrap())
        .sum::<T>()
        / num_traits::cast(temp.ncols()).unwrap();

    result
        .par_column_iter_mut()
        .zip(temp.par_column_iter())
        .for_each(|(mut col, temp_col)| {
            col.iter_mut()
                .zip(temp_col.iter())
                .for_each(|(value, (mean, local_variance))| {
                    let filter = *mean
                        + ((*local_variance - noise_variance) / *local_variance) * (*value - *mean);
                    *value *= filter;
                });
        });

    result
}

fn compute_prewitt_par<T>(matrix: DMatrixView<T>, settings: &PrewittSettings) -> DMatrix<T>
where
    T: Scalar
        + Float
        + Send
        + Sync
        + Copy
        + AddAssign
        + num_traits::NumCast
        + PartialOrd
        + SupersetOf<f32>
        + 'static,
{
    use rayon::prelude::*;

    let threshold = num_traits::cast(settings.threshold).unwrap();

    let kernel_x: Matrix3<T> = Matrix3::new(
        1.0, 0.0, -1.0, //
        1.0, 0.0, -1.0, //
        1.0, 0.0, -1.0, //
    )
    .cast();

    let kernel_y = kernel_x.transpose();

    let mut result_x = convolve_par(&matrix, &kernel_x);
    let result_y = convolve_par(&matrix, &kernel_y);

    result_x
        .par_column_iter_mut()
        .zip(result_y.par_column_iter())
        .for_each(|(mut col_x, col_y)| {
            for (value_x, value_y) in col_x.iter_mut().zip(col_y.iter()) {
                let (x, y) = (*value_x, *value_y);

                let magnitude = (x * x + y * y).sqrt();

                *value_x = if magnitude > threshold {
                    magnitude
                } else {
                    T::zero()
                };
            }
        });

    result_x
}
