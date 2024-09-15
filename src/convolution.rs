use nalgebra::*;
use num_traits::Zero;
use rayon::prelude::*;

/// Convolve a specified kernel over a specified matrix in parallel and return
/// the result as a new owned matrix.
///
/// Matrix is mirrored on the edges.
pub fn convolve_par<T, D1, D2, S1, DK1, DK2, S2>(
    matrix: &Matrix<T, D1, D2, S1>,
    kernel: &Matrix<T, DK1, DK2, S2>,
) -> OMatrix<T, D1, D2>
where
    T: Scalar + Zero + std::ops::Mul<Output = T> + std::ops::AddAssign + Send + Sync + Copy,
    D1: Dim,
    D2: Dim,
    S1: Storage<T, D1, D2> + Sync + Send,
    DK1: Dim,
    DK2: Dim,
    S2: Storage<T, DK1, DK2> + Sync + Send,
    DefaultAllocator: nalgebra::allocator::Allocator<D1, D2> + Send + Sync,
    <nalgebra::DefaultAllocator as nalgebra::allocator::Allocator<D1, D2>>::Buffer<T>: Send + Sync,
{
    assert!(kernel.nrows() <= matrix.nrows());
    assert!(kernel.ncols() <= matrix.ncols());

    let mut result = matrix.clone_owned();

    result
        .par_column_iter_mut()
        .enumerate()
        .for_each(|(col, mut col_data)| {
            for (row, value) in col_data.iter_mut().enumerate() {
                let start_row = row as isize - (kernel.nrows() / 2) as isize;
                let start_col = col as isize - (kernel.ncols() / 2) as isize;

                let mut sum = T::zero();

                for k_col in 0..kernel.ncols() as isize {
                    for k_row in 0..kernel.nrows() as isize {
                        let m_row = (start_row + k_row).abs() as usize;
                        let m_col = (start_col + k_col).abs() as usize;

                        // Mirror upper
                        let m_row = if m_row >= matrix.nrows() {
                            matrix.nrows() - (m_row - matrix.nrows()) - 2
                        } else {
                            m_row
                        };
                        let m_col = if m_col >= matrix.ncols() {
                            matrix.ncols() - (m_col - matrix.ncols()) - 2
                        } else {
                            m_col
                        };

                        sum += matrix[(m_row, m_col)] * kernel[(k_row as usize, k_col as usize)];
                    }
                }
                *value = sum;
            }
        });

    result
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_convolve_kernel_only_one() {
        let matrix = Matrix3::new(
            1.0, 2.0, 3.0, //
            4.0, 5.0, 6.0, //
            7.0, 8.0, 9.0, //
        );

        let kernel1 = Matrix1::new(
            1.0, //
        );
        let kernel2 = Matrix3::new(
            0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, //
        );

        let result1 = convolve_par(&matrix, &kernel1);
        let result2 = convolve_par(&matrix, &kernel2);

        assert_eq!(result1, matrix);
        assert_eq!(result2, matrix);
    }

    #[test]
    fn test_convolve_2_kernel_only_zero() {
        let matrix = Matrix3::new(
            1.0, 2.0, 3.0, //
            4.0, 5.0, 6.0, //
            7.0, 8.0, 9.0, //
        );

        let kernel1 = Matrix1::new(
            0.0, //
        );
        let kernel2 = Matrix2::new(
            0.0, 0.0, //
            0.0, 0.0, //
        );

        let result1 = convolve_par(&matrix, &kernel1);
        let result2 = convolve_par(&matrix, &kernel2);

        assert_eq!(result1, matrix.map(|_| 0.0));
        assert_eq!(result2, matrix.map(|_| 0.0));
    }

    #[test]
    fn test_convolve_shift_kernel() {
        let matrix = Matrix3::new(
            1.0, 2.0, 3.0, //
            4.0, 5.0, 6.0, //
            7.0, 8.0, 9.0, //
        );

        let kernel = Matrix1x3::new(
            1.0, 0.0, 0.0, //
        );

        let result = convolve_par(&matrix, &kernel);

        let expected = Matrix3::new(
            2.0, 1.0, 2.0, //
            5.0, 4.0, 5.0, //
            8.0, 7.0, 8.0, //
        ); // First row mirrored

        assert_eq!(result, expected);

        let kernel = Matrix1x3::new(
            0.0, 0.0, 1.0, //
        );

        let result = convolve_par(&matrix, &kernel);

        let expected = Matrix3::new(
            2.0, 3.0, 2.0, //
            5.0, 6.0, 5.0, //
            8.0, 9.0, 8.0, //
        ); // Last row mirrored

        assert_eq!(result, expected);
    }

    #[test]
    fn test_convolve_sum_kernel() {
        let matrix = Matrix3::new(
            1.0, 2.0, 3.0, //
            4.0, 5.0, 6.0, //
            7.0, 8.0, 9.0, //
        );

        let kernel = Matrix1x3::new(
            1.0, 1.0, 1.0, //
        );

        let result = convolve_par(&matrix, &kernel);

        let expected = Matrix3::new(
            5.0, 6.0, 7.0, //
            14.0, 15.0, 16.0, //
            23.0, 24.0, 25.0, //
        );

        assert_eq!(result, expected);
    }
}
