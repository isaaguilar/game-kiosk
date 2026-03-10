use libc;

const FBIOGET_VSCREENINFO: u32 = 0x4600;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FbBitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

/// Matches `struct fb_var_screeninfo` from linux/fb.h (all u32 fields, no padding surprises).
#[repr(C)]
#[derive(Default)]
struct FbVarScreenInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: FbBitfield,
    green: FbBitfield,
    blue: FbBitfield,
    transp: FbBitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    stride: usize,     // pixels per row (>= width)
    red_off: u32,
    green_off: u32,
    blue_off: u32,
    bpp: u32,
    ptr: *mut u8,
    mmap_size: usize,
    fd: libc::c_int,
}

impl Framebuffer {
    pub fn open() -> std::io::Result<Self> {
        let path = b"/dev/fb0\0";
        let fd = unsafe {
            libc::open(path.as_ptr() as *const libc::c_char, libc::O_RDWR)
        };
        if fd < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot open /dev/fb0. Run from a TTY (not inside a desktop session), \
                 or add your user to the 'video' group: sudo usermod -aG video $USER",
            ));
        }

        let mut var: FbVarScreenInfo = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut var as *mut _) } < 0 {
            unsafe { libc::close(fd) };
            return Err(std::io::Error::last_os_error());
        }

        let bpp = var.bits_per_pixel;
        if bpp != 16 && bpp != 32 {
            unsafe { libc::close(fd) };
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unsupported framebuffer depth {}bpp (need 16 or 32)", bpp),
            ));
        }

        let bytes_per_pixel = (bpp / 8) as usize;
        let stride = var.xres_virtual as usize;
        let mmap_size = var.yres_virtual as usize * stride * bytes_per_pixel;

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                mmap_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            unsafe { libc::close(fd) };
            return Err(std::io::Error::last_os_error());
        }

        Ok(Self {
            width: var.xres as usize,
            height: var.yres as usize,
            stride,
            red_off: var.red.offset,
            green_off: var.green.offset,
            blue_off: var.blue.offset,
            bpp,
            ptr: ptr as *mut u8,
            mmap_size,
            fd,
        })
    }

    /// Write a 0x00RRGGBB pixel buffer (width × height) to the framebuffer.
    pub fn present(&self, buf: &[u32]) {
        match self.bpp {
            32 => self.present_32(buf),
            16 => self.present_16(buf),
            _ => {}
        }
    }

    fn present_32(&self, buf: &[u32]) {
        let fb = self.ptr as *mut u32;
        for y in 0..self.height {
            for x in 0..self.width {
                let src = buf[y * self.width + x];
                let r = (src >> 16) & 0xFF;
                let g = (src >> 8) & 0xFF;
                let b = src & 0xFF;
                let pixel = (r << self.red_off) | (g << self.green_off) | (b << self.blue_off);
                unsafe { *fb.add(y * self.stride + x) = pixel; }
            }
        }
    }

    fn present_16(&self, buf: &[u32]) {
        let fb = self.ptr as *mut u16;
        for y in 0..self.height {
            for x in 0..self.width {
                let src = buf[y * self.width + x];
                let r = (src >> 16) & 0xFF;
                let g = (src >> 8) & 0xFF;
                let b = src & 0xFF;
                // Scale 8-bit channels to the bit-field lengths
                let rs = (r >> (8 - (5.min(self.red_off.max(self.red_off))))) & 0x1F;
                let _ = rs; // suppress; use generic shift below
                let r5 = (r >> 3) & 0x1F;
                let g6 = (g >> 2) & 0x3F;
                let b5 = (b >> 3) & 0x1F;
                let pixel = ((r5 << self.red_off) | (g6 << self.green_off) | (b5 << self.blue_off)) as u16;
                unsafe { *fb.add(y * self.stride + x) = pixel; }
            }
        }
    }

    /// Hide the blinking cursor on the framebuffer console.
    pub fn hide_cursor() {
        let _ = std::fs::write(
            "/sys/class/graphics/fbcon/cursor_blink",
            b"0",
        );
        // Also send VT escape to hide cursor in the TTY
        print!("\x1b[?25l");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }

    pub fn show_cursor() {
        print!("\x1b[?25h");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::fs::write(
            "/sys/class/graphics/fbcon/cursor_blink",
            b"1",
        );
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.mmap_size);
            libc::close(self.fd);
        }
    }
}

// SAFETY: Framebuffer is not Send/Sync by default due to raw pointer; we only ever
// use it from the main thread.
unsafe impl Send for Framebuffer {}
