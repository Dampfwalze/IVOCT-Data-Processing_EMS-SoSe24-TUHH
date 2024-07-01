use std::sync::Arc;

use crate::queue_channel;

use super::{execution::Request, types::*};

//MARK: Structures

#[derive(Debug, Clone, Copy)]
pub struct RawMScan;

#[derive(Debug, Clone, Copy)]
pub struct VectorData;

#[derive(Debug, Clone, Copy)]
pub struct MScan;

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
