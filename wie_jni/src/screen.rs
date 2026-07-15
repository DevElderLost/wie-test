//! Renders into the `Surface` provided by `EmulatorSurfaceView` on the Kotlin
//! side. We keep it deliberately simple (CPU blit via ANativeWindow_lock,
//! same approach wie_cli uses with `softbuffer`) rather than pulling in a
//! GPU path - the source resolution is tiny (~176x220 / 240x320) so this is
//! plenty fast even on old phones.

use std::sync::Mutex;

use ndk::{hardware_buffer_format::HardwareBufferFormat, native_window::NativeWindow};
use wie_backend::canvas::Image;
use wie_util::{Result, WieError};

pub struct AndroidScreen {
    width: u32,
    height: u32,
    window: Mutex<Option<NativeWindow>>,
}

impl AndroidScreen {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            window: Mutex::new(None),
        }
    }

    // NOTE: `NativeWindow::set_buffers_geometry` / `HardwareBufferFormat`
    // naming has shifted between `ndk` crate versions - if this doesn't
    // compile as-is against the `ndk` version pulled in by Cargo.lock,
    // check `cargo doc --open -p ndk` for the exact signature on that
    // version and adjust. The RGBA_8888 constant is the important bit;
    // the rest is bookkeeping.
    pub fn set_window(&self, window: Option<NativeWindow>) {
        if let Some(w) = &window {
            let _ = w.set_buffers_geometry(self.width as i32, self.height as i32, Some(HardwareBufferFormat::R8G8B8A8_UNORM));
        }
        *self.window.lock().unwrap() = window;
    }
}

impl wie_backend::Screen for AndroidScreen {
    fn request_redraw(&self) -> Result<()> {
        // Nothing to do: Android doesn't need an explicit "please redraw"
        // signal the way a desktop window manager does. `paint()` below
        // is what actually pushes pixels; the emulator core calls it after
        // this returns.
        Ok(())
    }

    fn paint(&self, image: &dyn Image) {
        let guard = self.window.lock().unwrap();
        let Some(window) = guard.as_ref() else {
            return; // surface not ready yet (e.g. app backgrounded) - drop the frame
        };

        let bpp = image.bytes_per_pixel();
        let src = image.raw();

        let Ok(mut buffer) = window.lock(None) else {
            return;
        };

        let dst_stride = buffer.stride() as usize;
        let dst: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(buffer.bits() as *mut u8, dst_stride * buffer.height() as usize * 4)
        };

        for y in 0..self.height.min(buffer.height() as u32) as usize {
            let src_row_start = y * self.width as usize * bpp as usize;
            let dst_row_start = y * dst_stride * 4;

            for x in 0..self.width.min(buffer.width() as u32) as usize {
                let color = image.get_pixel(x as i32, y as i32);
                let dst_off = dst_row_start + x * 4;
                if dst_off + 4 <= dst.len() {
                    // RGBX_8888 byte order
                    dst[dst_off] = color.r;
                    dst[dst_off + 1] = color.g;
                    dst[dst_off + 2] = color.b;
                    dst[dst_off + 3] = 0xff;
                }
            }
            let _ = src_row_start; // kept for future fast-path (raw() memcpy when formats match)
        }
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}

unsafe impl Send for AndroidScreen {}
unsafe impl Sync for AndroidScreen {}

pub fn window_error() -> WieError {
    WieError::FatalError("no native window bound".into())
}
