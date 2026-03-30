use crate::input::AppKey;
use libc;

// ── evdev constants ───────────────────────────────────────────────────────────

const EV_KEY: u16 = 0x01;

/// EVIOCGBIT(ev=0, size=4): returns bitmask of supported event types.
/// _IOC(READ=2, type='E'=0x45, nr=0x20, size=4)
const EVIOCGBIT_TYPES: u32 = (2 << 30) | (4 << 16) | (0x45 << 8) | 0x20;

// Linux evdev key codes (from linux/input-event-codes.h)
const K_ESC: u16 = 1;
const K_BACKSPACE: u16 = 14;
const K_Q: u16 = 16;
const K_W: u16 = 17;
const K_S: u16 = 31;
const K_ENTER: u16 = 28;
const K_SPACE: u16 = 57;
const K_UP: u16 = 103;
const K_DOWN: u16 = 108;
const K_KPENTER: u16 = 96;

/// `struct input_event` on 64-bit Linux (aarch64): 24 bytes.
#[repr(C)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    ev_type: u16,
    code: u16,
    value: i32,
}

fn bit_is_set(bits: &[u8], n: usize) -> bool {
    bits.get(n / 8).map_or(false, |&b| (b >> (n % 8)) & 1 != 0)
}

pub struct Keyboard {
    fds: Vec<libc::c_int>,
}

impl Keyboard {
    /// Scan /dev/input/event0..31, collect every device that emits EV_KEY.
    pub fn open() -> Self {
        let mut fds = Vec::new();
        for i in 0..32u32 {
            // NUL-terminated path for libc::open
            let path = format!("/dev/input/event{}\0", i);
            let fd = unsafe {
                libc::open(
                    path.as_ptr() as *const libc::c_char,
                    libc::O_RDONLY | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                continue;
            }

            // Check supported event types (4-byte bitmask)
            let mut evtypes = [0u8; 4];
            let r = unsafe { libc::ioctl(fd, EVIOCGBIT_TYPES as _, evtypes.as_mut_ptr()) };
            if r < 0 || !bit_is_set(&evtypes, EV_KEY as usize) {
                unsafe { libc::close(fd) };
                continue;
            }

            fds.push(fd);
        }
        Self { fds }
    }

    /// Drain all pending input events and return any key-press actions.
    pub fn poll(&mut self) -> Vec<AppKey> {
        let mut out = Vec::new();
        let ev_bytes = std::mem::size_of::<InputEvent>();

        for &fd in &self.fds {
            loop {
                let mut ev: InputEvent = unsafe { std::mem::zeroed() };
                let n = unsafe { libc::read(fd, &mut ev as *mut _ as *mut libc::c_void, ev_bytes) };
                if n != ev_bytes as libc::ssize_t {
                    break; // WouldBlock or error — nothing more to read
                }
                if ev.ev_type == EV_KEY && ev.value == 1 {
                    if let Some(k) = map_key(ev.code) {
                        out.push(k);
                    }
                }
            }
        }
        out
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        for &fd in &self.fds {
            unsafe { libc::close(fd) };
        }
    }
}

fn map_key(code: u16) -> Option<AppKey> {
    match code {
        K_UP | K_W => Some(AppKey::Up),
        K_DOWN | K_S => Some(AppKey::Down),
        K_ENTER | K_KPENTER | K_SPACE => Some(AppKey::Confirm),
        K_ESC | K_Q | K_BACKSPACE => Some(AppKey::Back),
        _ => None,
    }
}
