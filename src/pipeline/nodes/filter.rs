use std::{
    borrow::Cow,
    iter::Sum,
    ops::{AddAssign, Div, MulAssign},
    sync::Arc,
};

use futures::FutureExt;
use nalgebra::{DMatrix, DMatrixView, Matrix3, Scalar, Vector2};
use num_traits::{Float, Zero};
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
    WidenStructures,
    BWAreaOpen,
}

impl FilterType {
    pub const VALUES: [FilterType; 7] = [
        FilterType::Gaussian,
        FilterType::Median,
        FilterType::AlignBrightness,
        FilterType::Wiener,
        FilterType::Prewitt,
        FilterType::WidenStructures,
        FilterType::BWAreaOpen,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AreaConnectionType {
    Star4,
    Circle8,
}

impl AreaConnectionType {
    pub const VALUES: [AreaConnectionType; 2] =
        [AreaConnectionType::Star4, AreaConnectionType::Circle8];
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WidenStructuresSettings {
    pub width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BWareOpenSettings {
    pub area: usize,
    pub connection_type: AreaConnectionType,
}

// MARK: Node

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
    #[serde(default)]
    pub widen_structures_settings: WidenStructuresSettings,
    #[serde(default)]
    pub b_w_area_open_settings: BWareOpenSettings,

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

    pub fn widen_structures() -> Self {
        Self::new(FilterType::WidenStructures)
    }

    pub fn b_ware_open() -> Self {
        Self::new(FilterType::BWAreaOpen)
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

impl Default for WidenStructuresSettings {
    fn default() -> Self {
        Self { width: 3 }
    }
}

impl Default for BWareOpenSettings {
    fn default() -> Self {
        Self {
            area: 10,
            connection_type: AreaConnectionType::Star4,
        }
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
                FilterType::WidenStructures => {
                    self.widen_structures_settings != other.widen_structures_settings
                }
                FilterType::BWAreaOpen => {
                    self.b_w_area_open_settings != other.b_w_area_open_settings
                }
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
            widen_structures_settings: self.widen_structures_settings,
            b_ware_open_settings: self.b_w_area_open_settings,
            progress_tx: progress_tx,
            m_scan_out,
            m_scan_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    filter_type: FilterType,

    gauss_settings: GaussSettings,
    median_settings: MedianSettings,
    wiener_settings: WienerSettings,
    prewitt_settings: PrewittSettings,
    widen_structures_settings: WidenStructuresSettings,
    b_ware_open_settings: BWareOpenSettings,

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
        self.widen_structures_settings = node.widen_structures_settings;
        self.b_ware_open_settings = node.b_w_area_open_settings;
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
            let widen_structures_settings = self.widen_structures_settings;
            let b_ware_open_settings = self.b_ware_open_settings;
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
                    FilterType::WidenStructures => match m_scan.as_ref() {
                        DataMatrix::U8(matrix) => {
                            widen_structures_par(matrix.as_view(), widen_structures_settings.width)
                                .into()
                        }
                        DataMatrix::U16(matrix) => {
                            widen_structures_par(matrix.as_view(), widen_structures_settings.width)
                                .into()
                        }
                        DataMatrix::U32(matrix) => {
                            widen_structures_par(matrix.as_view(), widen_structures_settings.width)
                                .into()
                        }
                        DataMatrix::U64(matrix) => {
                            widen_structures_par(matrix.as_view(), widen_structures_settings.width)
                                .into()
                        }
                        DataMatrix::F32(matrix) => {
                            widen_structures_par(matrix.as_view(), widen_structures_settings.width)
                                .into()
                        }
                        DataMatrix::F64(matrix) => {
                            widen_structures_par(matrix.as_view(), widen_structures_settings.width)
                                .into()
                        }
                    },
                    FilterType::BWAreaOpen => match m_scan.as_ref() {
                        DataMatrix::U8(matrix) => {
                            b_warea_open_par(matrix.as_view(), &b_ware_open_settings).into()
                        }
                        DataMatrix::U16(matrix) => {
                            b_warea_open_par(matrix.as_view(), &b_ware_open_settings).into()
                        }
                        DataMatrix::U32(matrix) => {
                            b_warea_open_par(matrix.as_view(), &b_ware_open_settings).into()
                        }
                        DataMatrix::U64(matrix) => {
                            b_warea_open_par(matrix.as_view(), &b_ware_open_settings).into()
                        }
                        DataMatrix::F32(matrix) => {
                            b_warea_open_par(matrix.as_view(), &b_ware_open_settings).into()
                        }
                        DataMatrix::F64(matrix) => {
                            b_warea_open_par(matrix.as_view(), &b_ware_open_settings).into()
                        }
                    },
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

// MARK: Implementations

// MARK: Gaussian

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

// MARK: Median

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

// MARK: Align Brightness

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

// MARK: Wiener

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

// MARK: Prewitt

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

// MARK: Widen Structures

fn widen_structures_par<T>(matrix: DMatrixView<T>, width: usize) -> DMatrix<T>
where
    T: Scalar + Zero + PartialOrd + Send + Sync + Copy + 'static,
{
    use rayon::prelude::*;

    let mut result = matrix.clone_owned();

    let last_col = matrix.ncols() - 1;

    result
        .par_column_iter_mut()
        .enumerate()
        .for_each(|(col, mut col_data)| {
            for (row, value) in col_data.iter_mut().enumerate() {
                let start_col = col.saturating_sub(width);
                let end_col = col.saturating_add(width);

                let end_col = end_col.min(last_col);

                let max = (start_col..=end_col)
                    .map(|c| matrix[(row, c)])
                    .reduce(|a, b| if a > b { a } else { b })
                    .unwrap_or(T::zero());

                *value = max;
            }
        });

    result
}

// MARK: BWareOpen

fn b_warea_open_par<T>(matrix: DMatrixView<T>, settings: &BWareOpenSettings) -> DMatrix<T>
where
    T: Scalar + Zero + PartialOrd + Send + Sync + Copy + 'static,
{
    use rayon::prelude::*;

    const HAVE_SEEN: usize = usize::MAX;

    let mut result = matrix.clone_owned();

    let max_area = settings.area;

    let thread_count = rayon::current_num_threads();
    let block_size = matrix.ncols() / thread_count + 1;

    fn for_neighbors_star4(
        row: usize,
        col: usize,
        max_row: usize,
        max_col: usize,
        f: &mut dyn FnMut(usize, usize),
    ) {
        if row > 0 {
            f(row - 1, col);
        }
        if col > 0 {
            f(row, col - 1);
        }
        if row < max_row {
            f(row + 1, col);
        }
        if col < max_col {
            f(row, col + 1);
        }
    }

    fn for_neighbors_circle8(
        row: usize,
        col: usize,
        max_row: usize,
        max_col: usize,
        f: &mut dyn FnMut(usize, usize),
    ) {
        for i in row.saturating_sub(1)..=(row + 1).min(max_row) {
            for j in col.saturating_sub(1)..=(col + 1).min(max_col) {
                if i == row && j == col {
                    continue;
                }
                f(i, j);
            }
        }
    }

    let for_neighbors = match settings.connection_type {
        AreaConnectionType::Star4 => for_neighbors_star4,
        AreaConnectionType::Circle8 => for_neighbors_circle8,
    };

    let area_counters = (0..thread_count)
        .into_par_iter()
        .map(|i| {
            let start_idx = i * block_size;
            let end_idx = ((i + 1) * block_size).min(matrix.ncols());

            let mut area_counter = DMatrix::<usize>::zeros(matrix.nrows(), end_idx - start_idx);
            let max_row = area_counter.nrows() - 1;
            let max_col = area_counter.ncols() - 1;

            let mut queue = Vec::with_capacity(max_area);

            for col in 0..area_counter.ncols() {
                for row in 0..area_counter.nrows() {
                    if area_counter[(row, col)] > 0 || matrix[(row, col + start_idx)] <= T::zero() {
                        continue;
                    }

                    let mut area_count = 0;
                    let mut queue_cur = 0;
                    queue.clear();
                    queue.push((row, col));

                    while let Some((row, col)) = queue.get(queue_cur).copied() {
                        queue_cur += 1;

                        if matrix[(row, col + start_idx)] <= T::zero() {
                            continue;
                        }

                        let counter = &mut area_counter[(row, col)];

                        if *counter == HAVE_SEEN {
                            continue;
                        }

                        if *counter > 0 {
                            area_count = *counter;
                        }

                        area_count += 1;

                        if area_count >= max_area {
                            break;
                        }

                        *counter = HAVE_SEEN;

                        for_neighbors(row, col, max_row, max_col, &mut |n_row, n_col| {
                            if area_counter[(n_row, n_col)] == 0
                                && matrix[(n_row, n_col + start_idx)] > T::zero()
                            {
                                queue.push((n_row, n_col));
                            }
                        });
                    }

                    for (row, col) in queue.iter().take(queue_cur).copied() {
                        area_counter[(row, col)] = area_count;
                    }
                }
            }

            area_counter
        })
        .collect::<Vec<_>>();

    // TODO: Merge area counters

    result
        .par_column_iter_mut()
        .enumerate()
        .for_each(|(col, mut col_data)| {
            let counter_idx = col / block_size;
            let counter_col = col % block_size;
            for (row, value) in col_data.iter_mut().enumerate() {
                let local_area = area_counters[counter_idx][(row, counter_col)];

                *value = if local_area >= max_area {
                    matrix[(row, col)]
                } else {
                    T::zero()
                };
            }
        });

    result
}
