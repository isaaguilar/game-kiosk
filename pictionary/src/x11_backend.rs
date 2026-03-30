use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xfixes;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use crate::input::AppKey;

pub struct X11Backend {
    pub width: usize,
    pub height: usize,
    depth: u8,
    conn: RustConnection,
    root: Window,
    win: Window,
    gc: Gcontext,
    hidden_cursor: Cursor,
    xfixes_hidden: bool,
    pointer_grabbed: bool,
}

impl X11Backend {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, screen_num) = RustConnection::connect(None)?;
        let screen = conn.setup().roots[screen_num].clone();
        let root = screen.root;

        let width = screen.width_in_pixels as usize;
        let height = screen.height_in_pixels as usize;
        let depth = screen.root_depth;

        // Override-redirect window: the WM cannot intercept, decorate, or
        // reposition this window.  We control stacking ourselves and re-raise
        // every frame so nothing (including lxpanel) can sit above us.
        let win = conn.generate_id()?;
        conn.create_window(
            0,
            win,
            root,
            0,
            0,
            screen.width_in_pixels,
            screen.height_in_pixels,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .override_redirect(1u32)
                .background_pixel(0u32)
                .event_mask(EventMask::KEY_PRESS | EventMask::EXPOSURE),
        )?;

        // GC for rendering
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::default())?;

        // Disable keyboard auto-repeat
        conn.change_keyboard_control(
            &ChangeKeyboardControlAux::new().auto_repeat_mode(AutoRepeatMode::OFF),
        )?;

        // Map the window and force it to the top of the X11 stack
        conn.map_window(win)?;
        conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        conn.flush()?;

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Set input focus directly (no WM involvement for override_redirect)
        conn.set_input_focus(InputFocus::POINTER_ROOT, win, 0u32)?;

        // Grab the entire keyboard so lxpanel etc. can't receive any input
        let keyboard_status = conn
            .grab_keyboard(true, win, 0u32, GrabMode::ASYNC, GrabMode::ASYNC)?
            .reply()?
            .status;
        if keyboard_status != GrabStatus::SUCCESS {
            return Err(format!("failed to grab keyboard: {:?}", keyboard_status).into());
        }

        // Apply an explicit transparent cursor to both our window and root.
        let hidden_cursor = create_invisible_cursor(&conn, root)?;
        conn.change_window_attributes(
            win,
            &ChangeWindowAttributesAux::new().cursor(hidden_cursor),
        )?;
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().cursor(hidden_cursor),
        )?;

        // Also grab the pointer with that transparent cursor while we run.
        let pointer_status = conn
            .grab_pointer(
                false,
                win,
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                win,
                hidden_cursor,
                0u32,
            )?
            .reply()?
            .status;
        if pointer_status != GrabStatus::SUCCESS {
            return Err(format!("failed to grab pointer: {:?}", pointer_status).into());
        }

        // Also request XFixes cursor hiding when available.
        let mut xfixes_hidden = false;
        if let Ok(cookie) = xfixes::query_version(&conn, 4, 0) {
            if cookie.reply().is_ok() {
                xfixes::hide_cursor(&conn, root)?;
                xfixes_hidden = true;
            }
        }
        conn.flush()?;

        Ok(Self {
            width,
            height,
            depth,
            conn,
            root,
            win,
            gc,
            hidden_cursor,
            xfixes_hidden,
            pointer_grabbed: true,
        })
    }

    /// Re-raise the window to the top of the X11 stack.
    /// Called every frame to guarantee we stay above lxpanel/dock windows.
    pub fn raise(&self) {
        let _ = self.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        );
    }

    /// Write our 0x00RRGGBB pixel buffer to the X11 window via PutImage.
    pub fn present(&self, buf: &[u32]) {
        let w = self.width;
        let h = self.height;
        let bytes_per_row = w * 4;

        let max_bytes = self.conn.maximum_request_bytes();
        let max_data = max_bytes.saturating_sub(64);
        let rows_per_chunk = ((max_data / bytes_per_row).max(1)).min(h);

        let mut y = 0usize;
        while y < h {
            let chunk_h = rows_per_chunk.min(h - y);
            let start = y * w;
            let end = start + chunk_h * w;
            let data: &[u8] = unsafe {
                std::slice::from_raw_parts(buf[start..end].as_ptr() as *const u8, (end - start) * 4)
            };
            let _ = self.conn.put_image(
                ImageFormat::Z_PIXMAP,
                self.win,
                self.gc,
                w as u16,
                chunk_h as u16,
                0,
                y as i16,
                0,
                self.depth,
                data,
            );
            y += chunk_h;
        }
        let _ = self.conn.flush();
    }

    /// Drain all pending X11 events and return app key actions.
    /// Returns None if the X connection has been severed.
    pub fn poll_keys(&self) -> Option<Vec<AppKey>> {
        let mut keys = Vec::new();
        loop {
            match self.conn.poll_for_event() {
                Ok(None) => break,
                Ok(Some(Event::KeyPress(ev))) => {
                    if let Some(k) = x11_keycode_to_appkey(ev.detail) {
                        keys.push(k);
                    }
                }
                Ok(Some(_)) => {}
                Err(_) => return None,
            }
        }
        Some(keys)
    }
}

impl Drop for X11Backend {
    fn drop(&mut self) {
        let _ = self.conn.ungrab_keyboard(0u32);
        if self.pointer_grabbed {
            let _ = self.conn.ungrab_pointer(0u32);
        }
        let _ = self.conn.change_keyboard_control(
            &ChangeKeyboardControlAux::new().auto_repeat_mode(AutoRepeatMode::DEFAULT),
        );
        if self.xfixes_hidden {
            let _ = xfixes::show_cursor(&self.conn, self.root);
        }
        let _ = self
            .conn
            .change_window_attributes(self.win, &ChangeWindowAttributesAux::new().cursor(0));
        let _ = self
            .conn
            .change_window_attributes(self.root, &ChangeWindowAttributesAux::new().cursor(0));
        let _ = self.conn.free_cursor(self.hidden_cursor);
        let _ = self.conn.flush();
    }
}

fn create_invisible_cursor(
    conn: &RustConnection,
    drawable: Window,
) -> Result<Cursor, Box<dyn std::error::Error>> {
    let pixmap = conn.generate_id()?;
    conn.create_pixmap(1, pixmap, drawable, 1, 1)?;

    let cursor = conn.generate_id()?;
    conn.create_cursor(cursor, pixmap, pixmap, 0, 0, 0, 0, 0, 0, 0, 0)?;
    conn.free_pixmap(pixmap)?;

    Ok(cursor)
}

/// Map X11 hardware keycodes (evdev keycode + 8) to AppKey.
fn x11_keycode_to_appkey(code: u8) -> Option<AppKey> {
    match code {
        9 => Some(AppKey::Back),      // Escape
        22 => Some(AppKey::Back),     // BackSpace
        24 => Some(AppKey::Back),     // Q
        25 => Some(AppKey::Up),       // W
        36 => Some(AppKey::Confirm),  // Return
        39 => Some(AppKey::Down),     // S
        65 => Some(AppKey::Confirm),  // Space
        104 => Some(AppKey::Confirm), // KP_Enter
        111 => Some(AppKey::Up),      // Up
        116 => Some(AppKey::Down),    // Down
        _ => None,
    }
}
