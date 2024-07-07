use futures::FutureExt;

use crate::{pipeline::types::BScanDiameter, queue_channel::error::RecvError};

use super::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub mm_per_pixel: f32,
    #[serde(default)]
    pub refraction_index: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mm_per_pixel: 0.0055,
            refraction_index: 1.33,
        }
    }
}

pub enum InputId {
    BScans,
    Catheter,
    Lumen,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => BScans,
    1 => Catheter,
    2 => Lumen,
});

// MARK: Node

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(default)]
    pub settings: Settings,

    pub b_scans: NodeInput<()>,
    pub catheter: NodeInput<()>,
    pub lumen: NodeInput<()>,
}

deserialize_node!(Node, "diameter");

impl PipelineNode for Node {
    type InputId = InputId;
    type OutputId = OutputIdSingle;

    fn slug() -> &'static str {
        "diameter"
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        [
            (InputId::BScans, self.b_scans.connection()),
            (InputId::Catheter, self.catheter.connection()),
            (InputId::Lumen, self.lumen.connection()),
        ]
        .into_iter()
    }

    fn changed(&self, other: &Self) -> bool {
        self.settings != other.settings
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::Diameter))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let diameter_out = builder.output(OutputIdSingle);

        builder.task(Task {
            settings: self.settings,
            diameter_out,
            b_scans_in: TaskInput::default(),
            catheter_in: TaskInput::default(),
            lumen_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    settings: Settings,

    diameter_out: TaskOutput<requests::Diameter>,
    b_scans_in: TaskInput<requests::BScanSegmentation>,
    catheter_in: TaskInput<requests::MScanSegmentation>,
    lumen_in: TaskInput<requests::MScanSegmentation>,
}

impl NodeTask for Task {
    type InputId = InputId;
    type PipelineNode = Node;

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            InputId::BScans => self.b_scans_in.connect(input),
            InputId::Catheter => self.catheter_in.connect(input),
            InputId::Lumen => self.lumen_in.connect(input),
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::BScans => self.b_scans_in.disconnect(),
            InputId::Catheter => self.catheter_in.disconnect(),
            InputId::Lumen => self.lumen_in.disconnect(),
        }
    }

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.settings = node.settings;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.diameter_out.receive().await;

        let (Some(b_scans_res), Some(catheter_res), Some(lumen_res)) = futures::join!(
            self.b_scans_in.request(requests::BScanSegmentation),
            self.catheter_in.request(requests::MScanSegmentation),
            self.lumen_in.request(requests::MScanSegmentation),
        ) else {
            return Ok(());
        };

        let (Some(mut b_scans), Some(mut catheter), Some(mut lumen)) = (
            b_scans_res.subscribe(),
            catheter_res.subscribe(),
            lumen_res.subscribe(),
        ) else {
            return Ok(());
        };

        let (res, tx) = requests::StreamedResponse::new(100);

        self.diameter_out.respond(res);
        self.diameter_out.receive().now_or_never();

        let settings = self.settings;

        let mut processed_a_scans = 0;
        let mut processed_b_scans = 0;

        let mut received_b_scans = Vec::new();
        let mut received_catheter = Vec::<u32>::new();
        let mut received_lumen = Vec::<u32>::new();

        loop {
            let catheter = match catheter.recv().await {
                Ok(catheter) => catheter,
                Err(RecvError::Closed) => break,
                Err(e) => Err(e)?,
            };

            received_catheter.extend(catheter.iter());

            let lumen = match lumen.recv().await {
                Ok(lumen) => lumen,
                Err(RecvError::Closed) => break,
                Err(e) => Err(e)?,
            };

            received_lumen.extend(lumen.iter());

            while received_b_scans
                .last()
                .map_or(true, |&b_scan| b_scan < processed_a_scans + catheter.len())
            {
                let b_scan = match b_scans.recv().await {
                    Ok(b_scan) => b_scan,
                    Err(RecvError::Closed) => break,
                    Err(e) => Err(e)?,
                };

                received_b_scans.push(b_scan);
            }

            processed_a_scans += catheter.len();

            while received_b_scans
                .get(processed_b_scans)
                .map_or(false, |&scan| scan <= received_lumen.len())
            {
                if processed_b_scans > 1 {
                    let diameter = calculate_diameter(
                        received_b_scans[processed_b_scans - 1],
                        received_b_scans[processed_b_scans],
                        &received_catheter,
                        &received_lumen,
                        &settings,
                    );

                    tx.send(diameter);
                }

                processed_b_scans += 1;
            }
        }

        Ok(())
    }
}

fn calculate_diameter(
    b_scan_start: usize,
    b_scan_end: usize,
    catheter: &[u32],
    lumen: &[u32],
    settings: &Settings,
) -> BScanDiameter {
    let catheter = &catheter[b_scan_start..b_scan_end];
    let lumen = &lumen[b_scan_start..b_scan_end];

    let diameters = (0..catheter.len() / 2)
        .map(|i| calc_diameter(catheter, lumen, i, settings))
        .collect::<Vec<_>>();

    let max_diameter = diameters.iter().cloned().reduce(f32::max).unwrap_or(0.0);
    let min_diameter = diameters.iter().cloned().reduce(f32::min).unwrap_or(0.0);
    let mean_diameter = diameters.iter().sum::<f32>() / diameters.len() as f32;

    BScanDiameter {
        b_scan_start,
        b_scan_end,

        max: max_diameter,
        min: min_diameter,
        mean: mean_diameter,
    }
}

fn calc_diameter(catheter: &[u32], lumen: &[u32], offset: usize, settings: &Settings) -> f32 {
    assert_eq!(catheter.len(), lumen.len());
    let size = lumen.len();
    let a_idx = offset;
    let c_idx = (offset + size / 4) % size;
    let b_idx = (offset + size / 2) % size;
    let d_idx = (offset + size * 3 / 4) % size;

    let a = (lumen[a_idx] - catheter[a_idx]) as f32;
    let b = (lumen[b_idx] - catheter[b_idx]) as f32;
    let c = (lumen[c_idx] - catheter[c_idx]) as f32;
    let d = (lumen[d_idx] - catheter[d_idx]) as f32;

    let mean = (catheter[a_idx] + catheter[b_idx] + catheter[c_idx] + catheter[d_idx]) as f32 / 2.0;

    let mm_per_pixel = settings.mm_per_pixel / settings.refraction_index;

    let a = a * mm_per_pixel;
    let b = b * mm_per_pixel;
    let c = c * mm_per_pixel;
    let d = d * mm_per_pixel;
    let mean = mean * mm_per_pixel;

    let a_b = a - b;
    let c_d = c + d + mean;

    (a_b * a_b + c_d * c_d).sqrt()
}
