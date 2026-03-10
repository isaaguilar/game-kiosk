use crate::app::{AppState, MENU_ITEMS};
use fontdue::{
    layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign},
    Font, FontSettings,
};

const WHITE: u32 = 0x00_FF_F3_D9;
const GOLD: u32 = 0x00_F4_C9_5D;
const TEAL: u32 = 0x00_4D_B4_AA;
const NAVY: u32 = 0x00_12_2B_44;
const DUSK: u32 = 0x00_0B_18_2A;

static FONT_BYTES: &[u8] = include_bytes!("../charades/assets/font.ttf");

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

    pub fn draw(&self, buf: &mut Vec<u32>, state: &AppState) {
        self.fill_gradient(buf, DUSK, NAVY);
        self.draw_glow(buf);

        match state {
            AppState::GameSelect { selected } => self.draw_menu(buf, *selected),
            AppState::QuitPrompt { .. } => {
                self.draw_menu(buf, state.current_selection());
                self.draw_quit_prompt(buf);
            }
            AppState::LaunchGame { .. } => self.draw_transition(buf),
        }
    }

    fn draw_menu(&self, buf: &mut Vec<u32>, selected: usize) {
        let cx = self.width / 2;
        self.draw_text_centered(buf, "GAME KIOSK", 78.0, cx, self.height / 6, WHITE);
        self.draw_text_centered(
            buf,
            "Tap in. Play. Escape returns here.",
            24.0,
            cx,
            self.height / 6 + 54,
            TEAL,
        );

        let item_size = 48.0_f32;
        let area_top = self.height * 2 / 5;
        let area_bottom = self.height * 17 / 20;
        let pad_x = 22usize;
        let pad_y = 8usize;
        let max_label_h = MENU_ITEMS
            .iter()
            .map(|item| self.measure_text(item.label(), item_size).1)
            .max()
            .unwrap_or(item_size as usize);
        let min_gap = 14usize;
        let step = (max_label_h + 2 * pad_y + min_gap).max(1);
        let total_span = step.saturating_mul(MENU_ITEMS.len().saturating_sub(1));
        let available = area_bottom.saturating_sub(area_top);
        let start_y = area_top + available.saturating_sub(total_span) / 2;

        for (i, item) in MENU_ITEMS.iter().enumerate() {
            let y = start_y + step * i;
            let color = if i == selected { GOLD } else { WHITE };
            self.draw_text_centered(buf, item.label(), item_size, cx, y, color);
            if i == selected {
                let (tw, th) = self.measure_text(item.label(), item_size);
                let x0 = cx.saturating_sub(tw / 2 + pad_x);
                let y0 = y.saturating_sub(th / 2 + pad_y);
                let x1 = (cx + tw / 2 + pad_x).min(self.width - 1);
                let y1 = (y + th / 2 + pad_y).min(self.height - 1);
                self.draw_rect_outline(buf, x0, y0, x1, y1, GOLD);
            }
        }

        self.draw_text_centered(
            buf,
            "UP/DOWN choose   ENTER launch   ESC quit prompt",
            18.0,
            cx,
            self.height - 24,
            WHITE,
        );
    }

    fn draw_quit_prompt(&self, buf: &mut Vec<u32>) {
        let cx = self.width / 2;
        let cy = self.height / 2;
        let w = self.width * 2 / 3;
        let h = self.height / 3;
        let x0 = cx.saturating_sub(w / 2);
        let y0 = cy.saturating_sub(h / 2);
        let x1 = (x0 + w).min(self.width - 1);
        let y1 = (y0 + h).min(self.height - 1);

        self.fill_rect(buf, x0, y0, x1, y1, 0x00_0A_12_1E);
        self.draw_rect_outline(buf, x0, y0, x1, y1, GOLD);
        self.draw_text_centered(buf, "Quit Kiosk?", 44.0, cx, cy - 34, WHITE);
        self.draw_text_centered(buf, "ENTER exits   ESC returns", 24.0, cx, cy + 24, TEAL);
    }

    fn draw_transition(&self, buf: &mut Vec<u32>) {
        let cx = self.width / 2;
        let cy = self.height / 2;
        self.draw_text_centered(buf, "Launching...", 38.0, cx, cy, GOLD);
    }

    fn fill_gradient(&self, buf: &mut [u32], top: u32, bottom: u32) {
        let (tr, tg, tb) = ((top >> 16) & 0xFF, (top >> 8) & 0xFF, top & 0xFF);
        let (br, bg, bb) = (
            (bottom >> 16) & 0xFF,
            (bottom >> 8) & 0xFF,
            bottom & 0xFF,
        );
        for y in 0..self.height {
            let t = y as u32 * 255 / (self.height.max(1) as u32);
            let r = (tr * (255 - t) + br * t) / 255;
            let g = (tg * (255 - t) + bg * t) / 255;
            let b = (tb * (255 - t) + bb * t) / 255;
            let color = (r << 16) | (g << 8) | b;
            let row = y * self.width;
            for x in 0..self.width {
                buf[row + x] = color;
            }
        }
    }

    fn draw_glow(&self, buf: &mut [u32]) {
        let center_x = (self.width as i32) / 5;
        let center_y = (self.height as i32) / 4;
        let radius = (self.width.min(self.height) as i32) / 3;
        for y in 0..self.height {
            for x in 0..self.width {
                let dx = x as i32 - center_x;
                let dy = y as i32 - center_y;
                let d2 = dx * dx + dy * dy;
                if d2 > radius * radius {
                    continue;
                }
                let strength = ((radius * radius - d2) as u32 * 70) / (radius * radius) as u32;
                let idx = y * self.width + x;
                let base = buf[idx];
                let r = ((base >> 16) & 0xFF).saturating_add((strength * 77) / 255);
                let g = ((base >> 8) & 0xFF).saturating_add((strength * 180) / 255);
                let b = (base & 0xFF).saturating_add((strength * 170) / 255);
                buf[idx] = (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
            }
        }
    }

    fn fill_rect(&self, buf: &mut [u32], x0: usize, y0: usize, x1: usize, y1: usize, color: u32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                buf[y * self.width + x] = color;
            }
        }
    }

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
                    let r = ((color >> 16) & 0xFF) * alpha / 255;
                    let g = ((color >> 8) & 0xFF) * alpha / 255;
                    let b = (color & 0xFF) * alpha / 255;
                    buf[idx] = (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    fn draw_rect_outline(
        &self,
        buf: &mut [u32],
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        color: u32,
    ) {
        for x in x0..=x1 {
            if y0 < self.height {
                buf[y0 * self.width + x] = color;
            }
            if y1 < self.height {
                buf[y1 * self.width + x] = color;
            }
        }
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
