# Charades RPi Kiosk

A fullscreen keyboard-driven charades prompt app for Raspberry Pi 4.

## Controls

| Key | Menu | Playing |
|---|---|---|
| ↑ / W | Move selection up | — |
| ↓ / S | Move selection down | — |
| Enter / Space | Start round | Next prompt |
| Esc / Q / Backspace | Quit app | Return to menu |

## Difficulties

| Level | Source |
|---|---|
| Easy | `assets/easy.txt` |
| Medium | `assets/medium.txt` |
| Hard | `assets/hard.txt` |

Prompts are drawn in a shuffled random order. When the pool is exhausted it reshuffles automatically and continues indefinitely.
The app reads these files at runtime relative to the binary path (`<binary_dir>/assets/...`), so you can edit words without recompiling.

## Building

### Local smoke-test (macOS — uses a minifb window)

```bash
cargo r
```

### Raspberry Pi 4 (recommended) — GNU build

If you're deploying to Raspberry Pi OS Desktop, `aarch64-unknown-linux-gnu` is the most practical default.
It links against glibc on the Pi, which is already present on standard Pi OS images.

```bash
# Add target
rustup target add aarch64-unknown-linux-gnu

# Build
cargo build --release --target aarch64-unknown-linux-gnu
```

The Pi binary lives at:
```
target/aarch64-unknown-linux-gnu/release/charades
```

## Deploying to Pi

Copy the binary, the `assets` folder, and the `.desktop` launcher:
```bash
user=pi
rpi_ip=1.2.3.4
scp target/aarch64-unknown-linux-gnu/release/charades $user@$rpi_ip:~/
scp -r assets $user@$rpi_ip:~/
tmpfile=$(mktemp)
sed "s/pi/$user/" charades.desktop > $tmpfile
scp $tmpfile $user@$rpi_ip:~/Desktop/charades.desktop
ssh $user@$rpi_ip chmod +x ~/Desktop/charades.desktop
```

## Running on the Pi

The binary auto-detects its environment at startup:
- **Desktop session** (`DISPLAY` is set) → opens a fullscreen X11 window that **covers the taskbar**, hides the cursor
- **TTY / CLI** (no `DISPLAY`) → writes directly to `/dev/fb0`

At startup, the app also attempts to prevent sleep/blanking:
- On X11 desktop: runs `xset s off`, `xset -dpms`, `xset s noblank`
- On TTY framebuffer: runs `setterm -blank 0 -powerdown 0 -powersave off`

These are best-effort calls. If unavailable or blocked by permissions/session policy, the app continues and prints a warning.

### From Pi OS Desktop — double-click to launch

Double-click the `charades` icon on the Desktop.  
PCManFM will ask "Execute" or "Open" the first time — choose **Execute**.

The app opens fullscreen, covering the taskbar entirely, with the cursor hidden.

### From a TTY (kiosk / no desktop)

```bash
./charades
```

### Permissions

If you get a "permission denied" error on `/dev/fb0` or `/dev/input/event*` (TTY mode only):
```bash
sudo usermod -aG video,input pi
# then log out and back in
```

## Autostart on desktop login (optional)

Add `charades.desktop` to autostart:
```bash
mkdir -p ~/.config/autostart
cp ~/Desktop/charades.desktop ~/.config/autostart/
```

Or for a headless systemd boot with X:
```ini
# /etc/systemd/system/charades.service
[Unit]
Description=Charades Kiosk
After=graphical-session.target

[Service]
User=pi
Environment=DISPLAY=:0
ExecStart=/home/pi/charades
Restart=always

[Install]
WantedBy=graphical-session.target
```
Then: `sudo systemctl enable --now charades`

