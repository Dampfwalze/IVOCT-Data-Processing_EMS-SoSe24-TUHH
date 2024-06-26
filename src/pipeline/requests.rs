use super::execution::Request;

//MARK: Structures

#[derive(Debug, Clone, Copy)]
pub struct RawMScan;

#[derive(Debug, Clone, Copy)]
pub struct VectorData;

#[derive(Debug, Clone, Copy)]
pub struct MScan;

//MARK: Implementations

impl Request for RawMScan {
    type Response = String;
}

impl Request for VectorData {
    type Response = String;
}

impl Request for MScan {
    type Response = String;
}
