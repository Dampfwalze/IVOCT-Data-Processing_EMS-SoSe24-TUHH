use nalgebra::DMatrix;
use wgpu::{util::DeviceExt, TextureFormat};

macro_rules! import_color_maps {
    ( $( ($category:expr, [ $( $name:expr ),* ]) ),* ) => {
        const COLOR_MAPS: &[&[u8]] = &[
            $(
                $( include_bytes!(concat!($category, "_", $name, ".bmp")), )*
            )*
        ];

        const COLOR_MAP_NAMES: &[(&str, &[&str])] = &[
            $(
                ($category, &[ $( $name ),* ]),
            )*
        ];
    };
}

#[rustfmt::skip]
import_color_maps!(
    ("Perceptually Uniform Sequential", [
        "viridis", "plasma", "inferno", "magma", "cividis"]),
    ("Sequential", [
        "Greys", "Purples", "Blues", "Greens", "Oranges", "Reds",
        "YlOrBr", "YlOrRd", "OrRd", "PuRd", "RdPu", "BuPu",
        "GnBu", "PuBu", "YlGnBu", "PuBuGn", "BuGn", "YlGn"]),
    ("Sequential (2)", [
        "binary", "gist_yarg", "gist_gray", "gray", "bone", "pink",
        "spring", "summer", "autumn", "winter", "cool", "Wistia",
        "hot", "afmhot", "gist_heat", "copper"]),
    ("Diverging", [
        "PiYG", "PRGn", "BrBG", "PuOr", "RdGy", "RdBu",
        "RdYlBu", "RdYlGn", "Spectral", "coolwarm", "bwr", "seismic"]),
    ("Cyclic", ["twilight", "twilight_shifted", "hsv"]),
    ("Qualitative", [
        "Pastel1", "Pastel2", "Paired", "Accent",
        "Dark2", "Set1", "Set2", "Set3",
        "tab10", "tab20", "tab20b", "tab20c"]),
    ("Miscellaneous", [
        "flag", "prism", "ocean", "gist_earth", "terrain", "gist_stern",
        "gnuplot", "gnuplot2", "CMRmap", "cubehelix", "brg",
        "gist_rainbow", "rainbow", "jet", "turbo", "nipy_spectral",
        "gist_ncar"])
);

pub fn get_color_map_names() -> &'static [(&'static str, &'static [&'static str])] {
    COLOR_MAP_NAMES
}

pub fn get_color_maps() -> DMatrix<[u8; 4]> {
    let mut color_maps = DMatrix::from_fn(256, COLOR_MAPS.len(), |_, _| [0, 0, 0, 0]);

    let mut col_idx = 0;
    for bytes in COLOR_MAPS.iter() {
        assert_eq!(bytes[..2], b"BM"[..]);

        assert!(bytes.len() >= 14);

        let offset: u32 = bytemuck::cast([bytes[10], bytes[11], bytes[12], bytes[13]]);

        assert!(bytes.len() as u32 == offset + 256 * 8);

        let bytes = &bytes[offset as usize..offset as usize + 256 * 4];

        for (mat, byte) in color_maps
            .column_mut(col_idx)
            .iter_mut()
            .zip(bytes.chunks_exact(4))
        {
            *mat = [byte[2], byte[1], byte[0], byte[3]];
        }

        col_idx += 1;
    }

    color_maps
}

pub fn upload_color_maps(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let color_maps = get_color_maps();

    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("Color Maps"),
            size: wgpu::Extent3d {
                width: color_maps.nrows() as u32,
                height: color_maps.ncols() as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING,
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        bytemuck::cast_slice(color_maps.as_slice()),
    );

    texture
}
