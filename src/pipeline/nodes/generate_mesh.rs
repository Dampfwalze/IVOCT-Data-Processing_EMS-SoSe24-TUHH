use futures::FutureExt;
use nalgebra::Vector3;

use crate::{
    pipeline::types::{LumenMesh, LumenVertex},
    queue_channel::error::RecvError,
};

use super::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]

pub struct Settings {
    pub rotational_samples: u32,

    pub rotation_frequency: f32,
    pub pullback_speed: f32,
    pub mm_per_pixel: f32,
    pub refraction_index: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rotational_samples: 100,
            rotation_frequency: 180.0,
            pullback_speed: 18.0,
            mm_per_pixel: 0.0055,
            refraction_index: 1.0,
        }
    }
}

pub enum InputId {
    BScans,
    Lumen,
}

impl_enum_from_into_id_types!(InputId, [graph::InputId], {
    0 => BScans,
    1 => Lumen,
});

// MARK: Node

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Node {
    pub settings: Settings,

    pub b_scans: NodeInput<()>,
    pub lumen: NodeInput<()>,
}

deserialize_node!(Node, "generate_mesh");

impl PipelineNode for Node {
    type InputId = InputId;
    type OutputId = OutputIdSingle;

    fn slug() -> &'static str {
        "generate_mesh"
    }

    fn inputs(
        &self,
    ) -> impl Iterator<Item = (<Self as PipelineNode>::InputId, Option<NodeOutput>)> {
        [
            (InputId::BScans, self.b_scans.connection()),
            (InputId::Lumen, self.lumen.connection()),
        ]
        .into_iter()
    }

    fn changed(&self, other: &Self) -> bool {
        self.settings != other.settings
    }

    fn get_output_id_for_view_request(&self) -> Option<(OutputIdSingle, impl Into<TypeId>)> {
        Some((OutputIdSingle, PipelineDataType::Mesh))
    }

    fn create_node_task(&mut self, builder: &mut impl NodeTaskBuilder<PipelineNode = Self>) {
        let mesh_out = builder.output(OutputIdSingle);

        builder.task(Task {
            settings: self.settings,
            mesh_out,
            b_scans_in: TaskInput::default(),
            lumen_in: TaskInput::default(),
        });
    }
}

// MARK: Task

struct Task {
    settings: Settings,

    mesh_out: TaskOutput<requests::Mesh>,
    b_scans_in: TaskInput<requests::BScanSegmentation>,
    lumen_in: TaskInput<requests::MScanSegmentation>,
}

impl NodeTask for Task {
    type InputId = InputId;
    type PipelineNode = Node;

    fn connect(&mut self, input_id: Self::InputId, input: &mut ConnectionHandle) {
        match input_id {
            InputId::BScans => self.b_scans_in.connect(input),
            InputId::Lumen => self.lumen_in.connect(input),
        };
    }

    fn disconnect(&mut self, input_id: Self::InputId) {
        match input_id {
            InputId::BScans => self.b_scans_in.disconnect(),
            InputId::Lumen => self.lumen_in.disconnect(),
        };
    }

    fn sync_node(&mut self, node: &Self::PipelineNode) {
        self.settings = node.settings;
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let _req = self.mesh_out.receive().await;

        let (Some(b_scans_res), Some(lumen_res)) = futures::join!(
            self.b_scans_in.request(requests::BScanSegmentation),
            self.lumen_in.request(requests::MScanSegmentation),
        ) else {
            return Ok(());
        };

        let (Some(mut b_scans), Some(mut lumen)) = (b_scans_res.subscribe(), lumen_res.subscribe())
        else {
            return Ok(());
        };

        let (res, tx) = requests::StreamedResponse::new(100);

        self.mesh_out.respond(res);
        self.mesh_out.receive().now_or_never();

        let settings = self.settings;

        let mut processed_b_scans = 0;

        let mut received_b_scans = Vec::new();
        let mut received_lumen = Vec::<u32>::new();
        let mut resampled_lumen = Vec::new();

        let mut b_scan_closed = false;
        let mut lumen_closed = false;

        while !b_scan_closed || !lumen_closed {
            futures::select! {
                b_scan = b_scans.recv().fuse() => {
                    match b_scan {
                        Ok(b_scan) => {
                            received_b_scans.push(b_scan);
                        },
                        Err(RecvError::Closed) => b_scan_closed = true,
                        Err(e) => Err(e)?,
                    }
                },
                lumen = lumen.recv().fuse() => {
                    match lumen {
                        Ok(lumen) => {
                            received_lumen.extend_from_slice(lumen.as_slice());
                        },
                        Err(RecvError::Closed) => lumen_closed = true,
                        Err(e) => Err(e)?,
                    }
                },
            };

            while received_b_scans
                .get(processed_b_scans)
                .map_or(false, |&b_scan| received_lumen.len() >= b_scan)
            {
                if processed_b_scans > 0 {
                    let resampled = resample_b_scan(
                        received_b_scans[processed_b_scans - 1],
                        received_b_scans[processed_b_scans],
                        &received_lumen,
                        &settings,
                    );

                    resampled_lumen.extend(resampled);
                }

                if processed_b_scans > 1 {
                    let mesh = generate_mesh(
                        (processed_b_scans - 2) * settings.rotational_samples as usize,
                        (processed_b_scans - 1) * settings.rotational_samples as usize,
                        (processed_b_scans - 0) * settings.rotational_samples as usize,
                        processed_b_scans as f32 * settings.pullback_speed
                            / settings.rotation_frequency,
                        &resampled_lumen,
                        &settings,
                    );

                    tx.send(mesh);
                }

                processed_b_scans += 1;
            }
        }

        Ok(())
    }
}

fn resample_b_scan(
    b_scan_start: usize,
    b_scan_end: usize,
    lumen: &[u32],
    st: &Settings,
) -> Vec<f32> {
    let lumen = &lumen[b_scan_start..b_scan_end];

    let mut resampled = Vec::new();

    for i in 0..st.rotational_samples {
        let rot = i as f32 / st.rotational_samples as f32;

        let idx = rot * lumen.len() as f32;

        let lower = lumen[idx.floor() as usize] as isize;
        let upper = lumen[idx.ceil() as usize] as isize;

        let r = lower as f32 + (upper - lower) as f32 * (idx.fract() as f32);

        resampled.push(r);
    }

    resampled
}

fn generate_mesh(
    b_scan_prev: usize,
    b_scan: usize,
    b_scan_next: usize,
    start_z: f32,
    lumen: &[f32],
    st: &Settings,
) -> LumenMesh {
    let left_lumen = &lumen[b_scan_prev..b_scan];
    let right_lumen = &lumen[b_scan..b_scan_next];

    let mut vertices = Vec::<LumenVertex>::with_capacity((st.rotational_samples as usize + 1) * 2);
    let mut indices = Vec::with_capacity((st.rotational_samples as usize + 1) * 6);

    let width = st.pullback_speed / st.rotation_frequency;

    for i in 0..=st.rotational_samples {
        let rot = i as f32 / st.rotational_samples as f32;

        let i = i % st.rotational_samples;

        let rot = rot * std::f32::consts::TAU;

        let left = left_lumen[i as usize] * st.mm_per_pixel / st.refraction_index;
        let right = right_lumen[i as usize] * st.mm_per_pixel / st.refraction_index;

        let current_idx = vertices.len() as u32;

        let dir = Vector3::new(rot.cos(), rot.sin(), 0.0);

        let p_left = dir * left + Vector3::new(0.0, 0.0, start_z);
        let p_right = dir * right + Vector3::new(0.0, 0.0, start_z + width);

        let diff = p_right - p_left;
        let normal = diff.cross(&Vector3::new(-dir.y, dir.x, 0.0)).normalize();

        vertices.push(LumenVertex {
            position: p_left,
            normal,
        });
        vertices.push(LumenVertex {
            position: p_right,
            normal,
        });

        indices.extend([
            // First triangle
            current_idx + 0,
            current_idx + 1,
            current_idx + 2,
            // Second triangle
            current_idx + 2,
            current_idx + 1,
            current_idx + 3,
        ]);
    }
    indices.resize(indices.len() - 6, 0);

    LumenMesh { vertices, indices }
}
