// Definition of all request types, send between node tasks.

use std::sync::Arc;

use nalgebra::DVector;

use crate::queue_channel;

use super::{
    execution::Request,
    types::{self, *},
};

//MARK: Structures

#[derive(Debug, Clone, Copy)]
pub struct RawMScan;

#[derive(Debug, Clone, Copy)]
pub struct VectorData;

#[derive(Debug, Clone, Copy)]
pub struct MScan;

#[derive(Debug, Clone, Copy)]
pub struct BScanSegmentation;

#[derive(Debug, Clone, Copy)]
pub struct MScanSegmentation;

#[derive(Debug, Clone, Copy)]
pub struct Diameter;

#[derive(Debug, Clone, Copy)]
pub struct Mesh;

//MARK: Implementations

impl Request for RawMScan {
    type Response = RawMScanResponse;

    fn is_response_valid(&self, response: &Self::Response) -> bool {
        !response.data.is_lagged()
    }
}

impl Request for VectorData {
    type Response = Arc<DataVector>;
}

impl Request for MScan {
    type Response = MScanResponse;

    fn is_response_valid(&self, response: &Self::Response) -> bool {
        !response.data.is_lagged()
    }
}

impl Request for BScanSegmentation {
    // Every element is the index of the first a-scan in the next b-scan. The
    // last element is the index of the first a-scan after the last b-scan. (The
    // number of b-scans is len-1)
    type Response = StreamedResponse<usize>;

    fn is_response_valid(&self, response: &Self::Response) -> bool {
        !response.is_lagged()
    }
}

impl Request for MScanSegmentation {
    type Response = StreamedResponse<Arc<DVector<u32>>>;

    fn is_response_valid(&self, response: &Self::Response) -> bool {
        !response.is_lagged()
    }
}

impl Request for Diameter {
    type Response = StreamedResponse<types::BScanDiameter>;

    fn is_response_valid(&self, response: &Self::Response) -> bool {
        !response.is_lagged()
    }
}

impl Request for Mesh {
    type Response = StreamedResponse<types::LumenMesh>;

    fn is_response_valid(&self, response: &Self::Response) -> bool {
        !response.is_lagged()
    }
}

// MARK: Responses

#[derive(Debug, Clone)]
pub struct RawMScanResponse {
    pub data: StreamedResponse<Arc<DataMatrix>>,
    pub a_scan_samples: usize,
    pub a_scan_count: usize,
}

#[derive(Debug, Clone)]
pub struct MScanResponse {
    pub data: StreamedResponse<Arc<DataMatrix>>,
    pub a_scan_samples: usize,
    pub a_scan_count: usize,
}

// MARK: StreamedResponse

/// A response containing a [queue_channel::Receiver] used to receive the data
/// over time. Type T is the type of one chunk of the data. Call
/// [Self::subscribe] to get access to the receiver.
#[derive(Debug, Clone)]
pub struct StreamedResponse<T: Clone>(queue_channel::Receiver<T>);

impl<T: Clone> StreamedResponse<T> {
    pub fn new(capacity: usize) -> (Self, queue_channel::Sender<T>) {
        let (tx, rx) = queue_channel::channel(capacity);

        (Self(rx), tx)
    }

    pub fn subscribe(&self) -> Option<queue_channel::Receiver<T>> {
        match self {
            StreamedResponse(rx) if !rx.is_lagged() => Some(rx.clone()),
            _ => None,
        }
    }

    pub fn is_lagged(&self) -> bool {
        self.0.is_lagged()
    }
}
