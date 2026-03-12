mod app;
mod input;
mod render;

#[cfg(target_os = "linux")]
mod fb;
#[cfg(target_os = "linux")]
mod kbd;
#[cfg(target_os = "linux")]
mod x11_backend;

#[cfg(target_os = "linux")]
fn run_best_effort(command: &str, args: &[&str]) -> bool {
    std::process::Command::new(command)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn configure_keep_awake_for_x11() {
    let ok = run_best_effort("xset", &["s", "off"])
        && run_best_effort("xset", &["-dpms"])
        && run_best_effort("xset", &["s", "noblank"]);
    if !ok {
        eprintln!("warning: could not fully disable X11 sleep/blanking");
    }
}

#[cfg(target_os = "linux")]
fn configure_keep_awake_for_tty() {
    let ok = std::process::Command::new("sh")
        .args([
            "-c",
            "setterm -blank 0 -powerdown 0 -powersave off </dev/tty0 >/dev/null 2>&1",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("warning: could not disable TTY blanking");
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    linux_main();

    #[cfg(not(target_os = "linux"))]
    desktop_main();
}

#[cfg(target_os = "linux")]
fn linux_main() {
    let x_socket_exists = std::path::Path::new("/tmp/.X11-unix/X0").exists();
    if std::env::var("DISPLAY").is_ok() || x_socket_exists {
        if std::env::var("DISPLAY").is_err() && x_socket_exists {
            std::env::set_var("DISPLAY", ":0");
        }
        match x11_main() {
            Ok(()) => return,
            Err(e) => eprintln!("X11 backend unavailable ({e}), falling back to framebuffer"),
        }
    }
    fb_main();
}

#[cfg(target_os = "linux")]
fn x11_main() -> Result<(), Box<dyn std::error::Error>> {
    use app::AppState;
    use input::{handle_keys, Action};
    use render::Renderer;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use x11_backend::X11Backend;

    let x11 = X11Backend::new()?;
    configure_keep_awake_for_x11();

    let mut state = AppState::initial();
    let renderer = Renderer::new(x11.width, x11.height);
    let mut buf = vec![0u32; x11.width * x11.height];

    let frame = Duration::from_millis(16);
    let mut load_rx: Option<mpsc::Receiver<app::BackgroundLoadResult>> = None;

    loop {
        let t0 = Instant::now();

        let keys = match x11.poll_keys() {
            Some(k) => k,
            None => break,
        };
        match handle_keys(&keys, &mut state) {
            Action::Quit => break,
            Action::None => {}
        }

        if state.is_loading() && load_rx.is_none() {
            if let Some(request) = state.loading_request() {
                load_rx = Some(app::start_background_load(request));
            }
        }
        if let Some(ref rx) = load_rx {
            if let Ok(result) = rx.try_recv() {
                state.apply_load_result(result);
                load_rx = None;
            }
        }

        state.update();
        renderer.draw(&mut buf, &state);
        x11.raise();
        x11.present(&buf);

        let elapsed = t0.elapsed();
        if elapsed < frame {
            std::thread::sleep(frame - elapsed);
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn fb_main() {
    use app::AppState;
    use fb::Framebuffer;
    use input::{handle_keys, Action};
    use kbd::Keyboard;
    use render::Renderer;
    use std::time::{Duration, Instant};

    let fb = Framebuffer::open().unwrap_or_else(|e| {
        eprintln!("framebuffer error: {}", e);
        std::process::exit(1);
    });

    let mut kbd = Keyboard::open();
    let mut state = AppState::initial();
    let renderer = Renderer::new(fb.width, fb.height);
    let mut buf = vec![0u32; fb.width * fb.height];

    Framebuffer::hide_cursor();
    configure_keep_awake_for_tty();

    let frame = Duration::from_millis(16);
    let mut load_rx: Option<std::sync::mpsc::Receiver<app::BackgroundLoadResult>> = None;

    loop {
        let t0 = Instant::now();

        let keys = kbd.poll();
        match handle_keys(&keys, &mut state) {
            Action::Quit => break,
            Action::None => {}
        }

        if state.is_loading() && load_rx.is_none() {
            if let Some(request) = state.loading_request() {
                load_rx = Some(app::start_background_load(request));
            }
        }
        if let Some(ref rx) = load_rx {
            if let Ok(result) = rx.try_recv() {
                state.apply_load_result(result);
                load_rx = None;
            }
        }

        state.update();
        renderer.draw(&mut buf, &state);
        fb.present(&buf);

        let elapsed = t0.elapsed();
        if elapsed < frame {
            std::thread::sleep(frame - elapsed);
        }
    }

    Framebuffer::show_cursor();
}

#[cfg(not(target_os = "linux"))]
fn desktop_main() {
    use app::AppState;
    use input::{handle_keys, Action, AppKey};
    use minifb::{Key, KeyRepeat, Scale, ScaleMode, Window, WindowOptions};
    use render::Renderer;

    const W: usize = 800;
    const H: usize = 480;

    let mut state = AppState::initial();
    let renderer = Renderer::new(W, H);
    let mut buf = vec![0u32; W * H];

    let mut window = Window::new(
        "Trivia",
        W,
        H,
        WindowOptions {
            borderless: true,
            title: false,
            resize: false,
            scale: Scale::X1,
            scale_mode: ScaleMode::UpperLeft,
            topmost: false,
            transparency: false,
            none: false,
        },
    )
    .expect("failed to create window");

    window.set_target_fps(60);
    let mut load_rx: Option<std::sync::mpsc::Receiver<app::BackgroundLoadResult>> = None;

    while window.is_open() {
        let raw: Vec<Key> = window.get_keys_pressed(KeyRepeat::No);
        let keys: Vec<AppKey> = raw.iter().filter_map(|&k| map_minifb_key(k)).collect();

        match handle_keys(&keys, &mut state) {
            Action::Quit => break,
            Action::None => {}
        }

        if state.is_loading() && load_rx.is_none() {
            if let Some(request) = state.loading_request() {
                load_rx = Some(app::start_background_load(request));
            }
        }
        if let Some(ref rx) = load_rx {
            if let Ok(result) = rx.try_recv() {
                state.apply_load_result(result);
                load_rx = None;
            }
        }

        state.update();
        renderer.draw(&mut buf, &state);
        window
            .update_with_buffer(&buf, W, H)
            .expect("buffer update failed");
    }
}

#[cfg(not(target_os = "linux"))]
fn map_minifb_key(k: minifb::Key) -> Option<input::AppKey> {
    use input::AppKey;
    use minifb::Key;
    match k {
        Key::Up | Key::W => Some(AppKey::Up),
        Key::Down | Key::S => Some(AppKey::Down),
        Key::Enter | Key::Space => Some(AppKey::Confirm),
        Key::Escape | Key::Backspace | Key::Q => Some(AppKey::Back),
        _ => None,
    }
}
