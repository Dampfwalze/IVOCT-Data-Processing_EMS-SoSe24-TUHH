use nalgebra::{DMatrix, DVector, Dyn, VecStorage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl DataType {
    pub const VALUES: [DataType; 6] = [
        DataType::U8,
        DataType::U16,
        DataType::U32,
        DataType::U64,
        DataType::F32,
        DataType::F64,
    ];

    pub fn size(&self) -> usize {
        match self {
            DataType::U8 => std::mem::size_of::<u8>(),
            DataType::U16 => std::mem::size_of::<u16>(),
            DataType::U32 => std::mem::size_of::<u32>(),
            DataType::U64 => std::mem::size_of::<u64>(),
            DataType::F32 => std::mem::size_of::<f32>(),
            DataType::F64 => std::mem::size_of::<f64>(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DataVector {
    U8(DVector<u8>),
    U16(DVector<u16>),
    U32(DVector<u32>),
    U64(DVector<u64>),
    F32(DVector<f32>),
    F64(DVector<f64>),
}

impl DataVector {
    pub fn from_data_type(data_type: DataType, len: usize) -> Self {
        match data_type {
            DataType::U8 => DataVector::U8(DVector::zeros(len)),
            DataType::U16 => DataVector::U16(DVector::zeros(len)),
            DataType::U32 => DataVector::U32(DVector::zeros(len)),
            DataType::U64 => DataVector::U64(DVector::zeros(len)),
            DataType::F32 => DataVector::F32(DVector::zeros(len)),
            DataType::F64 => DataVector::F64(DVector::zeros(len)),
        }
    }

    pub fn as_mut_u8_slice(&mut self) -> &mut [u8] {
        match self {
            DataVector::U8(data) => data.as_mut_slice(),
            DataVector::U16(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataVector::U32(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataVector::U64(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataVector::F32(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataVector::F64(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
        }
    }

    pub fn data_type(&self) -> DataType {
        match self {
            DataVector::U8(_) => DataType::U8,
            DataVector::U16(_) => DataType::U16,
            DataVector::U32(_) => DataType::U32,
            DataVector::U64(_) => DataType::U64,
            DataVector::F32(_) => DataType::F32,
            DataVector::F64(_) => DataType::F64,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            DataVector::U8(data) => data.len(),
            DataVector::U16(data) => data.len(),
            DataVector::U32(data) => data.len(),
            DataVector::U64(data) => data.len(),
            DataVector::F32(data) => data.len(),
            DataVector::F64(data) => data.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DataMatrix {
    U8(DMatrix<u8>),
    U16(DMatrix<u16>),
    U32(DMatrix<u32>),
    U64(DMatrix<u64>),
    F32(DMatrix<f32>),
    F64(DMatrix<f64>),
}

impl DataMatrix {
    pub fn from_data_type(data_type: DataType, rows: usize, cols: usize) -> Self {
        match data_type {
            DataType::U8 => DataMatrix::U8(DMatrix::zeros(rows, cols)),
            DataType::U16 => DataMatrix::U16(DMatrix::zeros(rows, cols)),
            DataType::U32 => DataMatrix::U32(DMatrix::zeros(rows, cols)),
            DataType::U64 => DataMatrix::U64(DMatrix::zeros(rows, cols)),
            DataType::F32 => DataMatrix::F32(DMatrix::zeros(rows, cols)),
            DataType::F64 => DataMatrix::F64(DMatrix::zeros(rows, cols)),
        }
    }

    pub fn as_mut_u8_slice(&mut self) -> &mut [u8] {
        match self {
            DataMatrix::U8(data) => data.as_mut_slice(),
            DataMatrix::U16(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataMatrix::U32(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataMatrix::U64(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataMatrix::F32(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
            DataMatrix::F64(data) => bytemuck::cast_slice_mut(data.as_mut_slice()),
        }
    }

    pub fn data_type(&self) -> DataType {
        match self {
            DataMatrix::U8(_) => DataType::U8,
            DataMatrix::U16(_) => DataType::U16,
            DataMatrix::U32(_) => DataType::U32,
            DataMatrix::U64(_) => DataType::U64,
            DataMatrix::F32(_) => DataType::F32,
            DataMatrix::F64(_) => DataType::F64,
        }
    }

    pub fn ncols(&self) -> usize {
        match self {
            DataMatrix::U8(data) => data.ncols(),
            DataMatrix::U16(data) => data.ncols(),
            DataMatrix::U32(data) => data.ncols(),
            DataMatrix::U64(data) => data.ncols(),
            DataMatrix::F32(data) => data.ncols(),
            DataMatrix::F64(data) => data.ncols(),
        }
    }

    pub fn resize_horizontally(self, rows: usize) -> Self {
        match self {
            DataMatrix::U8(data) => DataMatrix::U8(data.resize_horizontally(rows, 0)),
            DataMatrix::U16(data) => DataMatrix::U16(data.resize_horizontally(rows, 0)),
            DataMatrix::U32(data) => DataMatrix::U32(data.resize_horizontally(rows, 0)),
            DataMatrix::U64(data) => DataMatrix::U64(data.resize_horizontally(rows, 0)),
            DataMatrix::F32(data) => DataMatrix::F32(data.resize_horizontally(rows, 0.0)),
            DataMatrix::F64(data) => DataMatrix::F64(data.resize_horizontally(rows, 0.0)),
        }
    }
}
