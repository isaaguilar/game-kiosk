# Game Kiosk Workspace

A fullscreen Raspberry Pi kiosk workspace with three Rust binaries:

- `kiosk` (root package): home screen and game selector
- `charades` (member crate): difficulty-based word prompt game
- `pictionary` (member crate): single-list word prompt game (`start.txt`)

All apps are keyboard-driven and use the same no-sleep behavior for kiosk environments.

## Workspace Layout

- `Cargo.toml` -> workspace + `kiosk` package
- `src/` -> root kiosk app
- `charades/` -> charades crate (`charades/src`, `charades/assets`)
- `pictionary/` -> pictionary crate (`pictionary/src`, `pictionary/assets`)

## Controls

### Kiosk

| Key | Behavior |
|---|---|
| ↑ / W | Select previous game |
| ↓ / S | Select next game |
| Enter / Space | Launch selected game |
| Esc / Q / Backspace | Open quit prompt |
| Esc (in quit prompt) | Return to selection |
| Enter (in quit prompt) | Quit kiosk |

### Charades

| Key | Menu | Playing |
|---|---|---|
| ↑ / W | Move selection up | — |
| ↓ / S | Move selection down | — |
| Enter / Space | Start round | Next prompt |
| Esc / Q / Backspace | Quit app | Return to menu |

### Pictionary

| Key | Menu | Playing |
|---|---|---|
| Enter / Space | Start round | Next prompt |
| Esc / Q / Backspace | Quit app | Return to menu |

## Word Lists

- `charades/assets/easy.txt`
- `charades/assets/medium.txt`
- `charades/assets/hard.txt`
- `pictionary/assets/start.txt`

Prompts are loaded at runtime and reshuffled indefinitely.

## Run Locally (macOS smoke test)

```bash
cargo run -p kiosk
cargo run -p charades
cargo run -p pictionary
```

## Build Pi Bundle (Dev Machine)

```bash
./scripts/make_pi_bundle.sh
```

This creates `dist/pi-bundle/` containing:

- `kiosk`
- `charades`
- `pictionary`
- `trivia`
- `charades-assets/`
- `pictionary-assets/`
- `trivia-assets/`
- `kiosk.desktop`

Move `dist/pi-bundle/` to your Pi by any local method (USB drive, Samba share, local copy).

## Install On Pi (No SSH/SCP)

```bash
./scripts/install_pi_local.sh --source-dir /path/to/pi-bundle
```

Defaults:

- install dir: `~/.local/games-kiosk`
- autostart: enabled

Optional flags:

```bash
./scripts/install_pi_local.sh --source-dir /path/to/pi-bundle --no-autostart
./scripts/install_pi_local.sh --source-dir /path/to/pi-bundle --install-dir ~/games-kiosk
./scripts/install_pi_local.sh --source-dir /path/to/pi-bundle --dry-run
```

## Automated Remote Install (From Dev Machine)

If you want bundle + copy + install in one command from your computer:

```bash
./scripts/deploy_pi_remote.sh --host raspberrypi.local --user pi
```

This script will:

- build/update `dist/pi-bundle/`
- upload the bundle to the Pi via `scp`
- upload `install_pi_local.sh`
- run the installer over `ssh`

You can disable autostart during remote install:

```bash
./scripts/deploy_pi_remote.sh --host raspberrypi.local --user pi --no-autostart
```

If SSH keys are not configured, `scp`/`ssh` will prompt for password automatically.

## Trivia API Key On Pi (Safe Launch)

Installer now creates:

- launcher: `~/.local/games-kiosk/kiosk-launch`
- env file: `~/.config/games-kiosk/trivia.env` (permission `0600`)

Set your key only on the Pi:

```bash
chmod 600 ~/.config/games-kiosk/trivia.env
nano ~/.config/games-kiosk/trivia.env
```

Put this in the file:

```bash
export GOOGLE_API_KEY="your_api_key_here"
```

Desktop/autostart now launches the wrapper script, which sources `trivia.env` and then starts `kiosk`.

## Build for Raspberry Pi 4 (Manual)

If you still want a direct manual build:

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --workspace --release --target aarch64-unknown-linux-gnu
```

## Runtime Behavior

- Desktop session (`DISPLAY` set): fullscreen X11 path
- TTY/no desktop: framebuffer path (`/dev/fb0`)

All apps attempt to disable sleep/blanking at startup:

- X11: `xset s off`, `xset -dpms`, `xset s noblank`
- TTY: `setterm -blank 0 -powerdown 0 -powersave off`

## Permissions

If TTY mode cannot access `/dev/fb0` or `/dev/input/event*`:

```bash
sudo usermod -aG video,input pi
```

Then log out and back in.
