use std::{
    io::{stdout, Write},
    ops::Range,
    sync::Arc,
};

use futures::FutureExt;
use nalgebra::{DMatrix, DVector, DVectorView};
use rayon::prelude::*;
use rustfft::{num_complex::Complex32, FftPlanner};

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

    pub raw_scan: NodeInput<()>,
    pub offset: NodeInput<()>,
    pub chirp: NodeInput<()>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            factor: 540.0,
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

    fn create_node_task(&self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let m_scan_out = builder.output(OutputIdSingle);

        builder.task(Task {
            factor: self.factor,
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

    fn invalidate(&mut self) {
        println!("Invalidated ProcessRawMScanNodeTask");
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.m_scan_out.receive().await;

        let raw_scan = self.raw_scan_in.request(requests::RawMScan).await;

        if let Some(mut raw_scan) = raw_scan.map(|r| r.subscribe()).flatten() {
            let offset = self.offset_in.request(requests::VectorData);
            let chirp = self.chirp_in.request(requests::VectorData);

            let (offset, chirp) = tokio::join!(offset, chirp);

            let factor = self.factor as f32;

            let offset = offset.map(|o| (*o).clone().cast::<f32>() * factor);
            let chirp = chirp.map(|c| (*c).clone().cast::<f32>());

            let offset = Arc::new(offset);
            let chirp = Arc::new(chirp);

            let (res, tx) = requests::StreamedResponse::new(5);

            self.m_scan_out.respond(res);
            self.m_scan_out.receive().now_or_never();

            print!("Processing RawMScan ");
            stdout().flush()?;

            loop {
                let raw_scan = match raw_scan.recv().await {
                    Ok(raw_scan) => raw_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                let (offset, chirp) = (offset.clone(), chirp.clone());

                let m_scan = tokio::task::spawn_blocking(move || {
                    let offset = offset.as_ref().as_ref().map(DVector::as_view);
                    let chirp = chirp.as_ref().as_ref().map(DVector::as_view);

                    let DataMatrix::F32(raw_scan) = raw_scan.cast_par(DataType::F32) else {
                        unreachable!()
                    };

                    let m_scan = pre_process_raw_m_scan(raw_scan, offset, chirp, factor);
                    m_scan
                })
                .await?;

                print!(".");
                stdout().flush()?;

                tx.send(Arc::new(DataMatrix::F32(m_scan)));
            }

            println!(" Done");
        }

        Ok(())
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

    raw_scan.par_column_iter_mut().for_each(|mut c| {
        let mut buffer = c
            .iter()
            .map(|x| Complex32 { re: *x, im: 0.0 })
            .collect::<Vec<_>>();
        fft.process(&mut buffer);
        c.copy_from(&DVector::from_iterator(
            a_scan_samples,
            buffer.iter().map(|x| x.norm()),
        ));

        for x in c.iter_mut() {
            *x = 20.0 * x.ln();
        }
    });

    raw_scan
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
