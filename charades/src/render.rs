use crate::app::{AppState, MENU_ITEMS};
use fontdue::{
    layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign},
    Font, FontSettings,
};

// ── Palette ──────────────────────────────────────────────────────────────────

const BLACK: u32 = 0x00_00_00_00;
const WHITE: u32 = 0x00_FF_FF_FF;

// ── Font ─────────────────────────────────────────────────────────────────────

static FONT_BYTES: &[u8] = include_bytes!("../assets/font.ttf");

pub struct Renderer {
    pub width: usize,
    pub height: usize,
    font: Font,
}

impl Renderer {
    pub fn new(width: usize, height: usize) -> Self {
        let font = Font::from_bytes(FONT_BYTES, FontSettings::default())
            .expect("failed to load bundled font");
        Self {
            width,
            height,
            font,
        }
    }

    /// Render the current app state into the pixel buffer.
    pub fn draw(&self, buf: &mut Vec<u32>, state: &AppState) {
        // Clear to black
        buf.iter_mut().for_each(|p| *p = BLACK);

        match state {
            AppState::Menu { selected } => self.draw_menu(buf, *selected),
            AppState::Playing {
                current_prompt,
                difficulty,
                ..
            } => {
                self.draw_playing(buf, current_prompt, difficulty.label());
            }
        }
    }

    // ── Menu ─────────────────────────────────────────────────────────────────

    fn draw_menu(&self, buf: &mut Vec<u32>, selected: usize) {
        let title = "CHARADES";
        let title_size = 72.0_f32;
        let item_size = 42.0_f32;
        let cx = self.width / 2;

        // Title
        let title_y = self.height / 5;
        self.draw_text_centered(buf, title, title_size, cx, title_y, WHITE);

        // Menu items — spread evenly in the lower 60% of screen
        let item_count = MENU_ITEMS.len();
        let area_top = self.height * 2 / 5;
        let area_bottom = self.height * 17 / 20;
        let pad_x = 20usize;
        let pad_y = 8usize;
        let max_label_h = MENU_ITEMS
            .iter()
            .map(|item| self.measure_text(item.label(), item_size).1)
            .max()
            .unwrap_or(item_size as usize);
        let min_gap = 14usize;
        let step = (max_label_h + 2 * pad_y + min_gap).max(1);
        let total_span = step.saturating_mul(item_count.saturating_sub(1));
        let available = area_bottom.saturating_sub(area_top);
        let start_y = area_top + available.saturating_sub(total_span) / 2;

        for (i, item) in MENU_ITEMS.iter().enumerate() {
            let item_y = start_y + step * i;
            self.draw_text_centered(buf, item.label(), item_size, cx, item_y, WHITE);

            if i == selected {
                // Draw outline box around selected item
                let (tw, th) = self.measure_text(item.label(), item_size);
                let x0 = cx.saturating_sub(tw / 2 + pad_x);
                let y0 = item_y.saturating_sub(th / 2 + pad_y);
                let x1 = (cx + tw / 2 + pad_x).min(self.width - 1);
                let y1 = (item_y + th / 2 + pad_y).min(self.height - 1);
                self.draw_rect_outline(buf, x0, y0, x1, y1, WHITE);
            }
        }

        // Footer hint
        let hint = "↑↓ select   Enter/Space start   Esc quit";
        self.draw_text_centered(buf, hint, 18.0, cx, self.height - 24, WHITE);
    }

    // ── Playing ──────────────────────────────────────────────────────────────

    fn draw_playing(&self, buf: &mut Vec<u32>, prompt: &str, difficulty_label: &str) {
        let cx = self.width / 2;
        let cy = self.height / 2;
        let margin_x = (self.width as f32 * 0.06) as usize;
        let max_w = self.width - 2 * margin_x;
        let max_h = (self.height as f32 * 0.55) as usize;

        // Find the largest font size that fits in one line
        let font_size = self.fit_font_size(prompt, max_w, max_h, 14.0, 200.0);
        self.draw_text_centered(buf, prompt, font_size, cx, cy, WHITE);

        // Difficulty label top-left
        self.draw_text_centered(buf, difficulty_label, 22.0, 60, 22, WHITE);

        // Footer hint
        let hint = "Enter/Space next   Esc menu";
        self.draw_text_centered(buf, hint, 18.0, cx, self.height - 24, WHITE);
    }

    // ── Text helpers ─────────────────────────────────────────────────────────

    /// Return (width_px, height_px) for a string at a given font size.
    fn measure_text(&self, text: &str, size: f32) -> (usize, usize) {
        let mut layout: Layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Top,
            ..Default::default()
        });
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));
        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return (0, size as usize);
        }
        let min_x = glyphs.iter().map(|g| g.x as i32).min().unwrap_or(0);
        let max_x = glyphs
            .iter()
            .map(|g| (g.x + g.width as f32) as i32)
            .max()
            .unwrap_or(0);
        let min_y = glyphs.iter().map(|g| g.y as i32).min().unwrap_or(0);
        let max_y = glyphs
            .iter()
            .map(|g| (g.y + g.height as f32) as i32)
            .max()
            .unwrap_or(0);
        let height = (max_y - min_y).unsigned_abs() as usize;
        let width = (max_x - min_x).unsigned_abs() as usize;
        (width, height)
    }

    /// Binary-search for the largest font size where text fits within max_w × max_h.
    fn fit_font_size(&self, text: &str, max_w: usize, max_h: usize, min: f32, max: f32) -> f32 {
        let mut lo = min;
        let mut hi = max;
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            let (w, h) = self.measure_text(text, mid);
            if w <= max_w && h <= max_h {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Draw text at the given pixel size, centered at (cx, cy).
    pub fn draw_text_centered(
        &self,
        buf: &mut Vec<u32>,
        text: &str,
        size: f32,
        cx: usize,
        cy: usize,
        color: u32,
    ) {
        let mut layout: Layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            horizontal_align: HorizontalAlign::Left,
            vertical_align: VerticalAlign::Top,
            ..Default::default()
        });
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));

        let glyphs = layout.glyphs().to_vec();
        if glyphs.is_empty() {
            return;
        }

        // Compute bounding box to center the whole run
        let min_gx = glyphs.iter().map(|g| g.x as i32).min().unwrap_or(0);
        let max_gx = glyphs
            .iter()
            .map(|g| (g.x + g.width as f32) as i32)
            .max()
            .unwrap_or(0);
        let total_w = (max_gx - min_gx).unsigned_abs() as i32;
        let total_h = layout.height() as i32;

        let draw_x = cx as i32 - total_w / 2 - min_gx;
        let draw_y = cy as i32 - total_h / 2;

        for glyph in &glyphs {
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let (metrics, bitmap) = self.font.rasterize(glyph.parent, size);
            let gx = draw_x + glyph.x as i32;
            let gy = draw_y + glyph.y as i32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col] as u32;
                    if alpha == 0 {
                        continue;
                    }
                    let px = gx + col as i32;
                    let py = gy + row as i32;
                    if px < 0 || py < 0 {
                        continue;
                    }
                    let px = px as usize;
                    let py = py as usize;
                    if px >= self.width || py >= self.height {
                        continue;
                    }
                    let idx = py * self.width + px;
                    // Alpha blend onto black
                    let r = ((color >> 16) & 0xFF) * alpha / 255;
                    let g = ((color >> 8) & 0xFF) * alpha / 255;
                    let b = (color & 0xFF) * alpha / 255;
                    buf[idx] = (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    /// Draw a 1-pixel-thick axis-aligned rectangle outline.
    fn draw_rect_outline(
        &self,
        buf: &mut Vec<u32>,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        color: u32,
    ) {
        // Top and bottom edges
        for x in x0..=x1 {
            if y0 < self.height {
                buf[y0 * self.width + x] = color;
            }
            if y1 < self.height {
                buf[y1 * self.width + x] = color;
            }
        }
        // Left and right edges
        for y in y0..=y1 {
            if y < self.height {
                if x0 < self.width {
                    buf[y * self.width + x0] = color;
                }
                if x1 < self.width {
                    buf[y * self.width + x1] = color;
                }
            }
        }
    }
}
