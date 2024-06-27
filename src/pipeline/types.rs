use nalgebra::{DMatrix, DMatrixView, DVector, Scalar};
use rayon::prelude::*;
use simba::scalar::SubsetOf;

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

    pub fn as_u8_slice(&self) -> &[u8] {
        match self {
            DataVector::U8(data) => data.as_slice(),
            DataVector::U16(data) => bytemuck::cast_slice(data.as_slice()),
            DataVector::U32(data) => bytemuck::cast_slice(data.as_slice()),
            DataVector::U64(data) => bytemuck::cast_slice(data.as_slice()),
            DataVector::F32(data) => bytemuck::cast_slice(data.as_slice()),
            DataVector::F64(data) => bytemuck::cast_slice(data.as_slice()),
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

    pub fn cast<T>(self) -> DVector<T>
    where
        T: Scalar,
        u8: SubsetOf<T>,
        u16: SubsetOf<T>,
        u32: SubsetOf<T>,
        u64: SubsetOf<T>,
        f32: SubsetOf<T>,
        f64: SubsetOf<T>,
    {
        match self {
            DataVector::U8(data) => data.cast(),
            DataVector::U16(data) => data.cast(),
            DataVector::U32(data) => data.cast(),
            DataVector::U64(data) => data.cast(),
            DataVector::F32(data) => data.cast(),
            DataVector::F64(data) => data.cast(),
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

    pub fn as_u8_slice(&self) -> &[u8] {
        match self {
            DataMatrix::U8(data) => data.as_slice(),
            DataMatrix::U16(data) => bytemuck::cast_slice(data.as_slice()),
            DataMatrix::U32(data) => bytemuck::cast_slice(data.as_slice()),
            DataMatrix::U64(data) => bytemuck::cast_slice(data.as_slice()),
            DataMatrix::F32(data) => bytemuck::cast_slice(data.as_slice()),
            DataMatrix::F64(data) => bytemuck::cast_slice(data.as_slice()),
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

    pub fn cast_par(&self, data_type: DataType) -> DataMatrix {
        match self {
            DataMatrix::U8(matrix) => Self::cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::U16(matrix) => Self::cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::U32(matrix) => Self::cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::U64(matrix) => Self::cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::F32(matrix) => Self::cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::F64(matrix) => Self::cast_from_matrix_par(data_type, matrix.as_view()),
        }
    }

    fn cast_from_matrix_par<T>(data_type: DataType, matrix: DMatrixView<T>) -> DataMatrix
    where
        T: Send + Sync + num_traits::NumCast + num_traits::Zero + nalgebra::Scalar + Copy,
    {
        match data_type {
            DataType::U8 => Self::cast_matrix_par::<_, u8>(matrix).into(),
            DataType::U16 => Self::cast_matrix_par::<_, u16>(matrix).into(),
            DataType::U32 => Self::cast_matrix_par::<_, u32>(matrix).into(),
            DataType::U64 => Self::cast_matrix_par::<_, u64>(matrix).into(),
            DataType::F32 => Self::cast_matrix_par::<_, f32>(matrix).into(),
            DataType::F64 => Self::cast_matrix_par::<_, f64>(matrix).into(),
        }
    }

    fn cast_matrix_par<A, B>(matrix: DMatrixView<A>) -> DMatrix<B>
    where
        A: Send + Sync + num_traits::NumCast + nalgebra::Scalar + Copy,
        B: Send + Sync + num_traits::NumCast + nalgebra::Scalar + num_traits::Zero,
    {
        let mut result = DMatrix::zeros(matrix.nrows(), matrix.ncols());

        result
            .par_column_iter_mut()
            .zip(matrix.par_column_iter())
            .for_each(|(mut r, x)| {
                for (r, x) in r.iter_mut().zip(x.iter()) {
                    *r = num_traits::cast(*x).unwrap();
                }
            });

        result
    }
}

macro_rules! impl_from_data_matrix {
    ($ty:ty, $name:ident) => {
        impl From<$ty> for DataMatrix {
            fn from(data: $ty) -> Self {
                DataMatrix::$name(data)
            }
        }
    };
    ( $(( $ty:ty, $name:ident )),* ) => {
        $( impl_from_data_matrix!($ty, $name); )*
    };
}

impl_from_data_matrix!(
    (DMatrix<u8>, U8),
    (DMatrix<u16>, U16),
    (DMatrix<u32>, U32),
    (DMatrix<u64>, U64),
    (DMatrix<f32>, F32),
    (DMatrix<f64>, F64)
);
