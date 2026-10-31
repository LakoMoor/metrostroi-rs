//! The Moscow metro map screen: every generated line drawn from its real coordinates, with station picking.

use crate::ui::{Color, Ui};
use std::sync::{Arc, Mutex};

pub struct MapStation {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub depth: f32,
    /// Metres along the route.
    pub s: f32,
}

/// One line (one direction of it is enough to draw it; both are kept to start either way).
pub struct MapLine {
    /// Index into `menu::lines()` of the route drawn and used for "forward".
    pub route: usize,
    /// The opposite direction, if it exists.
    pub reverse: Option<usize>,
    pub reference: String,
    pub name: String,
    pub colour: [f32; 3],
    pub poly: Vec<[f32; 2]>,
    pub from: String,
    pub to: String,
    pub length_km: f32,
    pub stations: Vec<MapStation>,
    /// Stations of the reverse route by name -> (route index, s).
    pub reverse_stops: Vec<(String, f32)>,
}

#[derive(Default)]
pub struct MapData {
    pub lines: Vec<MapLine>,
}

static DATA: Mutex<Option<Arc<MapData>>> = Mutex::new(None);

pub fn set_data(d: MapData) {
    *DATA.lock().unwrap() = Some(Arc::new(d));
}

pub fn data() -> Option<Arc<MapData>> {
    DATA.lock().unwrap().clone()
}

/// What the player picked on the map.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Pick {
    pub route: usize,
    pub s: f32,
}

#[derive(Default)]
pub struct MapState {
    /// (line index, station index) under the cursor.
    pub hover: Option<(usize, usize)>,
    pub selected: Option<(usize, usize)>,
    /// Line highlighted from the legend.
    pub line: Option<usize>,
    /// Zoom around the map centre (1 = whole city) and pan in metres.
    pub zoom: f32,
    pub pan: (f32, f32),
    hits: Vec<(f32, f32, usize, usize)>,
    legend: Vec<(f32, f32, f32, f32, usize)>,
    buttons: Vec<(f32, f32, f32, f32, Pick)>,
    back: (f32, f32, f32, f32),
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

impl MapState {
    pub fn new() -> MapState {
        MapState { zoom: 1.0, ..Default::default() }
    }

    fn view(&self, d: &MapData, area: (f32, f32, f32, f32)) -> ((f32, f32), f32) {
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for l in &d.lines {
            for p in &l.poly {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
        }
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let fit = ((area.2 - 60.0) / (x1 - x0).max(1.0)).min((area.3 - 60.0) / (y1 - y0).max(1.0));
        ((cx - self.pan.0, cy - self.pan.1), fit * self.zoom.max(0.5))
    }

    pub fn wheel(&mut self, dy: f32) {
        self.zoom = (self.zoom * if dy > 0.0 { 1.2 } else { 1.0 / 1.2 }).clamp(1.0, 12.0);
        if self.zoom <= 1.01 {
            self.pan = (0.0, 0.0);
        }
    }

    pub fn drag(&mut self, dx: f32, dy: f32, scale_m_per_px: f32) {
        if self.zoom > 1.01 {
            self.pan.0 -= dx * scale_m_per_px;
            self.pan.1 += dy * scale_m_per_px;
        }
    }

    pub fn meters_per_px(&self, sw: f32, sh: f32) -> f32 {
        let Some(d) = data() else { return 1.0 };
        1.0 / self.view(&d, (0.0, 0.0, sw - SIDE, sh)).1
    }

    pub fn hover_at(&mut self, x: f32, y: f32) {
        self.hover = self.hits.iter().filter(|h| (h.0 - x).hypot(h.1 - y) < 11.0).min_by(|a, b| (a.0 - x).hypot(a.1 - y).total_cmp(&(b.0 - x).hypot(b.1 - y))).map(|h| (h.2, h.3));
    }

    /// Returns a start request when a "go" button was hit; `Some(None)` for back.
    pub fn click(&mut self, x: f32, y: f32) -> Option<Option<Pick>> {
        let inside = |r: (f32, f32, f32, f32)| x >= r.0 && x <= r.0 + r.2 && y >= r.1 && y <= r.1 + r.3;
        if inside(self.back) {
            return Some(None);
        }
        for b in &self.buttons {
            if inside((b.0, b.1, b.2, b.3)) {
                return Some(Some(b.4));
            }
        }
        for l in &self.legend {
            if inside((l.0, l.1, l.2, l.3)) {
                self.line = if self.line == Some(l.4) { None } else { Some(l.4) };
                self.selected = None;
                return None;
            }
        }
        if let Some(h) = self.hits.iter().filter(|h| (h.0 - x).hypot(h.1 - y) < 11.0).min_by(|a, b| (a.0 - x).hypot(a.1 - y).total_cmp(&(b.0 - x).hypot(b.1 - y))) {
            self.selected = Some((h.2, h.3));
            self.line = Some(h.2);
        } else if x < 0.0 {
            self.selected = None;
        }
        None
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        let (sw, sh) = ui.screen;
        let Some(d) = data() else {
            ui.text(60.0, 60.0, 28.0, "Нет данных карты (game/content/maps/moscow/lines.json)", [1.0, 0.6, 0.5, 1.0]);
            return;
        };
        let k = (sh / 900.0).clamp(0.7, 1.6);
        // backdrop
        ui.grad(0.0, 0.0, sw, sh, [0.004, 0.007, 0.016, 1.0], [0.002, 0.004, 0.009, 1.0]);
        let area = (0.0, 0.0, sw - SIDE * k, sh);
        let (centre, scale) = {
            let v = self.view(&d, (area.0, area.1, area.2, area.3));
            (v.0, v.1)
        };
        let to_screen = |x: f32, y: f32| (area.2 / 2.0 + (x - centre.0) * scale, sh / 2.0 - (y - centre.1) * scale);
        // faint grid, 5 km
        let mut gx = -40_000.0f32;
        while gx < 40_000.0 {
            let (sx, _) = to_screen(gx, 0.0);
            if sx > 0.0 && sx < area.2 {
                ui.rect(sx, 0.0, 1.0, sh, [1.0, 1.0, 1.0, 0.025]);
            }
            let (_, sy) = to_screen(0.0, gx);
            if sy > 0.0 && sy < sh {
                ui.rect(0.0, sy, area.2, 1.0, [1.0, 1.0, 1.0, 0.025]);
            }
            gx += 5_000.0;
        }
        let focus = self.line;
        // lines
        for (li, l) in d.lines.iter().enumerate() {
            let dim = focus.is_some() && focus != Some(li);
            let c = l.colour;
            let col: Color = if dim { [c[0], c[1], c[2], 0.22] } else { [c[0], c[1], c[2], 1.0] };
            let w = if focus == Some(li) { 8.0 * k } else { 5.0 * k };
            for p in l.poly.windows(2) {
                let (x0, y0) = to_screen(p[0][0], p[0][1]);
                let (x1, y1) = to_screen(p[1][0], p[1][1]);
                if (x0 < -50.0 && x1 < -50.0) || (x0 > area.2 + 50.0 && x1 > area.2 + 50.0) {
                    continue;
                }
                ui.line(x0, y0, x1, y1, w, col);
            }
        }
        // stations; interchanges (the same name on several lines) get a white ring
        self.hits.clear();
        let mut count = std::collections::HashMap::<&str, usize>::new();
        for l in &d.lines {
            for s in &l.stations {
                *count.entry(s.name.as_str()).or_default() += 1;
            }
        }
        for (li, l) in d.lines.iter().enumerate() {
            let dim = focus.is_some() && focus != Some(li);
            for (si, s) in l.stations.iter().enumerate() {
                let (sx, sy) = to_screen(s.x, s.y);
                if sx < -20.0 || sx > area.2 + 20.0 || sy < -20.0 || sy > sh + 20.0 {
                    continue;
                }
                let c = l.colour;
                let a = if dim { 0.3 } else { 1.0 };
                let hot = self.hover == Some((li, si)) || self.selected == Some((li, si));
                if count[s.name.as_str()] > 1 {
                    ui.circle(sx, sy, (6.5 + if hot { 2.0 } else { 0.0 }) * k, [0.95, 0.96, 1.0, a]);
                    ui.circle(sx, sy, (3.6 + if hot { 2.0 } else { 0.0 }) * k, [0.04, 0.05, 0.09, a]);
                } else {
                    ui.circle(sx, sy, (4.2 + if hot { 3.0 } else { 0.0 }) * k, [0.04, 0.05, 0.09, a]);
                    ui.circle(sx, sy, (2.8 + if hot { 3.0 } else { 0.0 }) * k, [c[0] * 0.5 + 0.5, c[1] * 0.5 + 0.5, c[2] * 0.5 + 0.5, a]);
                }
                if !dim {
                    self.hits.push((sx, sy, li, si));
                }
                // names: the focused line, the hovered or selected station, or everything when zoomed in
                if hot || focus == Some(li) && self.zoom > 1.6 || self.zoom > 4.0 {
                    let t = &s.name;
                    let tw = ui.text_width(t, 15.0 * k);
                    ui.round_rect(sx + 9.0 * k - 4.0, sy - 10.0 * k, tw + 8.0, 20.0 * k, 5.0, [0.02, 0.03, 0.06, 0.8]);
                    ui.text(sx + 9.0 * k, sy - 9.0 * k, 15.0 * k, t, [1.0, 1.0, 1.0, 1.0]);
                }
            }
        }
        // side panel
        let pw = SIDE * k;
        let px = sw - pw;
        ui.grad(px, 0.0, pw, sh, [0.012, 0.018, 0.04, 0.97], [0.007, 0.011, 0.026, 0.97]);
        ui.rect(px, 0.0, 2.0, sh, [0.35, 0.6, 1.0, 0.5]);
        ui.text(px + 24.0 * k, 22.0 * k, 34.0 * k, "Карта метро", [1.0, 1.0, 1.0, 1.0]);
        ui.text(px + 24.0 * k, 62.0 * k, 16.0 * k, "колесо — масштаб · мышь — сдвиг · клик — станция", [0.55, 0.65, 0.85, 1.0]);
        // legend
        self.legend.clear();
        let mut y = 96.0 * k;
        for (li, l) in d.lines.iter().enumerate() {
            let on = focus == Some(li);
            let (rx, ry, rw, rh) = (px + 16.0 * k, y, pw - 32.0 * k, 25.0 * k);
            if on {
                ui.round_rect(rx, ry, rw, rh, 7.0, [1.0, 1.0, 1.0, 0.1]);
            }
            let c = l.colour;
            ui.round_rect(rx + 6.0, ry + 3.0 * k, 34.0 * k, rh - 6.0 * k, 7.0, [c[0], c[1], c[2], 1.0]);
            let lw = ui.text_width(&l.reference, 15.0 * k);
            ui.text(rx + 6.0 + (34.0 * k - lw) / 2.0, ry + 3.0 * k, 15.0 * k, &l.reference, [1.0, 1.0, 1.0, 1.0]);
            let label = l.name.split(':').next().unwrap_or("").trim().to_string();
            ui.text(rx + 50.0 * k, ry + 3.0 * k, 16.0 * k, &label, if on { [1.0, 1.0, 1.0, 1.0] } else { [0.78, 0.84, 0.95, 1.0] });
            self.legend.push((rx, ry, rw, rh, li));
            y += 27.0 * k;
        }
        // selected station
        self.buttons.clear();
        let card_y = y + 14.0 * k;
        let sel = self.selected.or(self.hover);
        if let Some((li, si)) = sel {
            let l = &d.lines[li];
            let s = &l.stations[si];
            let ch = (sh - card_y - 70.0 * k).max(150.0 * k);
            ui.round_rect_grad(px + 16.0 * k, card_y, pw - 32.0 * k, ch, 12.0, [0.03, 0.055, 0.12, 0.95], [0.018, 0.03, 0.075, 0.95]);
            let c = l.colour;
            ui.round_rect(px + 16.0 * k, card_y, 8.0 * k, ch, 4.0, [c[0], c[1], c[2], 1.0]);
            ui.text(px + 38.0 * k, card_y + 10.0 * k, 28.0 * k, &s.name, [1.0, 1.0, 1.0, 1.0]);
            let sub = format!("{} · станция {} из {}", l.name.split(':').next().unwrap_or("").trim(), si + 1, l.stations.len());
            ui.text(px + 38.0 * k, card_y + 44.0 * k, 15.0 * k, &sub, [0.65, 0.75, 0.95, 1.0]);
            let depth = if s.depth > 0.5 { format!("глубина {:.0} м", s.depth) } else if s.depth < -0.5 { format!("эстакада +{:.0} м", -s.depth) } else { "наземная".to_string() };
            ui.text(px + 38.0 * k, card_y + 68.0 * k, 17.0 * k, &format!("{depth} · {:.1} км от начала", s.s / 1000.0), [1.0, 0.9, 0.55, 1.0]);
            // go buttons: forward, and backward when the opposite route has this station
            let mut by = card_y + 104.0 * k;
            let fwd = Pick { route: l.route, s: s.s };
            let rev = l.reverse.and_then(|r| l.reverse_stops.iter().find(|(n, _)| *n == s.name).map(|(_, rs)| Pick { route: r, s: *rs }));
            let mut btn = |ui: &mut Ui, label: String, pick: Pick, by: f32, buttons: &mut Vec<(f32, f32, f32, f32, Pick)>| {
                let (bx, bw, bh) = (px + 38.0 * k, pw - 76.0 * k, 44.0 * k);
                ui.round_rect_grad(bx, by, bw, bh, 10.0, [0.06, 0.25, 0.9, 1.0], [0.03, 0.14, 0.65, 1.0]);
                let tw = ui.text_width(&label, 19.0 * k);
                ui.text(bx + (bw - tw) / 2.0, by + 10.0 * k, 19.0 * k, &label, [1.0, 1.0, 1.0, 1.0]);
                buttons.push((bx, by, bw, bh, pick));
            };
            btn(ui, format!("Поехать → {}", l.to), fwd, by, &mut self.buttons);
            by += 54.0 * k;
            if let Some(r) = rev {
                btn(ui, format!("Поехать ← {}", l.from), r, by, &mut self.buttons);
            }
        } else {
            ui.text(px + 24.0 * k, card_y + 6.0 * k, 18.0 * k, "Выберите станцию на карте, чтобы начать", [0.7, 0.78, 0.92, 1.0]);
            ui.text(px + 24.0 * k, card_y + 32.0 * k, 18.0 * k, "рейс с неё в любом направлении.", [0.7, 0.78, 0.92, 1.0]);
        }
        // back button
        let (bx, by, bw, bh) = (px + 16.0 * k, sh - 56.0 * k, pw - 32.0 * k, 40.0 * k);
        ui.round_rect(bx, by, bw, bh, 10.0, mix([0.03, 0.045, 0.09, 1.0], [0.3, 0.4, 0.6, 1.0], 0.0));
        let tw = ui.text_width("Назад · Esc", 18.0 * k);
        ui.text(bx + (bw - tw) / 2.0, by + 9.0 * k, 18.0 * k, "Назад · Esc", [0.9, 0.94, 1.0, 1.0]);
        self.back = (bx, by, bw, bh);
    }
}

const SIDE: f32 = 430.0;
