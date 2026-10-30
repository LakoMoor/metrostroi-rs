//! Main menu, pause menu, settings and help screens, drawn with [`crate::ui::Ui`].

use crate::ui::{Color, Ui};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Main,
    Playing,
    Pause,
    Settings,
    Help,
    Lines,
    Trip,
}

/// A selectable line (`lines.json` of a generated map set): folder under `content/maps/moscow` and the menu text.
#[derive(Clone, Debug)]
pub struct LineInfo {
    pub id: String,
    pub label: String,
}

/// Lines of the trip statistics screen, refreshed by the app while a menu is open.
static TRIP: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn set_trip(l: Vec<String>) {
    *TRIP.lock().unwrap() = l;
}

static LINES: std::sync::Mutex<Vec<LineInfo>> = std::sync::Mutex::new(Vec::new());

pub fn set_lines(l: Vec<LineInfo>) {
    *LINES.lock().unwrap() = l;
}

pub fn lines() -> Vec<LineInfo> {
    LINES.lock().unwrap().clone()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Vertical field of view, degrees.
    pub fov: f32,
    /// Mouse look multiplier.
    pub mouse: f32,
    pub hud: bool,
    /// Reserved for the sound phase.
    pub volume: f32,
    /// Map folder chosen in the line menu (relative to the game folder); empty = the configured default.
    pub map: String,
    /// 0 = standard, 1 = high (4x MSAA), 2 = ultra (4x MSAA + the heavier effects as they land).
    pub quality: u8,
    /// Ultra only: fraction of the window resolution the scene is drawn at (upscaled with a light sharpen).
    pub scale: f32,
    /// Wet rails: the wheel-rail adhesion drops to about a third, so wheels spin and slide.
    pub wet: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { fov: 65.0, mouse: 1.0, hud: true, volume: 0.8, map: String::new(), quality: 1, scale: 1.0, wet: false }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    None,
    /// Leave the menu and drive; `quick` runs the scripted start-up first.
    Start { quick: bool },
    Resume,
    QuickStart,
    SaveCab,
    LoadCab,
    ResetAutostop,
    ResetTrain,
    ToMain,
    Quit,
    /// Restart on the line with this index in the line list.
    PickLine(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum Item {
    Button(&'static str, Action),
    Goto(&'static str, Screen),
    Slider(&'static str, Field),
    Toggle(&'static str, Field),
    Line(usize),
    Back,
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Fov,
    Mouse,
    Volume,
    Hud,
    Quality,
    Scale,
    Rail,
}

pub struct Menu {
    pub screen: Screen,
    pub settings: Settings,
    settings_path: PathBuf,
    selected: usize,
    /// Screen to return to from Settings/Help.
    back_to: Screen,
    /// Item rectangles from the last draw, for the mouse.
    rects: Vec<(f32, f32, f32, f32)>,
    pub dirty: bool,
    lines: Vec<LineInfo>,
}

const KEYS_HELP: &[(&str, &str)] = &[
    ("Мышь", "нажать кнопку/тумблер в кабине; перетаскивание — осмотр"),
    ("F2 / F3", "вид снаружи / из кабины"),
    ("F4", "встать с сиденья / сесть обратно у кресла"),
    ("W A S D · Shift", "ходьба по вагону и снаружи (двери открывает KDL/KDP)"),
    ("Стрелки · PgUp/Dn", "двигать голову в кабине (Home — сброс)"),
    ("Num Enter · Num +", "ключ в КВ · реверсор вперёд"),
    ("4 · 5 · 3", "КВ: 0 · T1 · X3 (1 X1, 2 X2, 6 T1A, 7 T2)"),
    ("Num 1…7 · F · R", "кран машиниста"),
    ("A · D · V", "открыть левые / правые двери · закрыть"),
    ("Пробел", "кнопка бдительности"),
    ("F5 · F6", "быстрый запуск · сброс автостопа АРС"),
    ("F7 · F8 · F9", "случайный отказ · сброс · отказы со временем вкл/выкл"),
    ("Esc", "меню"),
];

impl Menu {
    pub fn new(settings_path: PathBuf, start: Screen) -> Menu {
        Menu { settings: Settings::load(&settings_path), settings_path, screen: start, selected: 0, back_to: Screen::Main, rects: Vec::new(), dirty: false, lines: LINES.lock().unwrap().clone() }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.settings_path.clone()
    }

    fn items(&self) -> Vec<Item> {
        match self.screen {
            Screen::Main => vec![
                Item::Button("Начать (холодный и тёмный)", Action::Start { quick: false }),
                Item::Button("Быстрый старт", Action::Start { quick: true }),
                Item::Goto("Настройки", Screen::Settings),
                Item::Goto("Управление", Screen::Help),
                Item::Button("Выход", Action::Quit),
            ]
            .into_iter()
            .enumerate()
            .flat_map(|(i, it)| if i == 2 && !self.lines.is_empty() { vec![Item::Goto("Выбрать линию", Screen::Lines), it] } else { vec![it] })
            .collect(),
            Screen::Pause => vec![
                Item::Button("Продолжить", Action::Resume),
                Item::Button("Быстрый запуск поезда", Action::QuickStart),
                Item::Button("Сохранить кабину", Action::SaveCab),
                Item::Button("Загрузить кабину", Action::LoadCab),
                Item::Button("Сбросить автостоп АРС", Action::ResetAutostop),
                Item::Button("Поезд в начало", Action::ResetTrain),
                Item::Goto("Статистика рейса", Screen::Trip),
                Item::Goto("Настройки", Screen::Settings),
                Item::Goto("Управление", Screen::Help),
                Item::Button("В главное меню", Action::ToMain),
                Item::Button("Выход", Action::Quit),
            ],
            Screen::Settings => vec![
                Item::Slider("Угол обзора", Field::Fov),
                Item::Slider("Чувствительность мыши", Field::Mouse),
                Item::Slider("Громкость (на будущее)", Field::Volume),
                Item::Slider("Графика", Field::Quality),
                Item::Slider("Масштаб рендера (ультра)", Field::Scale),
                Item::Toggle("Мокрые рельсы", Field::Rail),
                Item::Toggle("Показывать приборы на экране", Field::Hud),
                Item::Back,
            ],
            Screen::Help | Screen::Trip => vec![Item::Back],
            Screen::Lines => (0..self.lines.len()).map(Item::Line).chain(std::iter::once(Item::Back)).collect(),
            Screen::Playing => vec![],
        }
    }

    fn title(&self) -> &'static str {
        match self.screen {
            Screen::Main => "МЕТРОСТРОЙ",
            Screen::Pause => "Пауза",
            Screen::Settings => "Настройки",
            Screen::Help => "Управление",
            Screen::Trip => "Статистика рейса",
            Screen::Lines => "Выбор линии",
            Screen::Playing => "",
        }
    }

    fn value_text(&self, f: Field) -> (String, f32) {
        match f {
            Field::Fov => (format!("{:.0}°", self.settings.fov), (self.settings.fov - 50.0) / 50.0),
            Field::Mouse => (format!("{:.1}×", self.settings.mouse), (self.settings.mouse - 0.3) / 1.7),
            Field::Volume => (format!("{:.0}%", self.settings.volume * 100.0), self.settings.volume),
            Field::Quality => ((["обычная", "высокая", "ультра"][self.settings.quality.min(2) as usize]).to_string(), self.settings.quality as f32 / 2.0),
            Field::Scale => (format!("{:.0}%", self.settings.scale * 100.0), (self.settings.scale - 0.5) / 0.5),
            Field::Rail => (if self.settings.wet { "мокрые".into() } else { "сухие".into() }, 0.0),
            Field::Hud => (if self.settings.hud { "вкл".into() } else { "выкл".into() }, 0.0),
        }
    }

    fn adjust(&mut self, f: Field, dir: f32) {
        match f {
            Field::Fov => self.settings.fov = (self.settings.fov + dir * 5.0).clamp(50.0, 100.0),
            Field::Mouse => self.settings.mouse = ((self.settings.mouse + dir * 0.1) * 10.0).round() / 10.0,
            Field::Volume => self.settings.volume = ((self.settings.volume + dir * 0.1) * 10.0).round() / 10.0,
            Field::Quality => self.settings.quality = (self.settings.quality as i32 + if dir < 0.0 { -1 } else { 1 }).clamp(0, 2) as u8,
            Field::Scale => self.settings.scale = ((self.settings.scale + dir * 0.1) * 10.0).round() / 10.0,
            Field::Rail => self.settings.wet = !self.settings.wet,
            Field::Hud => self.settings.hud = !self.settings.hud,
        }
        self.settings.mouse = self.settings.mouse.clamp(0.3, 2.0);
        self.settings.volume = self.settings.volume.clamp(0.0, 1.0);
        self.settings.scale = self.settings.scale.clamp(0.5, 1.0);
        self.dirty = true;
    }

    fn go(&mut self, s: Screen) {
        if matches!(s, Screen::Settings | Screen::Help | Screen::Lines | Screen::Trip) {
            self.back_to = self.screen;
        }
        self.screen = s;
        self.selected = 0;
    }

    fn back(&mut self) {
        if self.dirty {
            self.settings.save(&self.settings_path);
            self.dirty = false;
        }
        self.screen = self.back_to;
        self.selected = 0;
    }

    fn activate(&mut self, i: usize, dir: f32) -> Action {
        let Some(item) = self.items().get(i).copied() else { return Action::None };
        match item {
            Item::Button(_, a) => {
                if matches!(a, Action::Start { .. } | Action::Resume | Action::QuickStart | Action::SaveCab | Action::LoadCab | Action::ResetAutostop | Action::ResetTrain) {
                    self.screen = Screen::Playing;
                }
                if a == Action::ToMain {
                    self.screen = Screen::Main;
                    self.selected = 0;
                }
                a
            }
            Item::Goto(_, s) => {
                self.go(s);
                Action::None
            }
            Item::Slider(_, f) | Item::Toggle(_, f) => {
                self.adjust(f, if dir == 0.0 { 1.0 } else { dir });
                Action::None
            }
            Item::Line(i) => Action::PickLine(i),
            Item::Back => {
                self.back();
                Action::None
            }
        }
    }

    /// Esc: open the pause menu while driving, go back from sub-screens, resume from the pause menu.
    pub fn escape(&mut self) -> Action {
        match self.screen {
            Screen::Playing => {
                self.screen = Screen::Pause;
                self.selected = 0;
                Action::None
            }
            Screen::Pause => {
                self.screen = Screen::Playing;
                Action::Resume
            }
            Screen::Settings | Screen::Help | Screen::Lines | Screen::Trip => {
                self.back();
                Action::None
            }
            Screen::Main => Action::None,
        }
    }

    pub fn key(&mut self, code: KeyCode) -> Action {
        let n = self.items().len();
        match code {
            KeyCode::Escape => return self.escape(),
            KeyCode::ArrowDown if n > 0 => self.selected = (self.selected + 1) % n,
            KeyCode::ArrowUp if n > 0 => self.selected = (self.selected + n - 1) % n,
            KeyCode::ArrowLeft => return self.activate_slider(-1.0),
            KeyCode::ArrowRight => return self.activate_slider(1.0),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => return self.activate(self.selected, 1.0),
            _ => {}
        }
        Action::None
    }

    fn activate_slider(&mut self, dir: f32) -> Action {
        if let Some(Item::Slider(..) | Item::Toggle(..)) = self.items().get(self.selected) {
            return self.activate(self.selected, dir);
        }
        Action::None
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.rects.iter().position(|&(rx, ry, rw, rh)| x >= rx && x <= rx + rw && y >= ry && y <= ry + rh)
    }

    pub fn hover(&mut self, x: f32, y: f32) {
        if let Some(i) = self.hit(x, y) {
            self.selected = i;
        }
    }

    pub fn click(&mut self, x: f32, y: f32) -> Action {
        let Some(i) = self.hit(x, y) else { return Action::None };
        self.selected = i;
        // sliders: the left third lowers, the rest raises
        let (rx, _, rw, _) = self.rects[i];
        let dir = if x < rx + rw / 3.0 { -1.0 } else { 1.0 };
        self.activate(i, dir)
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        let (sw, sh) = ui.screen;
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.02, 0.06, if self.screen == Screen::Main { 0.55 } else { 0.7 }]);
        let title = self.title();
        let tp = 64.0;
        let tw = ui.text_width(title, tp);
        ui.text((sw - tw) / 2.0, sh * 0.12, tp, title, [1.0, 1.0, 1.0, 1.0]);
        if self.screen == Screen::Main {
            let s = "симулятор метро · 81-717 · холодный и тёмный";
            let w = ui.text_width(s, 22.0);
            ui.text((sw - w) / 2.0, sh * 0.12 + 76.0, 22.0, s, [0.7, 0.8, 1.0, 1.0]);
        }
        let items = self.items();
        let (iw, ih, gap) = (560.0f32, 52.0f32, 12.0f32);
        let top = if self.screen == Screen::Help {
            sh * 0.12 + 96.0 + KEYS_HELP.len() as f32 * 34.0
        } else if self.screen == Screen::Trip {
            sh * 0.12 + 96.0 + 10.0 * 34.0
        } else {
            sh * 0.30
        };
        self.rects.clear();
        // long lists (the line menu) show a window of rows that follows the selection
        let rows = (((sh - top - 70.0) / (ih + gap)).floor() as usize).max(3);
        let first = if self.screen == Screen::Lines { self.selected.saturating_sub(rows - 1).min(items.len().saturating_sub(rows)) } else { 0 };
        for (i, item) in items.iter().enumerate() {
            if i < first || i >= first + rows {
                self.rects.push((-1000.0, -1000.0, 0.0, 0.0));
                continue;
            }
            let (x, y) = ((sw - iw) / 2.0, top + (i - first) as f32 * (ih + gap));
            self.rects.push((x, y, iw, ih));
            let sel = i == self.selected;
            let bg: Color = if sel { [0.16, 0.42, 0.85, 0.95] } else { [0.1, 0.16, 0.28, 0.85] };
            ui.rect(x, y, iw, ih, bg);
            if sel {
                ui.rect(x, y, 6.0, ih, [1.0, 0.85, 0.3, 1.0]);
            }
            let label: &str = match *item {
                Item::Button(l, _) | Item::Goto(l, _) | Item::Slider(l, _) | Item::Toggle(l, _) => l,
                Item::Line(n) => &self.lines[n].label,
                Item::Back => "Назад",
            };
            ui.text(x + 24.0, y + 11.0, if matches!(item, Item::Line(_)) { 19.0 } else { 28.0 }, label, [1.0, 1.0, 1.0, 1.0]);
            if let Item::Slider(_, f) | Item::Toggle(_, f) = *item {
                let (txt, frac) = self.value_text(f);
                let w = ui.text_width(&txt, 28.0);
                ui.text(x + iw - 24.0 - w, y + 11.0, 28.0, &txt, [1.0, 0.9, 0.5, 1.0]);
                if matches!(item, Item::Slider(..)) {
                    ui.rect(x + 24.0, y + ih - 9.0, iw - 48.0, 3.0, [1.0, 1.0, 1.0, 0.25]);
                    ui.rect(x + 24.0, y + ih - 9.0, (iw - 48.0) * frac.clamp(0.0, 1.0), 3.0, [1.0, 0.85, 0.3, 1.0]);
                }
            }
        }
        if self.screen == Screen::Help {
            for (i, (k, d)) in KEYS_HELP.iter().enumerate() {
                let y = sh * 0.12 + 96.0 + i as f32 * 34.0;
                ui.text(sw / 2.0 - 400.0, y, 24.0, k, [1.0, 0.9, 0.5, 1.0]);
                ui.text(sw / 2.0 - 130.0, y, 24.0, d, [0.9, 0.93, 1.0, 1.0]);
            }
        }
        if self.screen == Screen::Trip {
            for (i, l) in TRIP.lock().unwrap().iter().enumerate() {
                ui.text(sw / 2.0 - 400.0, sh * 0.12 + 96.0 + i as f32 * 34.0, 24.0, l, if i == 0 { [1.0, 0.9, 0.5, 1.0] } else { [0.9, 0.93, 1.0, 1.0] });
            }
        }
        let hint = "↑↓ выбор · Enter · ←→ значение · Esc назад";
        let hw = ui.text_width(hint, 18.0);
        ui.text((sw - hw) / 2.0, sh - 40.0, 18.0, hint, [0.6, 0.7, 0.85, 1.0]);
    }
}
