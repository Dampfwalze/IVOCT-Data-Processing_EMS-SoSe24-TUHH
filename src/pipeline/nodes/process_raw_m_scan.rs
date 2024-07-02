use std::{
    cmp::Ordering,
    ops::Range,
    sync::{Arc, Mutex},
};

use futures::FutureExt;
use nalgebra::{DMatrix, DVector, DVectorView};
use rayon::prelude::*;
use rustfft::{num_complex::Complex32, FftPlanner};
use tokio::sync::watch;

use crate::{
    pipeline::types::{DataMatrix, DataType},
    queue_channel::error::RecvError,
};

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

#[derive(Debug, Clone)]
pub struct Node {
    pub factor: f64,

    pub progress_rx: Option<watch::Receiver<Option<f32>>>,

    pub raw_scan: NodeInput<()>,
    pub offset: NodeInput<()>,
    pub chirp: NodeInput<()>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            factor: 540.0,
            progress_rx: None,
            raw_scan: NodeInput::default(),
            offset: NodeInput::default(),
            chirp: NodeInput::default(),
        }
    }
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

    fn changed(&self, other: &Self) -> bool {
        self.factor != other.factor
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::MScan))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let m_scan_out = builder.output(OutputIdSingle);

        let (progress_tx, progress_rx) = watch::channel(None);

        self.progress_rx = Some(progress_rx);

        builder.task(Task {
            factor: self.factor,
            progress_tx,
            m_scan_out,
            raw_scan_in: TaskInput::default(),
            offset_in: TaskInput::default(),
            chirp_in: TaskInput::default(),
        });
    }
}

// MARK: NodeTask

struct Task {
    factor: f64,

    progress_tx: watch::Sender<Option<f32>>,

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
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::RawMScan => self.raw_scan_in.disconnect(),
            InputId::Offset => self.offset_in.disconnect(),
            InputId::Chirp => self.chirp_in.disconnect(),
        }
    }

    fn invalidate(&mut self, _cause: InvalidationCause) {
        let _ = self.progress_tx.send(None);
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.m_scan_out.receive().await;

        let Some(raw_res) = self.raw_scan_in.request(requests::RawMScan).await else {
            return Ok(());
        };

        if let Some(mut raw_scan) = raw_res.data.subscribe() {
            let _ = self.progress_tx.send(Some(0.0));

            let offset = self.offset_in.request(requests::VectorData);
            let chirp = self.chirp_in.request(requests::VectorData);

            let (offset, chirp) = tokio::join!(offset, chirp);

            let factor = self.factor as f32;

            let (res, tx) = requests::StreamedResponse::new(100);

            self.m_scan_out.respond(requests::MScanResponse {
                data: res,
                a_scan_count: raw_res.a_scan_count,
                a_scan_samples: raw_res.a_scan_samples / 2,
            });
            self.m_scan_out.receive().now_or_never();

            let mut processed_a_scans = 0;

            struct Shared {
                offset: Option<DVector<f32>>,
                chirp: Option<DVector<f32>>,
                lower: Option<f32>,
                upper: Option<f32>,
            }

            let shared = Arc::new(Mutex::new(Shared {
                offset: offset.map(|o| (*o).clone().cast::<f32>() * factor),
                chirp: chirp.map(|c| (*c).clone().cast::<f32>()),
                lower: None,
                upper: None,
            }));

            loop {
                let raw_scan = match raw_scan.recv().await {
                    Ok(raw_scan) => raw_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                let shared = shared.clone();

                let m_scan = tokio::task::spawn_blocking(move || {
                    let mut shared = shared.lock().unwrap();

                    let DataMatrix::F32(raw_scan) = raw_scan.cast_par(DataType::F32) else {
                        unreachable!()
                    };

                    let mut m_scan = pre_process_raw_m_scan(
                        raw_scan,
                        shared.offset.as_ref().map(DVector::as_view),
                        shared.chirp.as_ref().map(DVector::as_view),
                        factor,
                    );

                    if shared.lower.is_none() || shared.upper.is_none() {
                        let (l_lower, l_upper) = find_bounds_par(m_scan.as_slice(), 100);
                        shared.lower = l_lower.last().copied();
                        shared.upper = l_upper.last().copied();
                        println!("Lower: {:?}, Upper: {:?}", shared.lower, shared.upper);
                    }

                    if let (Some(lower), Some(upper)) = (shared.lower, shared.upper) {
                        m_scan.par_column_iter_mut().for_each(|mut c| {
                            for x in c.iter_mut() {
                                *x = (*x - lower) / (upper - lower);
                            }
                        });
                    }

                    m_scan
                })
                .await?;

                processed_a_scans += m_scan.ncols();
                let _ = self
                    .progress_tx
                    .send(Some(processed_a_scans as f32 / raw_res.a_scan_count as f32));

                tx.send(Arc::new(DataMatrix::F32(m_scan)));
            }

            let _ = self.progress_tx.send(None);
        }

        Ok(())
    }
}

// MARK: Rescaling

fn find_bounds_par(data: &[f32], nth: usize) -> (Vec<f32>, Vec<f32>) {
    let upper = Mutex::new(Vec::new());
    let lower = Mutex::new(Vec::new());

    rayon::scope(|s| {
        let threads = rayon::current_num_threads();
        let block_size = (data.len() / threads) + 1;

        for i in 0..threads {
            let upper = &upper;
            let lower = &lower;
            s.spawn(move |_| {
                let (l_lower, l_upper) = find_bounds(
                    &data[(i * block_size).min(data.len())..((i + 1) * block_size).min(data.len())],
                    nth,
                );

                {
                    let mut lower = lower.lock().unwrap();
                    if lower.is_empty() {
                        *lower = l_lower;
                    } else {
                        *lower = merge(&lower, &l_lower, nth, Ordering::Less);
                    }
                }

                {
                    let mut upper = upper.lock().unwrap();
                    if upper.is_empty() {
                        *upper = l_upper;
                    } else {
                        *upper = merge(&upper, &l_upper, nth, Ordering::Greater);
                    }
                }
            });
        }
    });

    (lower.into_inner().unwrap(), upper.into_inner().unwrap())
}

fn merge(a: &[f32], b: &[f32], nth: usize, ordering: Ordering) -> Vec<f32> {
    let mut result = Vec::with_capacity(nth);

    let mut a_iter = a.iter();
    let mut b_iter = b.iter();

    let mut a_val = a_iter.next();
    let mut b_val = b_iter.next();

    for _ in 0..nth {
        match (a_val, b_val) {
            (Some(a), Some(b)) => match a.partial_cmp(b) {
                Some(o) if o == ordering => {
                    result.push(*a);
                    a_val = a_iter.next();
                }
                Some(_) => {
                    result.push(*b);
                    b_val = b_iter.next();
                }
                None => unreachable!("NaN should be filtered out in previous steps"),
            },
            (Some(a), None) => {
                result.push(*a);
                a_val = a_iter.next();
            }
            (None, Some(b)) => {
                result.push(*b);
                b_val = b_iter.next();
            }
            (None, None) => break,
        }
    }

    result
}

fn find_bounds(data: &[f32], nth: usize) -> (Vec<f32>, Vec<f32>) {
    let mut lower = Vec::with_capacity(nth);
    let mut upper = Vec::with_capacity(nth);

    let add_to_vec = |v: f32, vec: &mut Vec<f32>, ordering: Ordering| {
        if !v.is_normal() && v != 0.0 {
            return;
        }

        if vec.len() >= nth && v.partial_cmp(&vec[nth - 1]) == Some(ordering) {
            vec[nth - 1] = v;
        } else if vec.len() < nth {
            vec.push(v);
        } else {
            return;
        }

        if vec.len() == 1 {
            return;
        }

        // fix order
        for i in (0..=vec.len().saturating_sub(2)).rev() {
            if vec[i + 1].partial_cmp(&vec[i]) == Some(ordering) {
                vec.swap(i, i + 1);
            } else {
                break;
            }
        }
    };

    for d in data {
        add_to_vec(*d, &mut lower, Ordering::Less);
        add_to_vec(*d, &mut upper, Ordering::Greater);
    }

    (lower, upper)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_find_bounds_1() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (lower, upper) = find_bounds(&data, 2);

        assert_eq!(lower, vec![1.0, 2.0]);
        assert_eq!(upper, vec![5.0, 4.0]);
    }

    #[test]
    fn test_find_bounds_2() {
        let data = vec![3.0, 2.0, 3.0, 4.0, 3.0];
        let (lower, upper) = find_bounds(&data, 2);

        assert_eq!(lower, vec![2.0, 3.0]);
        assert_eq!(upper, vec![4.0, 3.0]);
    }

    #[test]
    fn test_subnormal() {
        let data = vec![
            1.0,
            2.0,
            f32::NEG_INFINITY,
            3.0,
            4.0,
            f32::NAN,
            5.0,
            f32::INFINITY,
        ];
        let (lower, upper) = find_bounds(&data, 2);

        assert_eq!(lower, vec![1.0, 2.0]);
        assert_eq!(upper, vec![5.0, 4.0]);
    }

    #[test]
    fn test_find_bounds_par_works_with_few_data() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (lower, upper) = find_bounds_par(&data, 2);

        assert_eq!(lower, vec![1.0, 2.0]);
        assert_eq!(upper, vec![5.0, 4.0]);

        let data = vec![1.0];
        let (lower, upper) = find_bounds_par(&data, 2);

        assert_eq!(lower, vec![1.0]);
        assert_eq!(upper, vec![1.0]);

        let data = vec![];
        let (lower, upper) = find_bounds_par(&data, 2);

        assert_eq!(lower, vec![]);
        assert_eq!(upper, vec![]);
    }

    #[test]
    fn test_find_bounds_par_works_with_many_data() {
        let mut data = vec![];
        for _ in 0..1000 {
            data.push(pseudo_rand(*data.last().unwrap_or(&532.0)));
        }

        let (lower, upper) = find_bounds_par(&data, 2);

        let mut sorted = data.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert_eq!(lower, sorted.iter().take(2).copied().collect::<Vec<_>>());
        assert_eq!(
            upper,
            sorted.iter().rev().take(2).copied().collect::<Vec<_>>()
        );
    }

    fn pseudo_rand(last: f32) -> f32 {
        let a = 1664525.0;
        let c = 1013904223.0;
        let m = 2_f32.powi(32);
        (a * last + c) % m
    }
}

// MARK: Algorithm

fn pre_process_raw_m_scan(
    mut raw_scan: DMatrix<f32>,
    offset: Option<DVectorView<f32>>,
    chirp: Option<DVectorView<f32>>,
    factor: f32,
) -> DMatrix<f32> {
    let a_scan_samples = raw_scan.nrows();

    // Multiply by factor (dunno why, but MATLAB version does it too)
    raw_scan.par_column_iter_mut().for_each(|mut x| {
        x *= factor;
    });

    // Remove Detector Offset
    if let Some(offset) = offset {
        raw_scan.par_column_iter_mut().for_each(|mut c| {
            c -= &offset;
        });
    }

    // Remove DC term
    let mean = raw_scan.column_mean();
    raw_scan.par_column_iter_mut().for_each(|mut c| {
        c -= &mean;
    });

    // Apply Chirp
    if let Some(chirp) = chirp {
        raw_scan.par_column_iter_mut().for_each(|mut c| {
            let new_col = linear_sample(chirp.as_slice(), c.as_slice(), 0..a_scan_samples);
            c.copy_from(&new_col);
        });
    }

    // Multiplication with Hann Window
    let hann_window = DVector::<f32>::from_iterator(
        a_scan_samples,
        (0..a_scan_samples).map(|i| hann(i as f32 / a_scan_samples as f32)),
    );
    raw_scan.par_column_iter_mut().for_each(|mut c| {
        c.component_mul_assign(&hann_window);
    });

    // Calculate FFT
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(a_scan_samples);

    let mut fft_out = DMatrix::zeros(a_scan_samples / 2, raw_scan.ncols());

    fft_out
        .par_column_iter_mut()
        .zip(raw_scan.par_column_iter())
        .for_each(|(mut out_c, c)| {
            let mut buffer = c
                .iter()
                .map(|x| Complex32 { re: *x, im: 0.0 })
                .collect::<Vec<_>>();
            fft.process(&mut buffer);
            out_c.copy_from(&DVector::from_iterator(
                a_scan_samples / 2,
                buffer.iter().take(a_scan_samples / 2).map(|x| x.norm()),
            ));

            for x in out_c.iter_mut() {
                *x = 20.0 * x.ln();
            }
        });

    fft_out
}

fn hann(x: f32) -> f32 {
    0.5 * (1.0 - (2.0 * std::f32::consts::PI * x).cos())
}

/// Linearly interpolate the values of `y` at the points `x` for the given `samples`.
///
/// Note: `y` must be monotonically increasing.
fn linear_sample(x: &[f32], y: &[f32], samples: Range<usize>) -> DVector<f32> {
    assert!(x.len() == y.len(), "x and y must have the same length");

    let mut result = DVector::zeros(samples.len());

    let mut upper = 1;

    for (i, sample) in samples.enumerate() {
        while upper < x.len() - 1 && sample as f32 > x[upper] {
            upper += 1;
        }

        let lower = upper - 1;

        // Linear interpolation between (x[lower], y[lower]) and (x[upper], y[upper])
        result[i] =
            y[lower] + (y[upper] - y[lower]) * (sample as f32 - x[lower]) / (x[upper] - x[lower]);
    }

    result
}
