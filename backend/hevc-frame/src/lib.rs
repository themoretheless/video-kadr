//! CPU upload representation for SDR HEVC: BT.709 matrix, limited-range NV12.
//! Input is opaque display RGB, after the renderer composites over black.
#![forbid(unsafe_code)]

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Geometry,
    Length,
    NonOpaque,
    Allocation,
    Stride,
}

pub struct Nv12 {
    width: usize,
    height: usize,
    y: Vec<u8>,
    uv: Vec<u8>,
}

fn buffer(len: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| Error::Allocation)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

impl Nv12 {
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn y(&self) -> &[u8] {
        &self.y
    }
    pub fn uv(&self) -> &[u8] {
        &self.uv
    }
    /// Uses 2x2 box chroma filtering before quantization; no implicit alpha loss.
    /// Only even nonzero dimensions are accepted by this upload boundary.
    pub fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Result<Self, Error> {
        if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
            return Err(Error::Geometry);
        }
        let pixels = width.checked_mul(height).ok_or(Error::Geometry)?;
        let bytes = pixels.checked_mul(4).ok_or(Error::Geometry)?;
        if rgba.len() != bytes {
            return Err(Error::Length);
        }
        if rgba.chunks_exact(4).any(|p| p[3] != 255) {
            return Err(Error::NonOpaque);
        }
        let mut y = buffer(pixels)?;
        let mut uv = buffer(pixels / 2)?;
        for row in (0..height).step_by(2) {
            for col in (0..width).step_by(2) {
                let (mut cb, mut cr) = (0.0, 0.0);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let pixel = (row + dy) * width + col + dx;
                        let at = pixel * 4;
                        let r = f64::from(rgba[at]) / 255.0;
                        let g = f64::from(rgba[at + 1]) / 255.0;
                        let b = f64::from(rgba[at + 2]) / 255.0;
                        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                        y[pixel] = (16.0 + 219.0 * luma).round().clamp(16.0, 235.0) as u8;
                        cb += (b - luma) / 1.8556;
                        cr += (r - luma) / 1.5748;
                    }
                }
                let at = row / 2 * width + col;
                uv[at] = (128.0 + 56.0 * cb).round().clamp(16.0, 240.0) as u8;
                uv[at + 1] = (128.0 + 56.0 * cr).round().clamp(16.0, 240.0) as u8;
            }
        }
        Ok(Self {
            width,
            height,
            y,
            uv,
        })
    }

    /// Copy into separately mapped CoreVideo planes. Validate both destinations
    /// before writing either, and preserve allocator-owned row padding.
    pub fn copy_to(
        &self,
        y: &mut [u8],
        y_stride: usize,
        uv: &mut [u8],
        uv_stride: usize,
    ) -> Result<(), Error> {
        fn required(width: usize, rows: usize, stride: usize) -> Result<usize, Error> {
            if stride < width {
                return Err(Error::Stride);
            }
            (rows - 1)
                .checked_mul(stride)
                .and_then(|n| n.checked_add(width))
                .ok_or(Error::Stride)
        }
        let y_len = required(self.width, self.height, y_stride)?;
        let uv_len = required(self.width, self.height / 2, uv_stride)?;
        if y.len() < y_len || uv.len() < uv_len {
            return Err(Error::Length);
        }
        for row in 0..self.height {
            y[row * y_stride..row * y_stride + self.width]
                .copy_from_slice(&self.y[row * self.width..(row + 1) * self.width]);
        }
        for row in 0..self.height / 2 {
            uv[row * uv_stride..row * uv_stride + self.width]
                .copy_from_slice(&self.uv[row * self.width..(row + 1) * self.width]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_primary_codes() {
        // BT.709 studio-range reference codes, independently tabulated.
        for (rgb, expected) in [
            ([0, 0, 0], [16, 128, 128]),
            ([255, 255, 255], [235, 128, 128]),
            ([255, 0, 0], [63, 102, 240]),
            ([0, 255, 0], [173, 42, 26]),
            ([0, 0, 255], [32, 240, 118]),
        ] {
            let rgba = [rgb[0], rgb[1], rgb[2], 255].repeat(4);
            let frame = Nv12::from_rgba(2, 2, &rgba).unwrap();
            assert_eq!(frame.y, vec![expected[0]; 4]);
            assert_eq!(frame.uv, expected[1..]);
        }
    }

    #[test]
    fn box_filter_uses_all_four_pixels_and_interleaves_uv() {
        let frame = Nv12::from_rgba(
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        )
        .unwrap();
        assert_eq!(frame.y, [63, 173, 32, 235]);
        assert_eq!(frame.uv, [128, 128]);
    }

    #[test]
    fn malformed_input_and_overflow_are_rejected() {
        for (w, h) in [(0, 2), (2, 0), (3, 2), (2, 3), (usize::MAX - 1, 2)] {
            assert!(matches!(Nv12::from_rgba(w, h, &[]), Err(Error::Geometry)));
        }
        assert!(matches!(
            Nv12::from_rgba(2, 2, &[255; 15]),
            Err(Error::Length)
        ));
        assert!(matches!(
            Nv12::from_rgba(2, 2, &[0; 16]),
            Err(Error::NonOpaque)
        ));
    }

    #[test]
    fn mapped_plane_stride_and_atomic_validation() {
        let frame = Nv12::from_rgba(2, 4, &[255; 32]).unwrap();
        let (mut y, mut uv) = ([77; 16], [88; 8]);
        assert_eq!(
            frame.copy_to(&mut y, 4, &mut uv[..1], 4),
            Err(Error::Length)
        );
        assert_eq!(y, [77; 16]);
        assert_eq!(uv, [88; 8]);
        assert_eq!(frame.copy_to(&mut y, 1, &mut uv, 4), Err(Error::Stride));
        assert_eq!(
            frame.copy_to(&mut y, usize::MAX, &mut uv, 4),
            Err(Error::Stride)
        );
        frame.copy_to(&mut y, 4, &mut uv, 4).unwrap();
        assert_eq!(
            y,
            [235, 235, 77, 77, 235, 235, 77, 77, 235, 235, 77, 77, 235, 235, 77, 77]
        );
        assert_eq!(uv, [128, 128, 88, 88, 128, 128, 88, 88]);
    }
}
