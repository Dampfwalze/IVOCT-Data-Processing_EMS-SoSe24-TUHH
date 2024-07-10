use std::mem;

use nalgebra::{DMatrix, DMatrixView, DVector, Scalar, Vector2, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use simba::scalar::SubsetOf;

#[derive(Debug, Clone)]
pub struct LumenMesh {
    pub vertices: Vec<LumenVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct LumenVertex {
    pub position: Vector3<f32>,
    pub normal: Vector3<f32>,
}

impl LumenVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BScanDiameter {
    pub b_scan_start: usize,
    pub b_scan_end: usize,

    pub min: f32,
    pub max: f32,
    pub mean: f32,

    pub min_points: [Vector2<f32>; 2],
    pub max_points: [Vector2<f32>; 2],
}

// MARK: DataType

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            DataType::U8 | DataType::U16 | DataType::U32 | DataType::U64
        )
    }
}

// MARK: DataVector

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

// MARK: DataMatrix

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
            DataMatrix::U8(matrix) => cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::U16(matrix) => cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::U32(matrix) => cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::U64(matrix) => cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::F32(matrix) => cast_from_matrix_par(data_type, matrix.as_view()),
            DataMatrix::F64(matrix) => cast_from_matrix_par(data_type, matrix.as_view()),
        }
    }

    pub fn cast_rescale_par(&self, data_type: DataType) -> DataMatrix {
        #[rustfmt::skip]
        macro_rules! impl_cast_rescale_par {
            (@int $ty:ty, $matrix:expr) => {
                match data_type {
                    DataType::U8 => DataMatrix::U8(to_type_matrix_i_to_i_par($matrix.as_view())),
                    DataType::U16 => DataMatrix::U16(to_type_matrix_i_to_i_par($matrix.as_view())),
                    DataType::U32 => DataMatrix::U32(to_type_matrix_i_to_i_par($matrix.as_view())),
                    DataType::U64 => DataMatrix::U64(to_type_matrix_i_to_i_par($matrix.as_view())),
                    DataType::F32 => DataMatrix::F32(to_type_matrix_i_to_f_par($matrix.as_view())),
                    DataType::F64 => DataMatrix::F64(to_type_matrix_i_to_f_par($matrix.as_view())),
                }
            };
            (@float $ty:ty, $matrix:expr) => {
                match data_type {
                    DataType::U8 => DataMatrix::U8(to_type_matrix_f_to_i_par($matrix.as_view())),
                    DataType::U16 => DataMatrix::U16(to_type_matrix_f_to_i_par($matrix.as_view())),
                    DataType::U32 => DataMatrix::U32(to_type_matrix_f_to_i_par($matrix.as_view())),
                    DataType::U64 => DataMatrix::U64(to_type_matrix_f_to_i_par($matrix.as_view())),
                    DataType::F32 => DataMatrix::F32(to_type_matrix_f_to_f_par($matrix.as_view())),
                    DataType::F64 => DataMatrix::F64(to_type_matrix_f_to_f_par($matrix.as_view())),
                }
            };
        }

        match self {
            DataMatrix::U8(matrix) => impl_cast_rescale_par!(@int u8, matrix),
            DataMatrix::U16(matrix) => impl_cast_rescale_par!(@int u16, matrix),
            DataMatrix::U32(matrix) => impl_cast_rescale_par!(@int u32, matrix),
            DataMatrix::U64(matrix) => impl_cast_rescale_par!(@int u64, matrix),
            DataMatrix::F32(matrix) => impl_cast_rescale_par!(@float f32, matrix),
            DataMatrix::F64(matrix) => impl_cast_rescale_par!(@float f64, matrix),
        }
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

// MARK: Helper functions

fn cast_from_matrix_par<T>(data_type: DataType, matrix: DMatrixView<T>) -> DataMatrix
where
    T: Send + Sync + num_traits::NumCast + num_traits::Zero + nalgebra::Scalar + Copy,
{
    match data_type {
        DataType::U8 => cast_matrix_par::<_, u8>(matrix).into(),
        DataType::U16 => cast_matrix_par::<_, u16>(matrix).into(),
        DataType::U32 => cast_matrix_par::<_, u32>(matrix).into(),
        DataType::U64 => cast_matrix_par::<_, u64>(matrix).into(),
        DataType::F32 => cast_matrix_par::<_, f32>(matrix).into(),
        DataType::F64 => cast_matrix_par::<_, f64>(matrix).into(),
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
                *r = num_traits::cast(*x).unwrap_or(B::zero());
            }
        });

    result
}

fn to_type_matrix_f_to_i_par<A, B>(matrix: DMatrixView<A>) -> DMatrix<B>
where
    A: Send + Sync + nalgebra::Scalar + num_traits::Float,
    B: Send + Sync + nalgebra::Scalar + num_traits::PrimInt + num_traits::Zero,
{
    let mut result = DMatrix::zeros(matrix.nrows(), matrix.ncols());

    let mut max: A = num_traits::cast(B::max_value()).unwrap();

    while num_traits::cast::<_, B>(max).is_none() {
        max = max * A::from(0.9999999).unwrap();
    }

    result
        .par_column_iter_mut()
        .zip(matrix.par_column_iter())
        .for_each(|(mut r, x)| {
            for (r, x) in r.iter_mut().zip(x.iter()) {
                *r = num_traits::cast((*x * max).clamp(A::zero(), max)).unwrap_or(B::zero());
            }
        });

    result
}

fn to_type_matrix_i_to_f_par<A, B>(matrix: DMatrixView<A>) -> DMatrix<B>
where
    A: Send + Sync + nalgebra::Scalar + num_traits::PrimInt + num_traits::Zero,
    B: Send + Sync + nalgebra::Scalar + num_traits::Float,
{
    let mut result = DMatrix::zeros(matrix.nrows(), matrix.ncols());

    let max: B = num_traits::cast(A::max_value()).unwrap();

    result
        .par_column_iter_mut()
        .zip(matrix.par_column_iter())
        .for_each(|(mut r, x)| {
            for (r, x) in r.iter_mut().zip(x.iter()) {
                *r = num_traits::cast::<_, B>(*x).unwrap() / max;
            }
        });

    result
}

fn to_type_matrix_f_to_f_par<A, B>(matrix: DMatrixView<A>) -> DMatrix<B>
where
    A: Send + Sync + nalgebra::Scalar + num_traits::Float,
    B: Send + Sync + nalgebra::Scalar + num_traits::Float,
{
    cast_matrix_par(matrix)
}

fn to_type_matrix_i_to_i_par<A, B>(matrix: DMatrixView<A>) -> DMatrix<B>
where
    A: Send + Sync + nalgebra::Scalar + num_traits::PrimInt + num_traits::Zero,
    B: Send + Sync + nalgebra::Scalar + num_traits::PrimInt + num_traits::Zero,
{
    if mem::size_of::<A>() == mem::size_of::<B>() {
        return cast_matrix_par(matrix);
    }

    let mut result = DMatrix::zeros(matrix.nrows(), matrix.ncols());

    if mem::size_of::<A>() > mem::size_of::<B>() {
        let shift = (mem::size_of::<A>() - mem::size_of::<B>()) * 8;

        result
            .par_column_iter_mut()
            .zip(matrix.par_column_iter())
            .for_each(|(mut r, x)| {
                for (r, x) in r.iter_mut().zip(x.iter()) {
                    *r = num_traits::cast(*x >> shift).unwrap();
                }
            });
    } else {
        let shift = (mem::size_of::<B>() - mem::size_of::<A>()) * 8;

        result
            .par_column_iter_mut()
            .zip(matrix.par_column_iter())
            .for_each(|(mut r, x)| {
                for (r, x) in r.iter_mut().zip(x.iter()) {
                    *r = num_traits::cast::<_, B>(*x).unwrap() << shift;
                }
            });
    };

    result
}

// MARK: Tests

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_data_matrix_to_type_matrix_f_to_i_par() {
        let data = DMatrix::from_row_slice(2, 2, &[0.0, 0.5, 1.0, 1.5]);
        let result = to_type_matrix_f_to_i_par::<f32, u8>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 127, 255, 255]);
        assert_eq!(result, expected);

        let result = to_type_matrix_f_to_i_par::<f32, u16>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 32767, 65535, 65535]);
        assert_eq!(result, expected);

        let result = to_type_matrix_f_to_i_par::<f32, u32>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 2147483392, 4294966784, 4294966784]);
        assert_eq!(result, expected);

        let result = to_type_matrix_f_to_i_par::<f32, u64>(data.as_view());
        let expected = [
            0,
            9223370937343148032,
            18446741874686296064,
            18446741874686296064,
        ];
        let expected = DMatrix::from_row_slice(2, 2, &expected);
        assert_eq!(result, expected);

        let data = DMatrix::from_row_slice(2, 2, &[0.0, 0.5, 1.0, 1.5]);
        let result = to_type_matrix_f_to_i_par::<f64, u8>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 127, 255, 255]);
        assert_eq!(result, expected);

        let result = to_type_matrix_f_to_i_par::<f64, u16>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 32767, 65535, 65535]);
        assert_eq!(result, expected);

        let result = to_type_matrix_f_to_i_par::<f64, u32>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 2147483647, 4294967295, 4294967295]);
        assert_eq!(result, expected);

        let result = to_type_matrix_f_to_i_par::<f64, u64>(data.as_view());
        let expected = [
            0,
            9223371114517572608,
            18446742229035145216,
            18446742229035145216,
        ];
        let expected = DMatrix::from_row_slice(2, 2, &expected);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_data_matrix_to_type_matrix_i_to_i_par() {
        let data = DMatrix::from_row_slice(2, 2, &[0, 1, 127, 255]);
        let result = to_type_matrix_i_to_i_par::<u8, u8>(data.as_view());
        assert_eq!(result, data);

        let result = to_type_matrix_i_to_i_par::<u8, u16>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 1 << 8, 127 << 8, 255 << 8]);
        assert_eq!(result, expected);

        let result = to_type_matrix_i_to_i_par::<u8, u32>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 1 << 24, 127 << 24, 255 << 24]);
        assert_eq!(result, expected);

        let result = to_type_matrix_i_to_i_par::<u8, u64>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 1u64 << 56, 127u64 << 56, 255u64 << 56]);
        assert_eq!(result, expected);

        let data = expected;

        let result = to_type_matrix_i_to_i_par::<u64, u8>(data.as_view());
        let expected = DMatrix::from_row_slice(2, 2, &[0, 1, 127, 255]);
        assert_eq!(result, expected);
    }
}
