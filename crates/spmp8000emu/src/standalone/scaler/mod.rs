// Display scaler dispatcher and aspect-ratio-preserving presentation buffer.

pub mod scalers;

use clap::ValueEnum;
use scalers::bicubic::BicubicScaler;
use scalers::bilinear::BilinearScaler;
use scalers::nearest::NearestScaler;
use scalers::xbrz::XbrzScaler;

/// Scaling filter used by the standalone display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ScaleFilter {
    /// Preserve hard pixel edges.
    #[default]
    Nearest,
    /// Smooth pixels using a 2x2 neighbourhood.
    Bilinear,
    /// Apply separable Catmull-Rom interpolation.
    Bicubic,
    /// Smooth pixel-art diagonals while retaining sharp edges.
    Xbrz,
}

/// Algorithm state and reusable output storage.
pub struct Scaler {
    filter: ScaleFilter,
    nearest: NearestScaler,
    bilinear: BilinearScaler,
    bicubic: BicubicScaler,
    xbrz: XbrzScaler,
    output: Vec<u32>,
}

impl Scaler {
    pub fn new(filter: ScaleFilter) -> Self {
        Self {
            filter,
            nearest: NearestScaler::new(),
            bilinear: BilinearScaler::new(),
            bicubic: BicubicScaler::new(),
            xbrz: XbrzScaler::new(),
            output: Vec::new(),
        }
    }

    /// Scale `src` to `dst_w × dst_h` using reusable internal storage.
    pub fn scale(&mut self, src: &[u32], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> &[u32] {
        debug_assert_eq!(src.len(), (src_w * src_h) as usize);
        let len = (dst_w * dst_h) as usize;
        self.output.resize(len, 0);

        match self.filter {
            ScaleFilter::Nearest => {
                self.nearest
                    .scale(src, src_w, src_h, dst_w, dst_h, &mut self.output);
            }
            ScaleFilter::Bilinear => {
                self.bilinear
                    .scale(src, src_w, src_h, dst_w, dst_h, &mut self.output);
            }
            ScaleFilter::Bicubic => {
                self.bicubic
                    .scale(src, src_w, src_h, dst_w, dst_h, &mut self.output);
            }
            ScaleFilter::Xbrz => {
                self.xbrz
                    .scale(src, src_w, src_h, dst_w, dst_h, &mut self.output);
            }
        }

        &self.output
    }
}

/// Scales a native frame and centers it in a window-sized black buffer.
pub struct DisplayScaler {
    scaler: Scaler,
    presentation: Vec<u32>,
}

impl DisplayScaler {
    pub fn new(filter: ScaleFilter) -> Self {
        Self {
            scaler: Scaler::new(filter),
            presentation: Vec::new(),
        }
    }

    /// Return a buffer matching the current window dimensions.
    pub fn render(
        &mut self,
        src: &[u32],
        src_w: u32,
        src_h: u32,
        window_w: usize,
        window_h: usize,
    ) -> &[u32] {
        let window_w = window_w.max(1);
        let window_h = window_h.max(1);
        let (content_w, content_h) = fit_aspect(src_w, src_h, window_w as u32, window_h as u32);
        let scaled = self.scaler.scale(src, src_w, src_h, content_w, content_h);

        self.presentation.resize(window_w * window_h, 0);
        self.presentation.fill(0);

        let content_w = content_w as usize;
        let content_h = content_h as usize;
        let offset_x = (window_w - content_w) / 2;
        let offset_y = (window_h - content_h) / 2;
        for row in 0..content_h {
            let src_start = row * content_w;
            let dst_start = (row + offset_y) * window_w + offset_x;
            self.presentation[dst_start..dst_start + content_w]
                .copy_from_slice(&scaled[src_start..src_start + content_w]);
        }

        &self.presentation
    }
}

/// Convert the core XRGB8888 framebuffer to minifb's packed XRGB u32 format.
///
/// The framebuffer stores pixels in little-endian XRGB8888 byte order [B, G, R, X].
pub fn rgba_to_xrgb(src: &[u8], dst: &mut Vec<u32>) {
    dst.resize(src.len() / 4, 0);
    for (pixel, bytes) in dst.iter_mut().zip(src.as_chunks::<4>().0.iter()) {
        *pixel = ((bytes[2] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[0] as u32;
    }
}

fn fit_aspect(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> (u32, u32) {
    debug_assert!(src_w > 0 && src_h > 0 && dst_w > 0 && dst_h > 0);
    let src_w64 = u64::from(src_w);
    let src_h64 = u64::from(src_h);
    let dst_w64 = u64::from(dst_w);
    let dst_h64 = u64::from(dst_h);

    if dst_w64 * src_h64 <= dst_h64 * src_w64 {
        let height = ((dst_w64 * src_h64 + src_w64 / 2) / src_w64)
            .max(1)
            .min(dst_h64);
        (dst_w, height as u32)
    } else {
        let width = ((dst_h64 * src_w64 + src_h64 / 2) / src_h64)
            .max(1)
            .min(dst_w64);
        (width as u32, dst_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_fit_adds_horizontal_bars_for_widescreen() {
        assert_eq!(fit_aspect(320, 240, 1920, 1080), (1440, 1080));
    }

    #[test]
    fn aspect_fit_adds_vertical_bars_for_square_window() {
        assert_eq!(fit_aspect(320, 240, 1000, 1000), (1000, 750));
    }

    #[test]
    fn rgba_conversion_ignores_alpha_and_reuses_storage() {
        // Input is little-endian XRGB8888: [B, G, R, X]
        let mut output = Vec::with_capacity(2);
        rgba_to_xrgb(&[0x56, 0x34, 0x12, 0x78, 0xEF, 0xCD, 0xAB, 0], &mut output);
        let pointer = output.as_ptr();
        assert_eq!(output, [0x00123456, 0x00ABCDEF]);

        rgba_to_xrgb(&[3, 2, 1, 4, 7, 6, 5, 8], &mut output);
        assert_eq!(pointer, output.as_ptr());
        assert_eq!(output, [0x00010203, 0x00050607]);
    }

    #[test]
    fn presentation_is_centered_with_black_bars() {
        let src = [0x00112233; 4];
        let mut display = DisplayScaler::new(ScaleFilter::Nearest);
        let output = display.render(&src, 2, 2, 4, 2);

        assert_eq!(output.len(), 8);
        assert_eq!(
            output,
            [0, 0x00112233, 0x00112233, 0, 0, 0x00112233, 0x00112233, 0]
        );
    }

    #[test]
    fn presentation_buffer_is_reused() {
        let src = [0x00112233; 4];
        let mut display = DisplayScaler::new(ScaleFilter::Nearest);
        let first = display.render(&src, 2, 2, 8, 8).as_ptr();
        let second = display.render(&src, 2, 2, 8, 8).as_ptr();
        assert_eq!(first, second);
    }

    #[test]
    fn every_filter_preserves_one_times_output() {
        let src = [0x00112233, 0x00445566, 0x00778899, 0x00AABBCC];
        for filter in [
            ScaleFilter::Nearest,
            ScaleFilter::Bilinear,
            ScaleFilter::Bicubic,
            ScaleFilter::Xbrz,
        ] {
            let mut scaler = Scaler::new(filter);
            assert_eq!(scaler.scale(&src, 2, 2, 2, 2), src);
        }
    }
}
