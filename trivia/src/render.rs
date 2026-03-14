use crate::app::AppState;
use fontdue::{
    layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign},
    Font, FontSettings,
};

const BLACK: u32 = 0x00_00_00_00;
const WHITE: u32 = 0x00_FF_FF_FF;
const GRAY: u32 = 0x00_88_88_88;
const YELLOW: u32 = 0x00_FF_FF_00;
const GREEN: u32 = 0x00_33_CC_66;
const RED: u32 = 0x00_E5_39_35;

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

    pub fn draw(&self, buf: &mut Vec<u32>, state: &AppState) {
        buf.iter_mut().for_each(|p| *p = BLACK);

        let cx = self.width / 2;
        let cy = self.height / 2;

        match state {
            AppState::SubjectMenu { subjects, selected } => {
                self.draw_text_centered(buf, "TRIVIA", 72.0, cx, self.height / 5, WHITE);
                self.draw_text_centered(buf, "Choose a Subject", 28.0, cx, self.height / 3, GRAY);

                let item_size = 36.0_f32;
                let area_top = self.height * 2 / 5;
                let area_bottom = self.height * 17 / 20;
                let pad_x = 20usize;
                let pad_y = 8usize;
                let max_label_h = subjects
                    .iter()
                    .map(|subject| self.measure_text(subject.label(), item_size).1)
                    .max()
                    .unwrap_or(item_size as usize);
                let min_gap = 14usize;
                let step = (max_label_h + 2 * pad_y + min_gap).max(1);
                let total_span = step.saturating_mul(subjects.len().saturating_sub(1));
                let available = area_bottom.saturating_sub(area_top);
                let start_y = area_top + available.saturating_sub(total_span) / 2;

                for (idx, subject) in subjects.iter().enumerate() {
                    let y = start_y + step * idx;
                    let label = subject.label();
                    let color = if idx == *selected { YELLOW } else { WHITE };
                    self.draw_text_centered(buf, label, item_size, cx, y, color);
                    if idx == *selected {
                        let (tw, th) = self.measure_text(label, item_size);
                        let x0 = cx.saturating_sub(tw / 2 + pad_x);
                        let y0 = y.saturating_sub(th / 2 + pad_y);
                        let x1 = (cx + tw / 2 + pad_x).min(self.width - 1);
                        let y1 = (y + th / 2 + pad_y).min(self.height - 1);
                        self.draw_rect_outline(buf, x0, y0, x1, y1, YELLOW);
                    }
                }
                self.draw_text_centered(
                    buf,
                    "Up/Down choose   Enter/Space load   Esc quit",
                    18.0,
                    cx,
                    self.height - 24,
                    GRAY,
                );
            }
            AppState::NewsCategoryMenu {
                categories,
                selected,
            } => {
                self.draw_text_centered(buf, "RECENT NEWS", 62.0, cx, self.height / 5, WHITE);
                self.draw_text_centered(buf, "Choose a Category", 28.0, cx, self.height / 3, GRAY);

                let item_size = 30.0_f32;
                let area_top = self.height * 2 / 5;
                let area_bottom = self.height * 17 / 20;
                let pad_x = 20usize;
                let pad_y = 8usize;
                let max_label_h = categories
                    .iter()
                    .map(|category| self.measure_text(category.label(), item_size).1)
                    .max()
                    .unwrap_or(item_size as usize);
                let min_gap = 14usize;
                let step = (max_label_h + 2 * pad_y + min_gap).max(1);
                let total_span = step.saturating_mul(categories.len().saturating_sub(1));
                let available = area_bottom.saturating_sub(area_top);
                let start_y = area_top + available.saturating_sub(total_span) / 2;

                for (idx, category) in categories.iter().enumerate() {
                    let y = start_y + step * idx;
                    let label = category.label();
                    let color = if idx == *selected { YELLOW } else { WHITE };
                    self.draw_text_centered(buf, label, item_size, cx, y, color);
                    if idx == *selected {
                        let (tw, th) = self.measure_text(label, item_size);
                        let x0 = cx.saturating_sub(tw / 2 + pad_x);
                        let y0 = y.saturating_sub(th / 2 + pad_y);
                        let x1 = (cx + tw / 2 + pad_x).min(self.width - 1);
                        let y1 = (y + th / 2 + pad_y).min(self.height - 1);
                        self.draw_rect_outline(buf, x0, y0, x1, y1, YELLOW);
                    }
                }
                self.draw_text_centered(
                    buf,
                    "Up/Down choose   Enter/Space load   Esc back",
                    18.0,
                    cx,
                    self.height - 24,
                    GRAY,
                );
            }
            AppState::Loading {
                status, started_at, ..
            } => {
                self.draw_text_centered(buf, "TRIVIA", 72.0, cx, self.height / 4, WHITE);
                let elapsed_ms = started_at.elapsed().as_millis() as u64;
                let dot_count = ((elapsed_ms / 400) % 4) as usize;
                let dots = &"..."[..dot_count];
                let animated = format!("{}{}", status, dots);
                self.draw_text_centered(buf, &animated, 28.0, cx, cy - 20, WHITE);
                self.draw_spinner(buf, cx, cy + 40, elapsed_ms);
            }
            AppState::Error { request, message } => {
                self.draw_text_centered(buf, "ERROR", 72.0, cx, self.height / 4, 0xFF0000);
                self.draw_text_centered(
                    buf,
                    &request.menu_title(),
                    22.0,
                    cx,
                    self.height / 4 + 46,
                    GRAY,
                );
                self.draw_multiline_text_centered(buf, message, 24.0, cx, cy, WHITE);
                self.draw_text_centered(
                    buf,
                    "Press Enter or Esc to go back",
                    18.0,
                    cx,
                    self.height - 24,
                    GRAY,
                );
            }
            AppState::Ready { request, items, .. } => {
                self.draw_text_centered(buf, "TRIVIA", 72.0, cx, self.height / 4, WHITE);
                self.draw_text_centered(
                    buf,
                    &request.menu_title(),
                    24.0,
                    cx,
                    self.height / 4 + 44,
                    GRAY,
                );
                let msg = format!("{} questions loaded.", items.len());
                self.draw_text_centered(buf, &msg, 32.0, cx, cy, WHITE);
                self.draw_text_centered(
                    buf,
                    "Press Enter/Space to start",
                    24.0,
                    cx,
                    cy + 60,
                    WHITE,
                );
            }
            AppState::Question {
                request,
                items,
                current_idx,
                start_time,
                duration,
            } => {
                let item = &items[*current_idx];
                let timer_radius = 40usize;
                let timer_cx = self.width.saturating_sub(timer_radius + 20);
                let timer_cy = timer_radius + 20;
                let elapsed = start_time.elapsed().as_secs_f32();
                let total = duration.as_secs_f32().max(1.0);
                let progress = (elapsed / total).clamp(0.0, 1.0);
                let remaining = 1.0 - progress;
                let timer_color = Self::heat_color(remaining);
                let seconds_left = (duration
                    .saturating_sub(std::time::Duration::from_secs_f32(elapsed))
                    .as_secs()) as i32;

                let heading = if request.subject == crate::app::TriviaSubject::RecentNews {
                    format!("QUESTION - {}", request.menu_title())
                } else {
                    "QUESTION".to_string()
                };
                self.draw_text_centered(buf, &heading, 24.0, cx, 34, GRAY);
                let text_cx = cx.saturating_sub(30);
                self.draw_wrapped_text_fit_centered(
                    buf,
                    &item.question,
                    48.0,
                    20.0,
                    text_cx,
                    cy,
                    (self.width as f32 * 0.74) as i32,
                    (self.height as f32 * 0.68) as i32,
                    WHITE,
                );
                self.draw_timer_pie(
                    buf,
                    timer_cx,
                    timer_cy,
                    timer_radius,
                    remaining,
                    timer_color,
                );
                self.draw_text_centered(
                    buf,
                    &seconds_left.max(0).to_string(),
                    18.0,
                    timer_cx,
                    timer_cy,
                    WHITE,
                );

                self.draw_text_centered(
                    buf,
                    "Enter/Space for answer",
                    18.0,
                    cx,
                    self.height - 24,
                    GRAY,
                );
            }
            AppState::Answer {
                request,
                items,
                current_idx,
            } => {
                let item = &items[*current_idx];
                let heading = if request.subject == crate::app::TriviaSubject::RecentNews {
                    format!("ANSWER - {}", request.menu_title())
                } else {
                    "ANSWER".to_string()
                };
                self.draw_text_centered(buf, &heading, 24.0, cx, 34, GRAY);
                self.draw_wrapped_text_fit_centered(
                    buf,
                    &item.answer,
                    52.0,
                    22.0,
                    cx,
                    cy,
                    (self.width as f32 * 0.90) as i32,
                    (self.height as f32 * 0.68) as i32,
                    YELLOW,
                );

                let hint = if *current_idx + 1 < items.len() {
                    "Press Enter/Space for next question"
                } else {
                    "End of round. Press Enter/Space to reload"
                };
                self.draw_text_centered(buf, hint, 18.0, cx, self.height - 24, GRAY);
            }
        }
    }

    fn draw_timer_pie(
        &self,
        buf: &mut Vec<u32>,
        cx: usize,
        cy: usize,
        radius: usize,
        fraction: f32,
        fill_color: u32,
    ) {
        let r_sq = (radius * radius) as i32;
        let inner_sq = ((radius.saturating_sub(2)) * (radius.saturating_sub(2))) as i32;
        let start_angle = -std::f32::consts::FRAC_PI_2;
        let sweep = fraction.clamp(0.0, 1.0) * 2.0 * std::f32::consts::PI;

        for dy in -(radius as i32)..=(radius as i32) {
            for dx in -(radius as i32)..=(radius as i32) {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > r_sq {
                    continue;
                }

                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px < 0 || py < 0 || (px as usize) >= self.width || (py as usize) >= self.height {
                    continue;
                }

                let angle = (dy as f32).atan2(dx as f32);
                let mut norm_angle = angle - start_angle;
                while norm_angle < 0.0 {
                    norm_angle += 2.0 * std::f32::consts::PI;
                }
                while norm_angle >= 2.0 * std::f32::consts::PI {
                    norm_angle -= 2.0 * std::f32::consts::PI;
                }

                let idx = (py as usize) * self.width + (px as usize);
                if dist_sq >= inner_sq {
                    buf[idx] = WHITE;
                } else if norm_angle <= sweep {
                    buf[idx] = fill_color;
                } else {
                    buf[idx] = GRAY;
                }
            }
        }
    }

    fn heat_color(remaining: f32) -> u32 {
        let p = remaining.clamp(0.0, 1.0);
        if p > 0.5 {
            let t = (p - 0.5) / 0.5;
            Self::lerp_color(YELLOW, GREEN, t)
        } else {
            let t = p / 0.5;
            Self::lerp_color(RED, YELLOW, t)
        }
    }

    fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
        let clamped = t.clamp(0.0, 1.0);
        let ar = ((a >> 16) & 0xFF) as f32;
        let ag = ((a >> 8) & 0xFF) as f32;
        let ab = (a & 0xFF) as f32;
        let br = ((b >> 16) & 0xFF) as f32;
        let bg = ((b >> 8) & 0xFF) as f32;
        let bb = (b & 0xFF) as f32;

        let r = (ar + (br - ar) * clamped).round() as u32;
        let g = (ag + (bg - ag) * clamped).round() as u32;
        let b = (ab + (bb - ab) * clamped).round() as u32;
        (r << 16) | (g << 8) | b
    }

    fn draw_text_centered(
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

        let glyphs = layout.glyphs();
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

        for glyph in glyphs {
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
                    if px >= 0
                        && py >= 0
                        && (px as usize) < self.width
                        && (py as usize) < self.height
                    {
                        let idx = (py as usize) * self.width + (px as usize);
                        let r = ((color >> 16) & 0xFF) * alpha / 255;
                        let g = ((color >> 8) & 0xFF) * alpha / 255;
                        let b = (color & 0xFF) * alpha / 255;
                        buf[idx] = (r << 16) | (g << 8) | b;
                    }
                }
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

    fn draw_rect_outline(
        &self,
        buf: &mut Vec<u32>,
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

    fn draw_multiline_text_centered(
        &self,
        buf: &mut Vec<u32>,
        text: &str,
        size: f32,
        cx: usize,
        cy: usize,
        color: u32,
    ) {
        let max_w = (self.width as f32 * 0.8) as f32;
        let mut layout: Layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            max_width: Some(max_w),
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Middle,
            ..Default::default()
        });
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return;
        }

        let draw_x = cx as i32 - (max_w / 2.0) as i32;
        let draw_y = cy as i32;

        for glyph in glyphs {
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
                    if px >= 0
                        && py >= 0
                        && (px as usize) < self.width
                        && (py as usize) < self.height
                    {
                        let idx = (py as usize) * self.width + (px as usize);
                        let r = ((color >> 16) & 0xFF) * alpha / 255;
                        let g = ((color >> 8) & 0xFF) * alpha / 255;
                        let b = (color & 0xFF) * alpha / 255;
                        buf[idx] = (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
    }

    fn draw_wrapped_text_fit_centered(
        &self,
        buf: &mut Vec<u32>,
        text: &str,
        preferred_size: f32,
        min_size: f32,
        cx: usize,
        cy: usize,
        max_width: i32,
        max_height: i32,
        color: u32,
    ) {
        if max_width <= 0 || max_height <= 0 {
            return;
        }

        let mut chosen = min_size;
        let mut size = preferred_size;
        while size >= min_size {
            let (_, total_h) = self.measure_wrapped_text(text, size, max_width as f32);
            if total_h <= max_height {
                chosen = size;
                break;
            }
            size -= 2.0;
        }

        self.draw_multiline_text_with_max(buf, text, chosen, cx, cy, color, max_width as f32);
    }

    fn measure_wrapped_text(&self, text: &str, size: f32, max_width: f32) -> (i32, i32) {
        let mut layout: Layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            max_width: Some(max_width),
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Top,
            ..Default::default()
        });
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return (0, 0);
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

        (max_x - min_x, max_y - min_y)
    }

    fn draw_multiline_text_with_max(
        &self,
        buf: &mut Vec<u32>,
        text: &str,
        size: f32,
        cx: usize,
        cy: usize,
        color: u32,
        max_w: f32,
    ) {
        let mut layout: Layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: 0.0,
            y: 0.0,
            max_width: Some(max_w),
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Top,
            ..Default::default()
        });
        layout.append(&[&self.font], &TextStyle::new(text, size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return;
        }

        let min_y = glyphs.iter().map(|g| g.y as i32).min().unwrap_or(0);
        let max_y = glyphs
            .iter()
            .map(|g| (g.y + g.height as f32) as i32)
            .max()
            .unwrap_or(0);
        let total_h = max_y - min_y;

        let draw_x = cx as i32 - (max_w / 2.0) as i32;
        let draw_y = cy as i32 - total_h / 2 - min_y;

        for glyph in glyphs {
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
                    if px >= 0
                        && py >= 0
                        && (px as usize) < self.width
                        && (py as usize) < self.height
                    {
                        let idx = (py as usize) * self.width + (px as usize);
                        let r = ((color >> 16) & 0xFF) * alpha / 255;
                        let g = ((color >> 8) & 0xFF) * alpha / 255;
                        let b = (color & 0xFF) * alpha / 255;
                        buf[idx] = (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
    }

    fn draw_spinner(&self, buf: &mut Vec<u32>, cx: usize, cy: usize, elapsed_ms: u64) {
        let radius = 18i32;
        let dot_radius = 4i32;
        let num_dots = 8u32;
        let active = ((elapsed_ms / 120) % num_dots as u64) as u32;

        for i in 0..num_dots {
            let angle = (i as f32 / num_dots as f32) * 2.0 * std::f32::consts::PI
                - std::f32::consts::FRAC_PI_2;
            let dx = (angle.cos() * radius as f32).round() as i32;
            let dy = (angle.sin() * radius as f32).round() as i32;
            let dot_cx = cx as i32 + dx;
            let dot_cy = cy as i32 + dy;

            let dist_from_active = ((i as i32 - active as i32).rem_euclid(num_dots as i32)) as u32;
            let brightness: u32 = if dist_from_active == 0 {
                255
            } else if dist_from_active <= 2 {
                160u32.saturating_sub(dist_from_active * 40)
            } else {
                60
            };
            let color = (brightness << 16) | (brightness << 8) | brightness;

            for dy2 in -dot_radius..=dot_radius {
                for dx2 in -dot_radius..=dot_radius {
                    if dx2 * dx2 + dy2 * dy2 > dot_radius * dot_radius {
                        continue;
                    }
                    let px = dot_cx + dx2;
                    let py = dot_cy + dy2;
                    if px >= 0
                        && py >= 0
                        && (px as usize) < self.width
                        && (py as usize) < self.height
                    {
                        buf[(py as usize) * self.width + (px as usize)] = color;
                    }
                }
            }
        }
    }
}
