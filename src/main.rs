//! Порт лобби (hotel_receptionentrance, hotel_1) и Waybackyard (wayback_*)
//! из GameMaker-проекта whatsshakinbacon на Rust + macroquad.

mod assets;
mod audio;
mod enemy;
mod hud;
mod menu;
mod player;
mod save;
mod world;

use assets::Assets;
use audio::Audio;
use hud::{Ctx, Hud};
use menu::{MenuInput, MenuRequest, Menus, OptionsMenu, Pause, PauseAction};
use save::{Options, Slot};
use enemy::{interact, line_blocked, update_darts, Dart, EState, Enemy, EnemyFx, Hit, Kind};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, UniformDesc, UniformType};
use macroquad::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use player::{Input, Player, State};
use world::{inst_bbox, Room};

const ASSETS_DIR: &str = "assets";
const START_ROOM: &str = "hotel_receptionentrance";
const STEP: f32 = 1.0 / 60.0; // room_speed = 60
const FADE_FRAMES: i32 = 20;

fn window_conf() -> Conf {
    Conf {
        window_title: "Fidelity Hotel - Rust port (lobby + Waybackyard)".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Menu,
    Play,
}

/// Куда и как поместить игрока в следующей комнате (obj_player/Other_4)
#[derive(Clone)]
struct Spawn {
    room: String,
    door: String,
    /// 0 - обычная дверь, 1 - горизонтальный холлвей, 2 - вертикальный
    hallway: u8,
    dir: f32,
    saved_pos: f32,
    saved_spd: f32,
    hallway_size: f32,
    from_climb: bool,
    /// точная позиция игрока (возврат к воротам после уровня), вместо двери
    at: Option<(f32, f32)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Playing,
    FadeOut(i32),
    FadeIn(i32),
}

/// Частица: обломок (цветной квадрат), искра или облачко пыли (спрайт spr_cloudeffect)
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    rot: f32,
    rotv: f32,
    life: f32,
    fade: f32,
    color: Color,
    puff: bool,
    frame: f32,
    gravity: f32,
}

/// Спрайтовая частица: обломки и трупы врагов, эффекты ударов, обломки клетки.
/// anim > 0 - проиграть кадры один раз и исчезнуть; anim < 0 - зацикленная анимация.
struct SpritePart {
    spr: String,
    frame: f32,
    anim: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    gravity: f32,
    angle: f32,
    spin: f32,
    scale: f32,
    life: f32,
}

impl SpritePart {
    fn new(spr: &str, x: f32, y: f32) -> SpritePart {
        SpritePart { spr: spr.to_string(), frame: 0.0, anim: 0.0, x, y, vx: 0.0, vy: 0.0, gravity: 0.0, angle: 0.0, spin: 0.0, scale: 1.0, life: 60.0 }
    }
}

/// Всплывающий текст очков ("+1000", "-100")
struct Popup {
    text: String,
    x: f32,
    y: f32,
    life: i32,
    color: Color,
}

/// Спасённый топпин: следует за игроком (par_potable)
struct Toppin {
    kind: String,
    x: f32,
    y: f32,
    xs: f32,
    index: f32,
    intro: bool,
    moving: bool,
}

/// global.escape: pizza time
struct Escape {
    active: bool,
    timer: i32,
    total: i32,
}

/// obj_slipperybutter: масло, на котором скользит игрок
struct Butter {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    grounded: bool,
    frame: f32,
}

/// obj_titlecard: титульная карта уровня (затемнение, карта с названием, шторка-"дырка", загрузка комнаты)
struct TitleCard {
    card: &'static str,
    scene: i32,
    fade: f32,
    fade_in: bool,
    name_alpha: f32,
    timer: i32,
    circle: f32,
    circle_speed: f32,
    spawn: Spawn,
}

fn approach_f(start: f32, end: f32, inc: f32) -> f32 {
    if start < end { (start + inc).min(end) } else { (start - inc).max(end) }
}

enum Layer {
    Bg(usize),
    Tiles(usize),
    Decor(usize),
    Pickups,
    Enemies,
    Toppins,
    Books,
    Spawners,
    Player,
}

struct Game {
    assets: Assets,
    room: Room,
    player: Player,
    cam: Vec2,
    phase: Phase,
    pending: Option<Spawn>,
    collect: i32,
    toast: Option<(String, i32)>,
    tick: i64,
    layers: Vec<(i32, Layer)>,
    latch: Latch,
    next_uid: u64,
    pub debug: bool,
    no_tex: bool,
    palette: i32,
    hud: Hud,
    mode: Mode,
    menus: Menus,
    pause: Pause,
    opt_menu: Option<OptionsMenu>,
    opts: Options,
    slot: usize,
    slot_data: Slot,
    wave: Option<Material>,
    escape_mat: Option<Material>,
    add_mat: Option<Material>,
    /// текстура для эффекта побега: (цель, ширина, высота)
    rt: RefCell<Option<(RenderTarget, u32, u32)>>,
    /// F5: перевернуть изображение эффекта побега (аварийный вариант для других версий macroquad).
    /// В macroquad 0.4.16 рендер-таргет с положительным zoom.y уже хранится правильно,
    /// поэтому по умолчанию flip_y выключен.
    rt_flip: bool,
    bleed: Cell<f32>,
    // --- Rock Bottom ---
    butters: Vec<Butter>,
    has_paint: bool,
    painting_count: i32,
    painting_max: i32,
    can_enter_bus: bool,
    painted: HashSet<(String, usize)>,
    taken_misc: HashSet<String>,
    det_alive: Vec<bool>,
    paint_alive: Vec<bool>,
    heart_alive: Vec<bool>,
    hearts_gone: HashSet<(String, usize)>,
    heart_cd: i32,
    detective: bool,
    titlecard: Option<TitleCard>,
    enter_is_gate: bool,
    audio: Audio,
    collect_cd: i32,
    beer_cd: i32,
    particles: Vec<Particle>,
    rng: u32,
    enemies: Vec<Enemy>,
    darts: Vec<Dart>,
    sprite_parts: Vec<SpritePart>,
    popups: Vec<Popup>,
    toppins: Vec<Toppin>,
    history: Vec<(f32, f32)>,
    hitstop: i32,
    flash: i32,
    escape: Escape,
    /// убитые враги: (комната, номер точки появления, escape-враг?)
    killed: HashSet<(String, usize, bool)>,
    /// собранные коллекты: (комната, номер, escape-коллект?)
    taken: HashSet<(String, usize, bool)>,
    /// разбитые клетки: (комната, x, y)
    got_cages: HashSet<(String, i32, i32)>,
    /// туториал пройден: obj_tutorialBlock исчезает
    tutorial_done: bool,
    /// откуда вернуться после уровня: (комната с воротами, x, y игрока)
    return_to: Option<(String, f32, f32)>,
    /// позиция возврата для ближайшего входа в дверь
    enter_at: Option<(f32, f32)>,
    dialog: String,
    dialog_t: f32,
    dialog_on: bool,
}

/// Нажатия за кадр отрисовки: шаг логики идёт на 60 Гц, а ввод читается на каждый кадр -
/// защёлка не даёт потерять короткое нажатие
#[derive(Default, Clone, Copy)]
struct Latch {
    jump: bool,
    grab: bool,
    down: bool,
    attack: bool,
}

const PAL_VERT: &str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    color = color0 / 255.0;
    uv = texcoord;
}";

/// shd_escape (obj_screensizer): лёгкая волна, разведение красного/синего каналов.
/// Кадр здесь не переворачивается: ориентацию рендер-таргета задаёт flip_y в escape_pass
/// (для macroquad 0.4.16 он должен быть выключен).
const ESCAPE_FRAG: &str = "#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform float u_time;
uniform float u_intensity;
uniform float u_wave_amplitude;
uniform float u_wave_frequency;
void main() {
    vec2 p = uv;
    float distort = u_intensity * u_wave_amplitude;
    p.x += sin(p.y * u_wave_frequency + u_time) * distort;
    p.y += cos(p.x * u_wave_frequency + u_time * 0.7) * distort;
    vec4 c = texture2D(Texture, p);
    vec2 off = vec2(0.01 * u_intensity, 0.0);
    c.r = texture2D(Texture, p + off).r;
    c.b = texture2D(Texture, p - off).b;
    if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) {
        c = vec4(0.0);
    }
    gl_FragColor = c * color;
}";

const PLAIN_FRAG: &str = "#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}";

/// shd_wave: волнистое искажение замороженного кадра на паузе
const WAVE_FRAG: &str = "#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform float time;
uniform float frequency;
uniform float amplitude;
void main() {
    float s = cos((uv.x * 960.0 + time) * frequency) * amplitude;
    gl_FragColor = color * texture2D(Texture, vec2(uv.x, uv.y + s));
}";

thread_local! {
    static FONT: std::cell::RefCell<Option<Font>> = std::cell::RefCell::new(None);
}

/// draw_text со шрифтом DejaVu (в стандартном шрифте macroquad нет кириллицы и стрелок)
fn draw_text_f(text: &str, x: f32, y: f32, size: f32, color: Color) {
    FONT.with(|f| {
        let f = f.borrow();
        draw_text_ex(text, x, y, TextParams { font: f.as_ref(), font_size: size as u16, color, ..Default::default() });
    });
}

fn measure_text_f(text: &str, size: u16) -> TextDimensions {
    FONT.with(|f| measure_text(text, f.borrow().as_ref(), size, 1.0))
}

/// Подсказки Flo: [J]/[D]/[U]... - обозначения кнопок оригинала, заменяем на кнопки порта
fn tidy_text(t: &str) -> String {
    t.replace("[wave]", "")
        .replace("[J]", "Z")
        .replace("[D]", "↓")
        .replace("[U]", "↑")
        .replace("[L]", "←")
        .replace("[R]", "→")
        .replace("[Q]", "Shift")
        .replace("[G]", "X")
}

fn overlap(a: [f32; 4], b: (f32, f32, f32, f32)) -> bool {
    a[0] <= b.2 && a[2] >= b.0 && a[1] <= b.3 && a[3] >= b.1
}

impl Game {
    fn new(mut audio: Audio) -> Game {
        audio.stop_all();
        let assets = Assets::load(ASSETS_DIR);
        let data = assets.room(START_ROOM).expect("нет стартовой комнаты");
        let room = Room::build(&assets, data);
        let mut g = Game {
            assets,
            room,
            player: Player::new(0.0, 0.0),
            cam: vec2(0.0, 0.0),
            phase: Phase::Playing,
            pending: None,
            collect: 0,
            toast: None,
            tick: 0,
            layers: Vec::new(),
            latch: Latch::default(),
            next_uid: 0,
            debug: false,
            no_tex: false,
            palette: 1,
            hud: Hud::new(),
            mode: Mode::Play,
            menus: Menus::new(),
            pause: Pause::new(),
            opt_menu: None,
            opts: Options::load(),
            slot: 0,
            slot_data: Slot::default(),
            wave: None,
            escape_mat: None,
            add_mat: None,
            rt: RefCell::new(None),
            rt_flip: false,
            bleed: Cell::new(0.0),
            butters: Vec::new(),
            has_paint: false,
            painting_count: 0,
            painting_max: 0,
            can_enter_bus: false,
            painted: HashSet::new(),
            taken_misc: HashSet::new(),
            det_alive: Vec::new(),
            paint_alive: Vec::new(),
            heart_alive: Vec::new(),
            hearts_gone: HashSet::new(),
            heart_cd: 0,
            detective: false,
            titlecard: None,
            enter_is_gate: false,
            audio,
            collect_cd: 0,
            beer_cd: 40,
            particles: Vec::new(),
            rng: 0x9E37_79B9,
            enemies: Vec::new(),
            darts: Vec::new(),
            sprite_parts: Vec::new(),
            popups: Vec::new(),
            toppins: Vec::new(),
            history: Vec::new(),
            hitstop: 0,
            flash: 0,
            escape: Escape { active: false, timer: 0, total: 1 },
            killed: HashSet::new(),
            taken: HashSet::new(),
            got_cages: HashSet::new(),
            tutorial_done: false,
            return_to: None,
            enter_at: None,
            dialog: String::new(),
            dialog_t: 0.0,
            dialog_on: false,
        };
        let sp = Spawn {
            room: START_ROOM.into(), door: "A".into(), hallway: 0, dir: 1.0,
            saved_pos: 0.0, saved_spd: 0.0, hallway_size: 1.0, from_climb: false, at: None,
        };
        g.place_player(&sp);
        g.spawn_entities();
        g.rebuild_layers();
        g.update_room_audio();
        g.cam = g.camera_target();
        g
    }

    // ---------------- комнаты и переходы ----------------

    fn rebuild_layers(&mut self) {
        let mut v: Vec<(i32, Layer)> = Vec::new();
        for (i, b) in self.room.data.backgrounds.iter().enumerate() {
            v.push((b.depth, Layer::Bg(i)));
        }
        for (i, t) in self.room.data.tiles.iter().enumerate() {
            v.push((t.depth, Layer::Tiles(i)));
        }
        for (i, d) in self.room.decor.iter().enumerate() {
            v.push((d.depth, Layer::Decor(i)));
        }
        v.push((50, Layer::Pickups)); // obj_collect: depth = 50
        v.push((15, Layer::Toppins)); // par_potable: depth = 15
        v.push((3, Layer::Enemies)); // obj_baddie: depth = 3
        v.push((10, Layer::Books)); // obj_tutorialbook: depth = 10
        v.push((5, Layer::Spawners)); // obj_enemyspawner: depth = 5
        v.push((-50, Layer::Player)); // obj_player: depth = -50
        // больший depth рисуется раньше (дальше от зрителя)
        v.sort_by(|a, b| b.0.cmp(&a.0));
        self.layers = v;
    }

    fn toast(&mut self, text: &str) {
        if self.toast.is_none() {
            self.toast = Some((text.to_string(), 150));
        }
    }

    fn start_transition(&mut self, sp: Spawn) {
        if self.assets.room(&sp.room).is_none() {
            let msg = format!("Комната «{}» в этот порт не входит (только лобби и Waybackyard)", sp.room);
            self.toast(&msg);
            // отменяем вход, чтобы игрок не застрял в состоянии EnterDoor
            if self.player.state == State::EnterDoor {
                self.player.state = State::Normal;
            }
            return;
        }
        self.pending = Some(sp);
        self.phase = Phase::FadeOut(0);
    }

    fn load_pending(&mut self) {
        let Some(sp) = self.pending.take() else { return };
        let Some(data) = self.assets.room(&sp.room) else { return };
        self.room = Room::build(&self.assets, data);
        self.place_player(&sp);
        self.spawn_entities();
        self.rebuild_layers();
        self.update_room_audio();
        // секретные комнаты и сокровище запоминаются в слоте (сердечки на паузе)
        {
            let nm = self.room.data.name.clone();
            let mut dirty = false;
            if nm.contains("secret") {
                dirty |= self.slot_data.secrets.insert(nm.clone());
            }
            if nm.contains("treasure") && !self.slot_data.treasure {
                self.slot_data.treasure = true;
                dirty = true;
            }
            if dirty {
                self.save_slot();
            }
        }
        self.beer_cd = 40;
        self.particles.clear();
        self.sprite_parts.clear();
        self.popups.clear();
        // топпины переходят в новую комнату вместе с игроком
        self.history.clear();
        for t in self.toppins.iter_mut() {
            t.x = self.player.x;
            t.y = self.player.y;
        }
        self.cam = self.camera_target();
    }

    /// obj_music: hotel_* -> музыка хаба, wayback_* -> Waybackyard (секреты - свой трек)
    fn update_room_audio(&mut self) {
        if self.escape.active {
            self.audio.set_music(Some("music_escape"));
            self.audio.set_ambience(None);
            return;
        }
        let n = self.room.data.name.clone();
        if n.starts_with("city") {
            self.audio.set_music(Some(if n.contains("secret") { "music_city_secret" } else { "music_city" }));
            self.audio.set_ambience(None);
        } else if n.starts_with("hotel_") {
            self.audio.set_music(Some("music_hub"));
            self.audio.set_ambience(None);
        } else if n.contains("secret") {
            self.audio.set_music(Some("music_wayback_secret"));
            self.audio.set_ambience(Some("amb_wayback"));
        } else {
            self.audio.set_music(Some("music_wayback"));
            self.audio.set_ambience(Some("amb_wayback"));
        }
    }

    // ---------------- Rock Bottom и титульные карты ----------------

    fn level_name(&self) -> &'static str {
        let n = &self.room.data.name;
        if n.starts_with("tutorial") {
            "tutorial"
        } else if n.starts_with("city") {
            "city"
        } else {
            "forest"
        }
    }

    fn titlecard_for(room: &str) -> Option<&'static str> {
        match room {
            "wayback_1" => Some("spr_titlecardMonolith"),
            "city_1" => Some("spr_titlecardRockBottom"),
            _ => None,
        }
    }

    fn start_titlecard(&mut self, spawn: Spawn, card: &'static str) {
        self.titlecard = Some(TitleCard {
            card,
            scene: 0,
            fade: 0.0,
            fade_in: true,
            name_alpha: 0.0,
            timer: 0,
            circle: 9.0,
            circle_speed: 0.0,
            spawn,
        });
    }

    /// obj_titlecard/Step_0 (сцены 0-3)
    fn step_titlecard(&mut self) {
        let Some(mut t) = self.titlecard.take() else { return };
        let mut finished = false;
        match t.scene {
            0 | 1 => {
                if t.fade_in {
                    t.fade = approach_f(t.fade, 1.0, 0.05);
                    if t.fade >= 1.0 {
                        t.fade_in = false;
                        t.scene = 1;
                        t.timer = 210; // 3.5 секунды
                        self.audio.set_music(None);
                        self.audio.set_ambience(None);
                        self.audio.set_loop(None);
                        if t.card == "spr_titlecardRockBottom" {
                            self.audio.sfx("jingle_city", 1.0);
                        }
                    }
                } else {
                    t.fade = approach_f(t.fade, 0.0, 0.1);
                }
                if t.scene == 1 {
                    t.name_alpha = approach_f(t.name_alpha, 1.0, 0.1);
                    t.timer -= 1;
                    if t.timer <= 0 {
                        t.scene = 2;
                        t.circle = 9.0;
                        t.circle_speed = 0.0;
                        t.fade = 1.0;
                    }
                }
            }
            2 => {
                t.circle = approach_f(t.circle, 0.0, t.circle_speed);
                t.circle_speed += 1.0 / 35.0;
                if t.circle <= 0.0 {
                    t.scene = 3;
                    t.timer = 45;
                }
            }
            _ => {
                t.timer -= 1;
                if t.timer <= 0 {
                    finished = true;
                }
            }
        }
        if finished {
            self.pending = Some(t.spawn.clone());
            self.load_pending();
            self.phase = Phase::FadeIn(0);
        } else {
            self.titlecard = Some(t);
        }
    }

    fn draw_titlecard(&self) {
        let Some(t) = &self.titlecard else { return };
        set_camera(&Camera2D { target: vec2(480.0, 270.0), zoom: vec2(2.0 / 960.0, 2.0 / 540.0), ..Default::default() });
        let a = &self.assets;
        if t.scene == 1 || t.scene == 2 {
            let jx = ((self.tick * 7) % 3 - 1) as f32;
            let jy = ((self.tick * 5) % 3 - 1) as f32;
            a.draw_sprite(&format!("{}1", t.card), 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
            a.draw_sprite(&format!("{}2", t.card), 0.0, jx, jy, 1.0, 1.0, t.name_alpha);
        }
        match t.scene {
            0 | 1 => {
                if t.fade > 0.0 {
                    draw_rectangle(0.0, 0.0, 960.0, 540.0, Color::new(0.0, 0.0, 0.0, t.fade.clamp(0.0, 1.0)));
                }
            }
            2 => {
                // чёрный кадр с "дыркой" в форме spr_transition_mildred, которая сжимается
                let s = t.circle / 9.0;
                let (cx, cy) = (480.0 + 96.0 * s, 270.0 - 270.0 * s);
                let sc = t.circle;
                match (a.hole_texture("spr_transition_mildred"), a.sprites.get("spr_transition_mildred")) {
                    (Some(tex), Some(m)) => {
                        let (l, tp) = (cx - m.xo as f32 * sc, cy - m.yo as f32 * sc);
                        let (w, h) = (m.w as f32 * sc, m.h as f32 * sc);
                        draw_texture_ex(&tex, l, tp, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
                        let (r, b) = (l + w, tp + h);
                        if tp > 0.0 {
                            draw_rectangle(0.0, 0.0, 960.0, tp, BLACK);
                        }
                        if b < 540.0 {
                            draw_rectangle(0.0, b.max(0.0), 960.0, 540.0 - b.max(0.0), BLACK);
                        }
                        let (y0, y1) = (tp.max(0.0), b.min(540.0));
                        if l > 0.0 && y1 > y0 {
                            draw_rectangle(0.0, y0, l, y1 - y0, BLACK);
                        }
                        if r < 960.0 && y1 > y0 {
                            draw_rectangle(r.max(0.0), y0, 960.0 - r.max(0.0), y1 - y0, BLACK);
                        }
                    }
                    _ => draw_rectangle(0.0, 0.0, 960.0, 540.0, BLACK),
                }
            }
            _ => draw_rectangle(0.0, 0.0, 960.0, 540.0, BLACK),
        }
        set_default_camera();
    }

    /// Масло, краска, такси, детектив, сердца секретов
    fn rock_logic(&mut self, input: &Input) {
        let name = self.room.data.name.clone();
        let pb = self.player.bbox();
        let (px, py) = (self.player.x, self.player.y);

        // --- масло: падает, скользит игрок ---
        if let Some(m) = self.assets.sprites.get("spr_butter").cloned() {
            let mut slip = false;
            let mut splats: Vec<(f32, f32)> = Vec::new();
            let blocks = &self.room.blocks;
            let mut i = 0;
            while i < self.butters.len() {
                let b = &mut self.butters[i];
                b.frame += 0.35;
                if !b.grounded {
                    b.vy = (b.vy + 0.25).min(20.0);
                }
                let dirv = if b.vy >= 0.0 { 1.0 } else { -1.0 };
                for _ in 0..(b.vy.abs().round() as i32) {
                    let ny = b.y + dirv;
                    let (l, t, r, bt) = inst_bbox(&m, b.x, ny, 1.0, 1.0);
                    if blocks.iter().any(|k| k.kind == world::BlockKind::Solid && k.hits(l, t, r, bt)) {
                        if dirv > 0.0 {
                            b.grounded = true;
                            b.vx = 0.0;
                        }
                        b.vy = 0.0;
                        break;
                    }
                    b.y = ny;
                }
                if b.vx != 0.0 {
                    let nx = b.x + b.vx;
                    let (l, t, r, bt) = inst_bbox(&m, nx, b.y, 1.0, 1.0);
                    if !blocks.iter().any(|k| k.kind == world::BlockKind::Solid && k.hits(l, t, r, bt)) {
                        b.x = nx;
                    } else {
                        b.vx = 0.0;
                    }
                }
                let (l, t, r, bt) = inst_bbox(&m, b.x, b.y, 1.0, 1.0);
                if !self.player.noclip && l <= pb.2 && r >= pb.0 && t <= pb.3 && bt >= pb.1 {
                    slip = true;
                    splats.push((b.x, b.y));
                    self.butters.swap_remove(i);
                } else {
                    i += 1;
                }
            }
            if slip {
                self.player.slip_on_butter();
            }
            for (x, y) in splats {
                let mut p = SpritePart::new("spr_butter_splat", x, y);
                p.anim = 0.35;
                p.life = 100.0;
                self.part(p);
            }
        }

        // --- paintbuddy: даёт краску ---
        for i in 0..self.room.paint_pickups.len() {
            if !self.paint_alive.get(i).copied().unwrap_or(false) {
                continue;
            }
            let (x, y) = self.room.paint_pickups[i];
            if (px - x).abs() <= 192.0 && (py - y).abs() <= 192.0 && !line_blocked(&self.room, x, y, px, py) {
                self.paint_alive[i] = false;
                self.taken_misc.insert(format!("paint:{name}:{i}"));
                self.has_paint = true;
                self.painting_count = 0;
                self.painting_max = 4;
                self.hud.tv.anim("spr_tv_happy", 180);
                self.hud.combo.timer = 60.0;
                self.toast = Some(("Paintbuddy с тобой! Раскрась 4 стены".to_string(), 200));
            }
        }
        // --- стены для покраски ---
        for i in 0..self.room.sprays.len() {
            let touched = overlap(self.room.sprays[i].rect, pb);
            let sp = &mut self.room.sprays[i];
            if self.has_paint && touched && !sp.active {
                sp.active = true;
                sp.alarm = 5;
                self.hud.combo.timer = 60.0;
                self.painting_count += 1;
                self.painted.insert((name.clone(), i));
            }
            if sp.active && sp.points > 0 {
                sp.alarm -= 1;
                if sp.alarm <= 0 {
                    self.collect += 10;
                    sp.points = (sp.points - 10).max(0);
                    sp.alarm = 5;
                }
            }
        }
        if self.has_paint && self.painting_max > 0 && self.painting_count >= self.painting_max && !self.can_enter_bus {
            self.can_enter_bus = true; // paintbuddy исчезает, такси ждёт
            self.toast = Some(("Head to the bus stop!".to_string(), 240));
        }

        // --- такси ---
        let cam_right = self.cam.x + self.view_size().x / 2.0;
        let can = self.can_enter_bus;
        let mut board: Option<(String, String)> = None;
        if let Some(m) = self.assets.sprites.get("spr_taxiidle").cloned() {
            for bus in self.room.buses.iter_mut() {
                let near = (bus.start_x - px).abs() <= 192.0;
                let target = if can || !near { bus.start_x } else { cam_right + 100.0 };
                bus.x = approach_f(bus.x, target, 32.0);
                let (l, t, r, b) = inst_bbox(&m, bus.x, bus.y, 1.0, 1.0);
                if can
                    && self.player.state == State::Normal
                    && self.player.grounded
                    && input.up
                    && overlap([l, t, r, b], pb)
                {
                    board = Some((bus.target_room.clone(), bus.target_door.clone()));
                }
            }
        }
        if let Some((room, door)) = board {
            if room.is_empty() {
                self.complete_level();
            } else {
                let door = if door.is_empty() { "A".to_string() } else { door };
                self.player.begin_enter("spr_player_lookdoor", &room, &door);
                self.audio.sfx("door", 0.8);
            }
        }

        // --- детектив ---
        if let Some(m) = self.assets.sprites.get("spr_detective_idle_collect").cloned() {
            for i in 0..self.room.detectives.len() {
                if !self.det_alive.get(i).copied().unwrap_or(false) {
                    continue;
                }
                let (x, y) = self.room.detectives[i];
                let (l, t, r, b) = inst_bbox(&m, x, y, 1.0, 1.0);
                if overlap([l, t, r, b], pb) {
                    self.det_alive[i] = false;
                    self.taken_misc.insert(format!("det:{name}:{i}"));
                    self.detective = true;
                    self.hud.tv.anim("spr_tv_happy", 180);
                    self.popup("DETECTIVE!", x, y - 60.0, Color::from_rgba(255, 230, 90, 255));
                    self.audio.sfx("toppin", 0.6);
                }
            }
        }

        // --- сердца секретов ---
        if self.heart_cd > 0 {
            self.heart_cd -= 1;
        } else if self.pending.is_none() && !self.player.noclip {
            let mut go: Option<(usize, String)> = None;
            for (i, h) in self.room.hearts.iter().enumerate() {
                if !self.heart_alive.get(i).copied().unwrap_or(false) || h.target_room.is_empty() {
                    continue;
                }
                let blocked = self.room.blocks.iter().any(|k| {
                    k.alive
                        && matches!(
                            k.obj.as_str(),
                            "obj_secretmetalblock" | "obj_destroyable_secret1" | "obj_destroyable_secret2" | "obj_metalblock"
                        )
                        && k.hits(h.rect[0], h.rect[1], h.rect[2], h.rect[3])
                });
                if !blocked && overlap(h.rect, pb) {
                    go = Some((i, h.target_room.clone()));
                    break;
                }
            }
            if let Some((i, target)) = go {
                if let Some(dest) = self.assets.room(&target) {
                    // место появления: obj_secret_heart_enter, иначе сердце в целевой комнате
                    let find = |obj: &str| dest.instances.iter().find(|n| n.obj == obj).map(|n| (n.x, n.y));
                    let at = find("obj_secret_heart_enter").or_else(|| find("obj_secret_heart"));
                    self.heart_alive[i] = false;
                    self.hearts_gone.insert((name.clone(), i));
                    self.heart_cd = 120;
                    self.start_transition(Spawn {
                        room: target, door: "DOODLE".into(), hallway: 0, dir: 1.0,
                        saved_pos: 0.0, saved_spd: 0.0, hallway_size: 1.0, from_climb: false, at,
                    });
                }
            }
        }
    }

    /// Конец этапа (такси без целевой комнаты): рекорд, возврат к воротам
    fn complete_level(&mut self) {
        let level = self.level_name();
        let secs = self.escape.total.saturating_sub(self.escape.timer) / 60;
        let _ = secs;
        let msg = format!("УРОВЕНЬ ПРОЙДЕН!  SCORE {}", self.collect);
        self.toast = Some((msg, 300));
        self.slot_data.record_level(level, self.collect);
        self.finish_escape();
        self.save_slot();
        let (room, at) = self.return_target();
        self.enter_at = at;
        let door = if at.is_some() { "" } else { "B" };
        self.player.begin_enter("spr_player_lookdoor", &room, door);
        self.audio.sfx("door", 0.8);
    }

    fn draw_rock(&self) {
        let a = &self.assets;
        let t = self.tick as f32;
        // стены для покраски (серые, пока не закрашены)
        if let Some(spr) = a.sprite_of("obj_sprayWall") {
            for sp in &self.room.sprays {
                let c = if sp.active { WHITE } else { Color::new(0.5, 0.5, 0.5, 1.0) };
                a.draw_sprite_col(spr, 0.0, sp.x, sp.y, 1.0, 1.0, c);
            }
        }
        if let Some(spr) = a.sprite_of("obj_paintbuddy_pickup") {
            for (i, &(x, y)) in self.room.paint_pickups.iter().enumerate() {
                if self.paint_alive.get(i).copied().unwrap_or(false) {
                    a.draw_sprite(spr, 0.0, x, y + (t * 0.1).sin() * 3.0, 1.0, 1.0, 1.0);
                }
            }
        }
        for bus in &self.room.buses {
            a.draw_sprite("spr_taxiidle", (t * 0.35) % a.frames("spr_taxiidle") as f32, bus.x, bus.y, 1.0, 1.0, 1.0);
        }
        for (i, &(x, y)) in self.room.detectives.iter().enumerate() {
            if self.det_alive.get(i).copied().unwrap_or(false) {
                a.draw_sprite("spr_detective_idle_collect", (t * 0.35) % a.frames("spr_detective_idle_collect") as f32, x, y, 1.0, 1.0, 1.0);
            }
        }
        for (i, h) in self.room.hearts.iter().enumerate() {
            if !self.heart_alive.get(i).copied().unwrap_or(false) {
                continue;
            }
            let blocked = self.room.blocks.iter().any(|k| {
                k.alive
                    && matches!(k.obj.as_str(), "obj_secretmetalblock" | "obj_destroyable_secret1" | "obj_destroyable_secret2" | "obj_metalblock")
                    && k.hits(h.rect[0], h.rect[1], h.rect[2], h.rect[3])
            });
            if !blocked {
                a.draw_sprite("spr_loveportal_idle", (t * 0.35) % a.frames("spr_loveportal_idle") as f32, h.x, h.y, 1.0, 1.0, 1.0);
            }
        }
        for b in &self.butters {
            let spr = if b.grounded { "spr_butter" } else { "spr_butter_spin" };
            a.draw_sprite(spr, b.frame % a.frames(spr) as f32, b.x, b.y, 1.0, 1.0, 1.0);
        }
    }

    /// Paintbuddy идёт за игроком, пока стены не закрашены
    fn draw_paintbuddy(&self) {
        if !self.has_paint || self.can_enter_bus {
            return;
        }
        if let Some(spr) = self.assets.sprite_of("obj_paintbuddy_pickup") {
            let len = self.history.len();
            if len == 0 {
                return;
            }
            let (hx, hy) = self.history[len.saturating_sub(41)];
            let xs = if self.player.xscale < 0.0 { -1.0 } else { 1.0 };
            self.assets.draw_sprite(spr, 0.0, hx - 32.0 * xs, hy - 18.0, xs, 1.0, 1.0);
        }
    }

    // ---------------- меню, пауза, сохранения ----------------

    fn apply_options(&mut self) {
        let o = self.opts.clone();
        self.audio.set_volumes(o.master, o.music, o.sfx, o.ambience);
        set_fullscreen(o.fullscreen);
    }

    fn enter_menu(&mut self) {
        self.mode = Mode::Menu;
        self.menus = Menus::new();
        self.pause.close();
        self.opt_menu = None;
        self.audio.stop_all();
        self.audio.set_music(Some("music_menu"));
    }

    /// Новая игра поверх старых материалов/звуков (после выбора файла, рестарта)
    fn carry_over(old: &mut Game) -> Game {
        let audio = std::mem::replace(&mut old.audio, Audio::empty());
        let mut g = Game::new(audio);
        g.debug = old.debug;
        g.no_tex = old.no_tex;
        g.wave = old.wave.take();
        g.escape_mat = old.escape_mat.take();
        g.add_mat = old.add_mat.take();
        g.rt_flip = old.rt_flip;
        g.opts = old.opts.clone();
        g.audio.set_volumes(g.opts.master, g.opts.music, g.opts.sfx, g.opts.ambience);
        g
    }

    /// Загрузка прогресса из файла сохранения и начало игры
    fn start_slot(&mut self, n: usize) {
        self.slot = n;
        let slot = Slot::load(n);
        self.palette = slot.palette.clamp(0, 11);
        self.tutorial_done = slot.tutorial_done;
        self.got_cages = slot
            .cages
            .iter()
            .filter_map(|k| {
                let mut it = k.rsplitn(3, ':');
                let y = it.next()?.parse().ok()?;
                let x = it.next()?.parse().ok()?;
                Some((it.next()?.to_string(), x, y))
            })
            .collect();
        self.slot_data = slot;
        self.slot_data.created = true;
        self.mode = Mode::Play;
        self.spawn_entities();
        self.update_room_audio();
        self.save_slot();
    }

    fn save_slot(&mut self) {
        if self.mode != Mode::Play {
            return;
        }
        self.slot_data.created = true;
        self.slot_data.palette = self.palette;
        self.slot_data.tutorial_done = self.tutorial_done;
        self.slot_data.cages = self.got_cages.iter().map(|(r, x, y)| format!("{r}:{x}:{y}")).collect();
        self.slot_data.save(self.slot);
    }

    fn step_menu(&mut self, mi: &MenuInput) -> Option<MenuRequest> {
        if self.opt_menu.is_some() {
            let (changed, close, snd) = {
                let om = self.opt_menu.as_mut().unwrap();
                om.update(&mut self.opts, mi)
            };
            for s in snd {
                self.audio.sfx(s, 0.8);
            }
            if changed {
                self.apply_options();
                self.opts.save();
            }
            if close {
                self.opt_menu = None;
            }
            return None;
        }
        let (req, snd) = self.menus.update(mi);
        for s in snd {
            self.audio.sfx(s, 0.8);
        }
        req
    }

    fn paused_ui(&self) -> bool {
        self.pause.active || self.pause.want_capture || self.opt_menu.is_some()
    }

    fn can_pause(&self) -> bool {
        self.mode == Mode::Play
            && self.pending.is_none()
            && self.titlecard.is_none()
            && self.phase == Phase::Playing
            && !self.paused_ui()
    }

    fn in_level(&self) -> bool {
        let n = &self.room.data.name;
        n.starts_with("wayback") || n.starts_with("tutorial") || n.starts_with("city")
    }

    fn open_pause(&mut self) {
        let sec = |i: u32| self.slot_data.secrets.contains(&format!("wayback_secret{i}"));
        let secrets = [sec(1), sec(2), sec(3)];
        let (in_level, treasure) = (self.in_level(), self.slot_data.treasure);
        self.pause.open(in_level, secrets, treasure);
        self.audio.set_music(Some("music_pause"));
        self.audio.set_ambience(None);
        self.audio.set_loop(None);
    }

    /// Снимок экрана для паузы (вызывается из main сразу после draw)
    fn capture_pause(&mut self) {
        let img = get_screen_data();
        self.pause.activate(Texture2D::from_image(&img));
    }

    fn resume_game(&mut self) {
        self.pause.close();
        self.audio.set_music(None);
        self.update_room_audio();
    }

    fn step_pause(&mut self, mi: &MenuInput) {
        if self.opt_menu.is_some() {
            let (changed, close, snd) = {
                let om = self.opt_menu.as_mut().unwrap();
                om.update(&mut self.opts, mi)
            };
            for s in snd {
                self.audio.sfx(s, 0.8);
            }
            if changed {
                self.apply_options();
                self.opts.save();
            }
            if close {
                self.opt_menu = None;
            }
            return;
        }
        let (act, snd) = self.pause.update(mi);
        for s in snd {
            self.audio.sfx(s, 0.8);
        }
        match act {
            Some(PauseAction::Resume) => self.resume_game(),
            Some(PauseAction::Options) => self.opt_menu = Some(OptionsMenu::new()),
            Some(PauseAction::Restart) => {
                // перезапуск уровня: с самого начала (global.resetRoom)
                self.resume_game();
                let room = match self.level_name() {
                    "tutorial" => "tutorial_1",
                    "city" => "city_1",
                    _ => "wayback_1",
                };
                self.escape.active = false;
                self.collect = 0;
                self.killed.clear();
                self.taken.clear();
                self.toppins.clear();
                self.hud.combo.amt = 0;
                self.hud.combo.timer = 60.0;
                self.hud.combo.dropped = false;
                self.start_transition(Spawn {
                    room: room.into(), door: "A".into(), hallway: 0, dir: 1.0,
                    saved_pos: 0.0, saved_spd: 0.0, hallway_size: 1.0, from_climb: false, at: None,
                });
            }
            Some(PauseAction::Exit) => {
                if self.in_level() {
                    // выйти из уровня к воротам хаба
                    self.resume_game();
                    self.finish_escape();
                    self.collect = 0;
                    let (room, at) = self.return_target();
                    self.start_transition(Spawn {
                        room, door: "B".into(), hallway: 0, dir: 1.0,
                        saved_pos: 0.0, saved_spd: 0.0, hallway_size: 1.0, from_climb: false, at,
                    });
                } else {
                    // MAIN MENU
                    self.save_slot();
                    self.enter_menu();
                }
            }
            None => {}
        }
    }

    fn draw_menu(&self) {
        set_camera(&Camera2D { target: vec2(480.0, 270.0), zoom: vec2(2.0 / 960.0, 2.0 / 540.0), ..Default::default() });
        self.menus.draw(&self.assets);
        if let Some(om) = &self.opt_menu {
            om.draw(&self.opts);
        }
        set_default_camera();
    }

    fn toggle_noclip(&mut self) {
        let on = !self.player.noclip;
        if self.player.set_noclip(on, &self.room) {
            self.audio.sfx("noclip_on", 0.6);
            let msg = if on { "NOCLIP вкл: стрелки - полёт, Shift - быстрее, V - выкл" } else { "NOCLIP выкл" };
            self.toast = Some((msg.to_string(), 120));
        } else {
            self.toast = Some(("Нельзя выключить ноуклип внутри стены".to_string(), 120));
        }
    }

    /// obj_player/Other_4
    fn place_player(&mut self, sp: &Spawn) {
        if let Some((x, y)) = sp.at {
            self.player.x = x;
            self.player.y = y;
        } else if let Some(d) = self.room.door(&sp.door).cloned() {
            if sp.hallway == 2 {
                let hall = self
                    .room
                    .hallways
                    .iter()
                    .find(|h| h.vertical && overlap(d.rect, (h.rect[0], h.rect[1], h.rect[2], h.rect[3])))
                    .cloned();
                if let Some(h) = hall {
                    let ratio = h.width / sp.hallway_size;
                    self.player.x = if sp.from_climb {
                        d.x + sp.saved_pos * ratio
                    } else {
                        d.x + 16.0 + (sp.saved_pos * ratio - 16.0)
                    };
                    self.player.y = d.y - (32.0 * 6.0) * sp.dir;
                    self.player.vsp = sp.saved_spd;
                }
            } else {
                self.player.x = if sp.hallway == 1 { d.x + 16.0 + 32.0 * 3.0 * sp.dir } else { d.x + 16.0 };
                self.player.y = d.y - 14.0;
            }
        }
        // Если игрок оказался на двери/воротах - анимация выхода "лицом к зрителю"
        let bb = self.player.bbox();
        let on_portal = self.room.portals.iter().any(|p| overlap(p.rect, bb));
        if on_portal {
            self.player.begin_walkfront();
        } else if self.player.state == State::EnterDoor {
            self.player.state = State::Normal;
        }
        self.player.enter_target = None;
    }

    // ---------------- камера ----------------

    fn view_size(&self) -> Vec2 {
        let h = 540.0;
        let aspect = screen_width() / screen_height();
        vec2(h * aspect, h)
    }

    fn camera_target(&self) -> Vec2 {
        let v = self.view_size();
        let (rw, rh) = (self.room.data.width as f32, self.room.data.height as f32);
        let mut cx = self.player.x;
        let mut cy = self.player.y - 30.0;
        cx = if rw > v.x { cx.clamp(v.x / 2.0, rw - v.x / 2.0) } else { rw / 2.0 };
        cy = if rh > v.y { cy.clamp(v.y / 2.0, rh - v.y / 2.0) } else { rh / 2.0 };
        vec2(cx, cy)
    }

    // ---------------- игровой шаг (60 Гц) ----------------

    fn step(&mut self, input: Input) {
        self.tick += 1;
        if self.titlecard.is_some() {
            self.step_titlecard();
            return;
        }

        match self.phase {
            Phase::FadeOut(n) if n >= FADE_FRAMES => {
                self.load_pending();
                self.phase = Phase::FadeIn(0);
            }
            Phase::FadeOut(n) => self.phase = Phase::FadeOut(n + 1),
            Phase::FadeIn(n) if n >= FADE_FRAMES => self.phase = Phase::Playing,
            Phase::FadeIn(n) => self.phase = Phase::FadeIn(n + 1),
            Phase::Playing => {}
        }

        // хитстоп (5 кадров) после убийства врага
        if self.hitstop > 0 {
            self.hitstop -= 1;
            self.update_particles();
            return;
        }

        self.player.update(&self.assets, &mut self.room, &input);

        // звуки игрока
        // частицы разрушенных блоков
        let broken = std::mem::take(&mut self.player.broken);
        for (l, t, r, b, obj) in broken {
            self.spawn_break(l, t, r, b, &obj);
            if obj == "obj_monolith" {
                self.monolith_destroyed(l, t, r, b);
            }
        }
        self.update_particles();

        let events = std::mem::take(&mut self.player.events);
        for e in events {
            let vol = match e {
                "step" => 0.20,
                "hurt" => 0.16, // был слишком громким
                _ => 0.7,
            };
            self.audio.sfx(e, vol);
        }
        let run_loop = match self.player.state {
            State::Mach2 if self.player.grounded => Some("mach2loop"),
            State::Mach3 if self.player.grounded => {
                if self.player.sprite == "spr_player_crazyrun" { Some("maxspeed") } else { Some("mach3loop") }
            }
            _ => None,
        };
        self.audio.set_loop(run_loop);
        self.portal_logic(&input);
        self.beer_logic(&input);
        self.pickup_logic();
        self.entity_logic(&input);
        self.rock_logic(&input);

        // телевизор и комбо
        let ctx = Ctx {
            player_state: self.player.state,
            movespeed: self.player.movespeed,
            escape: self.escape.active,
            room: &self.room.data.name,
            player_x: self.player.x,
            player_y: self.player.y,
            room_w: self.room.data.width as f32,
            freeze_timer: matches!(self.player.state, State::EnterDoor | State::WalkFront)
                || self.pending.is_some()
                || self.phase != Phase::Playing,
            hud_w: self.view_size().x,
        };
        let lost_points = self.hud.update(&self.assets, &ctx);
        if lost_points > 0 {
            // потеряно комбо - очки за него (obj_comboTitle: points)
            self.collect += lost_points;
            let (x, y) = (self.player.x, self.player.y - 80.0);
            self.popup(&format!("+{lost_points}"), x, y, Color::from_rgba(255, 230, 90, 255));
        }

        // анимация декора
        for d in self.room.decor.iter_mut() {
            if d.animate {
                let n = self.assets.frames(&d.sprite) as f32;
                d.frame += d.speed;
                if d.frame >= n {
                    d.frame -= n;
                }
            }
        }

        let target = self.camera_target();
        self.cam += (target - self.cam) * 0.15;

        if let Some((_, t)) = self.toast.as_mut() {
            *t -= 1;
            if *t <= 0 {
                self.toast = None;
            }
        }
    }

    /// obj_hallway / obj_hallway_vertical / obj_door / obj_levelgate
    fn portal_logic(&mut self, input: &Input) {
        if self.pending.is_some() {
            return;
        }
        let bb = self.player.bbox();

        // --- холлвеи: срабатывают при касании ---
        let hit = self
            .room
            .hallways
            .iter()
            .find(|h| !h.target_room.is_empty() && overlap(h.rect, bb))
            .cloned();
        if let Some(h) = hit {
            let sp = Spawn {
                room: h.target_room.clone(),
                door: if h.target_door.is_empty() { "A".into() } else { h.target_door.clone() },
                hallway: if h.vertical { 2 } else { 1 },
                dir: h.dir,
                saved_pos: self.player.x - h.x,
                saved_spd: self.player.vsp,
                hallway_size: h.width,
                from_climb: self.player.state == State::ClimbWall, at: None,
            };
            self.start_transition(sp);
            return;
        }

        // --- obj_exitgate: выход из уровня, только во время pizza time ---
        if self.escape.active
            && self.player.state == State::Normal
            && self.player.grounded
            && input.up
            && self.room.portals.iter().any(|q| q.is_exit && overlap(q.rect, bb))
        {
            let secs = (self.escape.total - self.escape.timer) / 60;
            let msg = format!("УРОВЕНЬ ПРОЙДЕН!  SCORE {}   TIME {}:{:02}", self.collect, secs / 60, secs % 60);
            self.toast = Some((msg, 300));
            let level = self.level_name();
            self.slot_data.record_level(level, self.collect);
            self.finish_escape();
            self.save_slot();
            let (room, at) = self.return_target();
            self.enter_at = at;
            let door = if at.is_some() { "" } else { "B" };
            self.player.begin_enter("spr_player_lookdoor", &room, door);
            self.audio.sfx("door", 0.8);
            return;
        }

        // --- двери и ворота уровней: вход по "вверх" ---
        let p = &self.player;
        if p.state == State::Normal && p.grounded && input.up {
            let portal = self
                .room
                .portals
                .iter()
                .find(|q| !q.target_room.is_empty() && !(q.hide_in_escape && self.escape.active) && overlap(q.rect, bb))
                .cloned();
            if let Some(q) = portal {
                let sprite = if q.is_gate { "spr_player_entergate" } else { "spr_player_lookdoor" };
                let door = if q.target_door.is_empty() { "A".to_string() } else { q.target_door.clone() };
                if q.is_gate {
                    self.return_to = Some((self.room.data.name.clone(), q.x, q.y + 98.0));
                    self.collect = 0; // счёт считается отдельно для каждого прохождения уровня
                    self.enter_is_gate = true;
                    self.has_paint = false;
                    self.painting_count = 0;
                    self.painting_max = 0;
                    self.can_enter_bus = false;
                    self.painted.clear();
                    self.hearts_gone.clear();
                    self.taken_misc.clear();
                }
                self.player.begin_enter(sprite, &q.target_room, &door);
                self.audio.sfx("door", 0.8);
            }
        }

        if self.player.state == State::EnterDoor {
            // игрок подходит к центру двери (scr_player_enterdoor)
            if let Some(q) = self.room.portals.iter().find(|q| overlap(q.rect, bb)) {
                let tx = if q.is_gate { q.x } else { (q.rect[0] + q.rect[2] + 1.0) / 2.0 };
                let dx = tx - self.player.x;
                self.player.x += dx.clamp(-6.0, 6.0);
            }
            let last = self.assets.frames(&self.player.sprite) as f32 - 1.0;
            if self.player.index >= last {
                if let Some((room, door)) = self.player.enter_target.clone() {
                    let at = self.enter_at.take();
                    let sp = Spawn {
                        room, door, hallway: 0, dir: 1.0,
                        saved_pos: 0.0, saved_spd: 0.0, hallway_size: 1.0, from_climb: false, at,
                    };
                    // ворота уровня -> титульная карта; остальное - обычное затемнение
                    let card = if self.enter_is_gate { Self::titlecard_for(&sp.room) } else { None };
                    self.enter_is_gate = false;
                    match card {
                        Some(c) if self.assets.room(&sp.room).is_some() => self.start_titlecard(sp, c),
                        _ => self.start_transition(sp),
                    }
                    self.player.enter_target = None;
                }
            }
        }
    }

    /// obj_ballofbeer/Step_0: ↓ (или пике) сверху - в бочку; для висящей на потолке бочки - ↑
    fn beer_logic(&mut self, input: &Input) {
        if self.beer_cd > 0 {
            self.beer_cd -= 1;
            return;
        }
        if self.player.noclip || self.player.state == State::EnterDoor || self.player.state == State::WalkFront {
            return;
        }
        let (l, t, r, b) = self.player.bbox();
        let diving = matches!(self.player.state, State::GroundPound | State::Tumble);
        let beer = self
            .room
            .beers
            .iter()
            .find(|q| {
                if q.target_room.is_empty() {
                    return false;
                }
                if q.dir > 0.0 {
                    overlap(q.rect, (l, t + 10.0, r, b + 10.0)) && (input.down || diving)
                } else {
                    overlap(q.rect, (l, t - 10.0, r, b - 10.0)) && input.up
                }
            })
            .cloned();
        if let Some(q) = beer {
            let door = if q.target_door.is_empty() { "A".to_string() } else { q.target_door.clone() };
            let (x, y) = (q.x + 32.0, if q.dir > 0.0 { q.y } else { q.y - 20.0 });
            self.player.begin_beer(q.dir < 0.0, x, y, &q.target_room, &door);
            self.audio.sfx("door", 0.8);
        }
    }

    fn rnd(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    /// Обломки, искры и пыль на месте разрушенного блока
    fn spawn_break(&mut self, l: f32, t: f32, r: f32, b: f32, obj: &str) {
        let (w, h) = (r - l + 1.0, b - t + 1.0);
        let metal = obj.contains("metal");
        let palette: &[(u8, u8, u8)] = if metal {
            &[(170, 176, 188), (120, 126, 140), (210, 214, 224), (80, 84, 96)]
        } else if obj.contains("donut") {
            &[(233, 150, 190), (250, 220, 170), (180, 110, 70)]
        } else if obj.contains("secret") {
            &[(150, 90, 190), (110, 60, 150), (190, 140, 220)]
        } else {
            &[(160, 108, 60), (196, 140, 80), (112, 72, 40), (220, 170, 110)]
        };
        let cx = (l + r) / 2.0;
        let n = ((w * h) / 180.0).clamp(10.0, 40.0) as i32;
        for _ in 0..n {
            let px = l + self.rnd() * w;
            let py = t + self.rnd() * h;
            let c = palette[(self.rnd() * palette.len() as f32) as usize % palette.len()];
            let life = 40.0 + self.rnd() * 30.0;
            let vx = (px - cx) / w * 8.0 + (self.rnd() - 0.5) * 3.0;
            let vy = -2.0 - self.rnd() * 6.0;
            let size = 4.0 + self.rnd() * 6.0;
            let rotv = (self.rnd() - 0.5) * 0.5;
            let rot = self.rnd() * 6.28;
            self.particles.push(Particle {
                x: px, y: py, vx, vy, size, rot, rotv, life, fade: 20.0,
                color: Color::from_rgba(c.0, c.1, c.2, 255), puff: false, frame: 0.0, gravity: 0.45,
            });
        }
        if metal {
            // искры
            for _ in 0..12 {
                let px = l + self.rnd() * w;
                let py = t + self.rnd() * h;
                let ang = self.rnd() * std::f32::consts::TAU;
                let sp = 3.0 + self.rnd() * 6.0;
                let life = 12.0 + self.rnd() * 14.0;
                self.particles.push(Particle {
                    x: px, y: py, vx: ang.cos() * sp, vy: ang.sin() * sp - 2.0, size: 2.5, rot: ang, rotv: 0.0,
                    life, fade: 8.0, color: Color::from_rgba(255, 238, 160, 255), puff: false, frame: 0.0, gravity: 0.25,
                });
            }
        }
        // облачка пыли
        for _ in 0..4 {
            let px = l + self.rnd() * w;
            let py = t + self.rnd() * h;
            let vx = (self.rnd() - 0.5) * 1.5;
            self.particles.push(Particle {
                x: px, y: py, vx, vy: -0.4, size: 0.0, rot: 0.0, rotv: 0.0, life: 100.0, fade: 1.0,
                color: WHITE, puff: true, frame: 0.0, gravity: 0.0,
            });
        }
        if self.particles.len() > 600 {
            let extra = self.particles.len() - 600;
            self.particles.drain(0..extra);
        }
    }

    fn update_particles(&mut self) {
        let max_puff = self.assets.frames("spr_cloudeffect") as f32;
        for p in self.particles.iter_mut() {
            p.x += p.vx;
            p.y += p.vy;
            p.vy += p.gravity;
            p.vx *= 0.985;
            p.rot += p.rotv;
            if p.puff {
                p.frame += 0.4;
                if p.frame >= max_puff {
                    p.life = 0.0;
                }
            } else {
                p.life -= 1.0;
            }
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// obj_uparrow: стрелка над игроком, когда он стоит на двери/воротах
    fn draw_uparrow(&self) {
        if self.player.noclip || matches!(self.player.state, State::EnterDoor | State::WalkFront) {
            return;
        }
        let bb = self.player.bbox();
        let esc = self.escape.active;
        let on_portal = self.room.portals.iter().any(|q| {
            let usable = (!q.target_room.is_empty() && !(q.hide_in_escape && esc)) || (q.is_exit && esc);
            usable && overlap(q.rect, bb)
        });
        if on_portal {
            let n = self.assets.frames("spr_uparrow") as f32;
            let frame = (self.tick as f32 * 0.1) % n.max(1.0);
            self.assets.draw_sprite("spr_uparrow", frame, self.player.x.floor(), self.player.y.floor() - 16.0, 1.0, 1.0, 1.0);
        }
    }

    /// Реплика Flo: рамка сверху экрана, текст печатается по буквам
    fn draw_dialog(&self) {
        let text = tidy_text(&self.dialog);
        let w = screen_width() * 0.74;
        let x = (screen_width() - w) / 2.0;
        let size = 26.0;
        // перенос по словам
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in text.split_whitespace() {
            let trial = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if measure_text_f(&trial, size as u16).width > w - 44.0 && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = trial;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        let h = lines.len() as f32 * 32.0 + 34.0;
        let y = 118.0;
        draw_rectangle(x, y, w, h, Color::new(0.07, 0.04, 0.12, 0.92));
        draw_rectangle_lines(x, y, w, h, 4.0, Color::from_rgba(255, 190, 60, 255));
        let mut left = self.dialog_t as usize;
        for (i, line) in lines.iter().enumerate() {
            let n = line.chars().count();
            let shown: String = line.chars().take(left.min(n)).collect();
            left = left.saturating_sub(n + 1);
            draw_text_f(&shown, x + 22.0, y + 36.0 + i as f32 * 32.0, size, WHITE);
        }
    }

    /// shd_pal_swapper: палитра игрока (строка 0 текстуры - исходные цвета, столбец N - замена)
    /// Шейдеры: волна паузы (shd_wave) и эффект побега (shd_escape). Палитры перекрашиваются на CPU (assets.rs).
    fn build_palette(&mut self) {
        let alpha_blend = || PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };
        self.escape_mat = load_material(
            ShaderSource::Glsl { vertex: PAL_VERT, fragment: ESCAPE_FRAG },
            MaterialParams {
                pipeline_params: alpha_blend(),
                uniforms: vec![
                    UniformDesc::new("u_time", UniformType::Float1),
                    UniformDesc::new("u_intensity", UniformType::Float1),
                    UniformDesc::new("u_wave_amplitude", UniformType::Float1),
                    UniformDesc::new("u_wave_frequency", UniformType::Float1),
                ],
                ..Default::default()
            },
        )
        .map_err(|e| eprintln!("шейдер побега не собрался: {e:?}"))
        .ok();
        // обычная текстура с аддитивным смешиванием (призрачные копии кадра при побеге)
        self.add_mat = load_material(
            ShaderSource::Glsl { vertex: PAL_VERT, fragment: PLAIN_FRAG },
            MaterialParams {
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::One,
                    )),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(|e| eprintln!("аддитивный материал не собрался: {e:?}"))
        .ok();
        self.wave = load_material(
            ShaderSource::Glsl { vertex: PAL_VERT, fragment: WAVE_FRAG },
            MaterialParams {
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::Value(BlendValue::SourceAlpha),
                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                    )),
                    ..Default::default()
                },
                uniforms: vec![
                    UniformDesc::new("time", UniformType::Float1),
                    UniformDesc::new("frequency", UniformType::Float1),
                    UniformDesc::new("amplitude", UniformType::Float1),
                ],
                ..Default::default()
            },
        )
        .ok();
    }

    /// Хитбоксы (F3): блоки, склоны, платформы, маска игрока, враги, двери, холлвеи, бочки, лестницы...
    fn draw_hitboxes(&self, view: Rect) {
        let rect_col = |l: f32, t: f32, r: f32, b: f32, c: Color, fill: f32| {
            let (w, h) = (r - l + 1.0, b - t + 1.0);
            if l > view.x + view.w || l + w < view.x || t > view.y + view.h || t + h < view.y {
                return;
            }
            if fill > 0.0 {
                draw_rectangle(l, t, w, h, Color::new(c.r, c.g, c.b, fill));
            }
            draw_rectangle_lines(l, t, w, h, 2.0, c);
        };
        for k in &self.room.blocks {
            if !k.alive {
                continue;
            }
            let dim = if k.enabled { 1.0 } else { 0.25 };
            match k.kind {
                world::BlockKind::Solid => {
                    let c = if k.metal {
                        Color::new(0.55, 0.7, 1.0, dim)
                    } else if k.destructible {
                        Color::new(1.0, 0.6, 0.1, dim)
                    } else {
                        Color::new(1.0, 0.2, 0.2, dim)
                    };
                    rect_col(k.l, k.t, k.r, k.b, c, 0.12 * dim);
                }
                world::BlockKind::Slope => {
                    let c = Color::new(1.0, 1.0, 0.2, dim);
                    let (l, t, r, b) = (k.l, k.t, k.r + 1.0, k.b + 1.0);
                    // направление по правилам scr_slope_collideCheck: sx > 0 - поверхность растёт вправо
                    let (x0, mut y0, x1, mut y1) = if k.sx > 0.0 { (l, b, r, t) } else { (l, t, r, b) };
                    if k.sy < 0.0 {
                        y0 = t + b - y0;
                        y1 = t + b - y1;
                    }
                    draw_line(x0, y0, x1, y1, 3.0, c);
                    rect_col(k.l, k.t, k.r, k.b, Color::new(1.0, 1.0, 0.2, 0.5 * dim), 0.0);
                }
                world::BlockKind::Platform => {
                    let y = if k.sy < 0.0 { k.b + 1.0 } else { k.t };
                    draw_line(k.l, y, k.r + 1.0, y, 3.0, Color::new(0.2, 1.0, 1.0, dim));
                    rect_col(k.l, k.t, k.r, k.b, Color::new(0.2, 1.0, 1.0, 0.35 * dim), 0.05);
                }
            }
        }
        for h in &self.room.hallways {
            rect_col(h.rect[0], h.rect[1], h.rect[2], h.rect[3], MAGENTA, 0.12);
            draw_text_f(&format!("→ {} [{}]", h.target_room, h.target_door), h.rect[0] + 4.0, h.rect[1] + 16.0, 16.0, MAGENTA);
        }
        for d in &self.room.doors {
            rect_col(d.rect[0], d.rect[1], d.rect[2], d.rect[3], SKYBLUE, 0.0);
            draw_text_f(&format!("door{}", d.letter), d.rect[0] + 2.0, d.rect[1] + 14.0, 14.0, SKYBLUE);
        }
        for q in &self.room.portals {
            let c = if q.is_exit { WHITE } else if q.is_gate { LIME } else { BLUE };
            rect_col(q.rect[0], q.rect[1], q.rect[2], q.rect[3], c, 0.1);
            if !q.target_room.is_empty() {
                draw_text_f(&format!("→ {} [{}]", q.target_room, q.target_door), q.rect[0] + 4.0, q.rect[1] - 6.0, 16.0, c);
            }
        }
        for b in &self.room.beers {
            rect_col(b.rect[0], b.rect[1], b.rect[2], b.rect[3], PURPLE, 0.15);
        }
        for l in &self.room.ladders {
            rect_col(l[0], l[1], l[2], l[3], ORANGE, 0.15);
        }
        for c in &self.room.cages {
            rect_col(c.rect[0], c.rect[1], c.rect[2], c.rect[3], PINK, 0.15);
        }
        for p in self.room.pickups.iter().filter(|p| p.alive) {
            if let Some(m) = self.assets.sprites.get(&p.sprite) {
                let (l, t, r, b) = inst_bbox(m, p.x, p.y, 1.0, 1.0);
                rect_col(l, t, r, b, Color::new(0.3, 1.0, 0.9, 0.9), 0.0);
            }
        }
        for sp in &self.room.spawners {
            draw_circle_lines(sp.x, sp.y, 300.0, 1.0, Color::new(0.7, 0.7, 0.7, 0.5));
            draw_circle(sp.x, sp.y, 6.0, GRAY);
        }
        for b in &self.room.books {
            draw_circle(b.x, b.y, 6.0, Color::new(1.0, 0.8, 0.3, 1.0));
        }
        for e in &self.enemies {
            let (l, t, r, b) = e.rect();
            rect_col(l, t, r, b, YELLOW, 0.1);
        }
        for d in &self.darts {
            if let Some(m) = self.assets.sprites.get("spr_cocoantDart") {
                let (l, t, r, b) = inst_bbox(m, d.x, d.y, d.dir, 1.0);
                rect_col(l, t, r, b, YELLOW, 0.3);
            }
        }
        // маска игрока (текущая) и точка origin
        let (l, t, r, b) = self.player.bbox();
        rect_col(l, t, r, b, GREEN, 0.15);
        draw_circle(self.player.x, self.player.y, 3.0, GREEN);
    }

    /// shd_escape + призрачные аддитивные копии кадра (obj_screensizer/Draw_74)
    fn escape_pass(&self) {
        let rt = self.rt.borrow();
        let Some((target, sw, sh)) = rt.as_ref() else { return };
        let (sw, sh) = (*sw as f32, *sh as f32);
        let (Some(em), Some(am)) = (&self.escape_mat, &self.add_mat) else { return };
        set_default_camera();
        // bleedIntensity = lerp(bleedIntensity, 0.07, 0.01)
        let b = self.bleed.get();
        let b = b + (0.07 - b) * 0.01;
        self.bleed.set(b);
        em.set_uniform("u_time", get_time() as f32);
        em.set_uniform("u_intensity", b);
        em.set_uniform("u_wave_amplitude", 0.02f32);
        em.set_uniform("u_wave_frequency", 10.0f32);
        let params = || DrawTextureParams { dest_size: Some(vec2(sw, sh)), flip_y: self.rt_flip, ..Default::default() };
        gl_use_material(em);
        draw_texture_ex(&target.texture, 0.0, 0.0, WHITE, params());
        gl_use_default_material();
        // два призрачных сдвинутых слоя (±15 px по wave), прозрачность 0.08
        let off = hud::wave(-15.0, 15.0, 3.0, 0.0) * sw / 960.0;
        gl_use_material(am);
        draw_texture_ex(&target.texture, off, 0.0, Color::new(1.0, 1.0, 1.0, 0.08), params());
        draw_texture_ex(&target.texture, -off, 0.0, Color::new(1.0, 1.0, 1.0, 0.08), params());
        gl_use_default_material();
    }

    fn draw_particles(&self) {
        for p in &self.particles {
            if p.puff {
                self.assets.draw_sprite("spr_cloudeffect", p.frame, p.x, p.y, 2.0, 2.0, 0.9);
            } else {
                let a = (p.life / p.fade).clamp(0.0, 1.0);
                let t = self.assets.tint_color(1.0);
                let c = Color::new(p.color.r * t.r, p.color.g * t.g, p.color.b * t.b, a);
                draw_rectangle_ex(
                    p.x, p.y, p.size, p.size,
                    DrawRectangleParams { offset: vec2(0.5, 0.5), rotation: p.rot, color: c },
                );
            }
        }
    }

    /// obj_collect / obj_bigcollect
    fn pickup_logic(&mut self) {
        let pb = self.player.bbox();
        let (px, py) = (self.player.x, self.player.y);
        let esc = self.escape.active;
        let room_name = self.room.data.name.clone();
        let solids: Vec<_> = self
            .room
            .blocks
            .iter()
            .filter(|b| b.kind == world::BlockKind::Solid && b.alive)
            .cloned()
            .collect();
        let mut gained = 0;
        let mut sounds: Vec<&'static str> = Vec::new();

        for (i, p) in self.room.pickups.iter_mut().enumerate().filter(|(_, p)| p.alive) {
            let Some(meta) = self.assets.sprites.get(&p.sprite) else { continue };
            p.frame += 0.35;
            let nf = self.assets.frames(&p.sprite) as f32;
            if p.frame >= nf {
                p.frame -= nf;
            }
            // обычные коллекты исчезают в pizza time, коллекты побега до неё - полупрозрачные и неактивные
            let active = if p.escape { esc } else { !esc };
            if !active {
                continue;
            }
            // canmove = place_empty(xstart, ystart, obj_solid)
            let (l0, t0, r0, b0) = inst_bbox(meta, p.x0, p.y0, 1.0, 1.0);
            let canmove = !solids.iter().any(|s| s.hits(l0, t0, r0, b0));
            if !canmove {
                continue; // спрятан внутри разрушаемого блока
            }
            let (l, t, r, b) = inst_bbox(meta, p.x, p.y, 1.0, 1.0);
            // distance_to_object(obj_player) < 25
            let dx = (l - pb.2).max(pb.0 - r).max(0.0);
            let dy = (t - pb.3).max(pb.1 - b).max(0.0);
            if (dx * dx + dy * dy).sqrt() < 25.0 {
                p.homing = true;
            }
            if p.homing {
                let (vx, vy) = (px - p.x, py - p.y);
                let dist = (vx * vx + vy * vy).sqrt();
                if dist <= p.speed {
                    p.x = px;
                    p.y = py;
                } else {
                    p.x += vx / dist * p.speed;
                    p.y += vy / dist * p.speed;
                }
                p.speed += 1.0;
            }
            if l <= pb.2 && r >= pb.0 && t <= pb.3 && b >= pb.1 {
                p.alive = false;
                gained += p.value;
                self.taken.insert((room_name.clone(), i, p.escape));
                sounds.push(if p.value >= 100 || (p.escape && p.value >= 20) { "bigcollect" } else { "collect" });
            }
        }
        self.collect += gained;
        if gained > 0 {
            // obj_collect: таймер комбо +10 (максимум 60)
            self.hud.combo.timer = (self.hud.combo.timer + 10.0).min(60.0);
        }
        if sounds.iter().any(|s| *s == "bigcollect") {
            self.hud.tv.anim("spr_tv_happy", 180);
        }
        if self.collect_cd > 0 {
            self.collect_cd -= 1;
        }
        for s in sounds {
            if s == "collect" {
                // обычные коллекты идут пачками - не чаще раза в 6 кадров
                if self.collect_cd == 0 {
                    self.audio.sfx(s, 0.4);
                    self.collect_cd = 6;
                }
            } else {
                self.audio.sfx(s, 0.6);
            }
        }
    }

    // ---------------- враги, топпины, pizza time ----------------

    /// Расставляет врагов комнаты и убирает уже собранные предметы и клетки
    fn spawn_entities(&mut self) {
        self.enemies.clear();
        self.darts.clear();
        let name = self.room.data.name.clone();
        for (i, sp) in self.room.enemy_spawns.iter().enumerate() {
            if sp.escape && !self.escape.active {
                continue; // escape-враги появляются только в pizza time
            }
            let Some(kind) = Kind::from_obj(&sp.obj) else { continue };
            if kind != Kind::Sunny && self.killed.contains(&(name.clone(), i, sp.escape)) {
                continue;
            }
            self.next_uid += 1;
            let mut e = Enemy::new(i, kind, sp.x, sp.y, sp.xscale, sp.plosher, sp.escape);
            e.uid = self.next_uid;
            self.enemies.push(e);
        }
        for (i, p) in self.room.pickups.iter_mut().enumerate() {
            if self.taken.contains(&(name.clone(), i, p.escape)) {
                p.alive = false;
            }
        }
        // Rock Bottom: масло, покрашенные стены, подобранные предметы
        self.butters = self
            .room
            .butters
            .iter()
            .map(|&(x, y)| Butter { x, y, vx: 0.0, vy: -3.0, grounded: false, frame: 0.0 })
            .collect();
        for (i, sp) in self.room.sprays.iter_mut().enumerate() {
            if self.painted.contains(&(name.clone(), i)) {
                sp.active = true;
                sp.points = 0;
            }
        }
        self.det_alive = (0..self.room.detectives.len()).map(|i| !self.taken_misc.contains(&format!("det:{name}:{i}"))).collect();
        self.paint_alive = (0..self.room.paint_pickups.len()).map(|i| !self.taken_misc.contains(&format!("paint:{name}:{i}"))).collect();
        self.heart_alive = (0..self.room.hearts.len()).map(|i| !self.hearts_gone.contains(&(name.clone(), i))).collect();
        self.refresh_block_modes();
        // obj_tutorialbook/Other_4: Flo опускается на землю
        let blocks = self.room.blocks.clone();
        if let Some(m) = self.assets.sprites.get("spr_floidle") {
            for b in self.room.books.iter_mut() {
                let mut guard = 0;
                while guard < 600 {
                    let (l, t, r, bt) = inst_bbox(m, b.x, b.y + 1.0, 1.0, 1.0);
                    if blocks.iter().any(|k| k.kind == world::BlockKind::Solid && k.enabled && k.hits(l, t, r, bt)) {
                        break;
                    }
                    b.y += 1.0;
                    guard += 1;
                }
            }
        }
        let got = &self.got_cages;
        self.room.cages.retain(|c| !got.contains(&(name.clone(), c.x as i32, c.y as i32)));
    }

    fn part(&mut self, p: SpritePart) {
        if self.sprite_parts.len() < 400 {
            self.sprite_parts.push(p);
        }
    }

    fn entity_logic(&mut self, input: &Input) {
        // --- враги ---
        let mut fx: Vec<EnemyFx> = Vec::new();
        for e in self.enemies.iter_mut() {
            e.update(&self.assets, &self.room, &self.player, &mut fx);
        }
        for f in fx {
            match f {
                EnemyFx::Puff(x, y) => {
                    let mut p = SpritePart::new("spr_cloudeffect", x, y);
                    p.anim = 0.4;
                    p.life = 200.0;
                    self.part(p);
                }
                EnemyFx::Shoot(x, y, dir) => self.darts.push(Dart { x, y, dir, frame: 0.0 }),
                EnemyFx::Butter(x, y, dir) => self.butters.push(Butter { x, y, vx: -dir, vy: -7.0, grounded: false, frame: 0.0 }),
            }
        }
        let mut hits: Vec<Hit> = Vec::new();
        interact(&mut self.enemies, &mut self.player, input, &mut hits);
        update_darts(&mut self.darts, &self.room, &self.assets, &mut self.player, &mut hits);
        for h in hits {
            self.on_hit(h);
        }

        // --- hauling: бросок, отпускание, piledriver ---
        if let Some((uid, hh, hv)) = self.player.throw.take() {
            if let Some(e) = self.enemies.iter_mut().find(|e| e.uid == uid) {
                e.state = EState::Thrown;
                e.hit_hsp = hh;
                e.hit_vsp = hv;
            }
            self.audio.sfx("punch", 0.8);
            self.hud.combo.timer = 60.0; // бросок врага продлевает комбо
        }
        if self.player.release {
            self.player.release = false;
            if let Some(uid) = self.player.haul_uid.take() {
                if let Some(e) = self.enemies.iter_mut().find(|e| e.uid == uid) {
                    e.state = EState::Stun;
                    e.scared = 120;
                    e.vsp = -5.0;
                    e.sprite = e.spr().2.to_string();
                }
            }
        }
        if self.player.piledriver_hit {
            self.player.piledriver_hit = false;
            if let Some(uid) = self.player.haul_uid.take() {
                let mut at = None;
                if let Some(e) = self.enemies.iter_mut().find(|e| e.uid == uid) {
                    e.state = EState::Hit;
                    at = Some((e.x, e.y));
                }
                if let Some((x, y)) = at {
                    self.on_hit(Hit::Kill(x, y));
                }
            }
        }
        if let Some(uid) = self.player.haul_uid {
            if !self.enemies.iter().any(|e| e.uid == uid && e.state == EState::Grabbed) {
                self.player.haul_uid = None;
            }
        }
        self.update_thrown();

        let mut i = 0;
        while i < self.enemies.len() {
            if self.enemies[i].dead {
                let e = self.enemies.remove(i);
                self.on_enemy_death(e);
            } else {
                i += 1;
            }
        }

        // --- Flo (obj_tutorialbook): говорит, когда игрок ближе 16 px ---
        let esc = self.escape.active;
        let pbb = self.player.bbox();
        let mut say: Option<String> = None;
        if let Some(m) = self.assets.sprites.get("spr_floidle").cloned() {
            let idle_n = self.assets.frames("spr_floidle") as f32;
            let talk_n = self.assets.frames("spr_flotalk") as f32;
            for b in self.room.books.iter_mut() {
                if b.hide_in_escape && esc {
                    continue;
                }
                let (l, t, r, bt) = inst_bbox(&m, b.x, b.y, 1.0, 1.0);
                let dx = (l - pbb.2).max(pbb.0 - r).max(0.0);
                let dy = (t - pbb.3).max(pbb.1 - bt).max(0.0);
                b.talking = (dx * dx + dy * dy).sqrt() < 16.0 && !self.player.noclip;
                b.frame += 0.35;
                let n = if b.talking { talk_n } else { idle_n };
                if b.frame >= n {
                    b.frame -= n;
                }
                if b.talking && say.is_none() {
                    say = Some(if esc { "[wave]BRO YOU GOTTA LEAVE BRO!".to_string() } else { b.text.clone() });
                }
            }
        }
        match say {
            Some(t) => {
                if self.dialog != t {
                    self.dialog = t;
                    self.dialog_t = 0.0;
                }
                self.dialog_t += 1.5;
                self.dialog_on = true;
            }
            None => {
                self.dialog_on = false;
                self.dialog.clear();
            }
        }

        // --- obj_enemyspawner: выпускает врага при приближении ---
        let (px, py) = (self.player.x, self.player.y);
        let spawner_frames = self.assets.frames("spr_enemyspawner") as f32;
        for i in 0..self.room.spawners.len() {
            let alive = self.enemies.iter().any(|e| e.spawner == Some(i));
            let mut to_spawn: Option<(String, f32, f32, f32)> = None;
            {
                let s = &mut self.room.spawners[i];
                if s.speed > 0.0 {
                    s.frame += s.speed;
                }
                let dist = ((px - s.x).powi(2) + (py - s.y).powi(2)).sqrt();
                if dist <= 300.0 && !alive {
                    if s.speed == 0.0 {
                        s.speed = 0.35;
                    }
                    if s.frame.floor() as i32 == 7 {
                        to_spawn = Some((s.content.clone(), s.x, s.y, s.xscale));
                    }
                } else if s.frame >= spawner_frames {
                    s.frame = 0.0;
                    s.speed = 0.0;
                }
            }
            if let Some((content, x, y, xs)) = to_spawn {
                if let Some(kind) = Kind::from_obj(&content) {
                    let mut e = Enemy::new(usize::MAX, kind, x, y - 50.0, xs, false, false);
                    e.vsp = -5.0;
                    e.state = EState::Stun;
                    e.scared = 100;
                    e.sprite = e.spr().2.to_string();
                    e.spawner = Some(i);
                    self.next_uid += 1;
                    e.uid = self.next_uid;
                    self.enemies.push(e);
                }
            }
        }

        // --- клетки с топпинами ---
        let cage_frames = self.assets.frames("spr_juiceboxcage") as f32;
        for c in self.room.cages.iter_mut() {
            c.frame += 0.35;
            if c.frame >= cage_frames {
                c.frame -= cage_frames;
            }
        }
        let bb = self.player.bbox();
        let cages = std::mem::take(&mut self.room.cages);
        let mut keep = Vec::new();
        for c in cages {
            if overlap(c.rect, bb) && !self.player.noclip {
                self.break_cage(&c);
            } else {
                keep.push(c);
            }
        }
        self.room.cages = keep;
        self.update_toppins();

        // --- спрайтовые частицы и всплывающие тексты ---
        let assets = &self.assets;
        for p in self.sprite_parts.iter_mut() {
            p.x += p.vx;
            p.y += p.vy;
            p.vy += p.gravity;
            p.angle += p.spin;
            p.life -= 1.0;
            if p.anim > 0.0 {
                p.frame += p.anim;
                if p.frame >= assets.frames(&p.spr) as f32 {
                    p.life = 0.0;
                }
            } else if p.anim < 0.0 {
                p.frame += -p.anim;
                let n = assets.frames(&p.spr) as f32;
                if p.frame >= n {
                    p.frame -= n;
                }
            }
        }
        self.sprite_parts.retain(|p| p.life > 0.0);
        for t in self.popups.iter_mut() {
            t.y -= 1.0;
            t.life -= 1;
        }
        self.popups.retain(|t| t.life > 0);
        if self.flash > 0 {
            self.flash -= 1;
        }

        // --- pizza time: таймер ---
        if self.escape.active {
            self.escape.timer -= 1;
            if self.escape.timer <= 1800 && self.escape.timer % 60 == 0 && self.escape.timer > 0 {
                self.audio.sfx("tick", 0.6);
            }
            if self.escape.timer <= 0 && self.pending.is_none() {
                self.toast = Some(("ВРЕМЯ ВЫШЛО! Возврат в лобби".to_string(), 240));
                self.finish_escape();
                self.save_slot();
                let (room, at) = self.return_target();
                self.start_transition(Spawn {
                    room, door: "B".into(), hallway: 0, dir: 1.0,
                    saved_pos: 0.0, saved_spd: 0.0, hallway_size: 1.0, from_climb: false, at,
                });
            }
        }
    }

    /// Брошенные враги летят по прямой: ломают разрушаемые блоки, убивают других врагов,
    /// сами погибают, врезавшись в стену (scr_enemy_grabbed / obj_baddie, state hit)
    fn update_thrown(&mut self) {
        let n = self.enemies.len();
        let mut victims: Vec<usize> = Vec::new();
        let mut broken: Vec<(f32, f32, f32, f32, String)> = Vec::new();
        let (rw, rh) = (self.room.data.width as f32, self.room.data.height as f32);
        for i in 0..n {
            if self.enemies[i].state != EState::Thrown {
                continue;
            }
            let (hh, hv) = (self.enemies[i].hit_hsp, self.enemies[i].hit_vsp);
            let (l, t, r, b) = self.enemies[i].rect();
            let (l, t, r, b) = (l + hh, t + hv, r + hh, b + hv);
            for blk in self.room.blocks.iter_mut() {
                if blk.destructible && blk.hits(l, t, r, b) {
                    blk.alive = false;
                    broken.push((blk.l, blk.t, blk.r, blk.b, blk.obj.clone()));
                }
            }
            let hit_solid = self.room.blocks.iter().any(|k| k.kind == world::BlockKind::Solid && k.hits(l, t, r, b));
            for j in 0..n {
                if j != i && matches!(self.enemies[j].state, EState::Move | EState::Turn | EState::Stun | EState::Shoot | EState::Charge | EState::Wake | EState::Run) {
                    let (el, et, er, eb) = self.enemies[j].rect();
                    if l <= er && r >= el && t <= eb && b >= et {
                        victims.push(j);
                    }
                }
            }
            let e = &mut self.enemies[i];
            if hit_solid || e.x < -100.0 || e.x > rw + 100.0 || e.y < -200.0 || e.y > rh + 100.0 {
                e.dead = true;
            } else {
                e.x += hh;
                e.y += hv;
            }
        }
        for j in victims {
            let (x, y) = (self.enemies[j].x, self.enemies[j].y);
            self.enemies[j].state = EState::Hit;
            self.on_hit(Hit::Kill(x, y));
        }
        if !broken.is_empty() {
            self.audio.sfx("breakblock", 0.6);
            self.player.broken.extend(broken);
        }
    }

    fn popup(&mut self, text: &str, x: f32, y: f32, color: Color) {
        self.popups.push(Popup { text: text.to_string(), x, y, life: 70, color });
    }

    fn on_hit(&mut self, h: Hit) {
        match h {
            Hit::Bump(x, y) => {
                self.audio.sfx("punch", 0.5);
                let mut p = SpritePart::new("spr_bangeffect", x, y);
                p.anim = 0.5;
                p.life = 100.0;
                self.part(p);
            }
            Hit::Bash(x, y) => {
                self.audio.sfx("punch", 0.7);
                for _ in 0..2 {
                    self.slapstar(x, y);
                }
                let mut p = SpritePart::new("spr_bangeffect", x, y);
                p.anim = 0.5;
                p.life = 100.0;
                self.part(p);
            }
            Hit::Kill(x, y) => {
                self.audio.sfx("punch", 0.8);
                self.audio.sfx("killenemy", 0.5);
                self.hitstop = 5;
                let mut p = SpritePart::new("spr_kungfueffect", x, y - 5.0);
                p.anim = 0.4;
                p.life = 100.0;
                self.part(p);
            }
            Hit::Grab(x, y) => {
                self.audio.sfx("punch", 0.5);
                let mut p = SpritePart::new("spr_bangeffect", x, y);
                p.anim = 0.5;
                p.life = 100.0;
                self.part(p);
            }
            Hit::Hurt => {
                // hurt_player(): -100 очков, обломки, эффект удара
                self.collect = (self.collect - 100).max(0);
                // hurt_player: таймер комбо -25, телевизор делает кислое лицо
                self.hud.combo.timer = (self.hud.combo.timer - 25.0).clamp(0.0, 60.0);
                self.hud.tv.anim("spr_tv_hurt", 180);
                let (x, y) = (self.player.x, self.player.y);
                self.popup("-100", x, y - 60.0, Color::from_rgba(255, 90, 90, 255));
                for _ in 0..8 {
                    self.slapstar(x, y);
                    let n = 1 + (self.rnd() * 5.0) as i32 % 5;
                    let mut c = SpritePart::new(&format!("spr_collect{n}"), x, y);
                    c.vx = (self.rnd() - 0.5) * 10.0;
                    c.vy = (self.rnd() - 0.5) * 10.0 - 3.0;
                    c.gravity = 0.4;
                    c.life = 50.0;
                    self.part(c);
                }
                let mut p = SpritePart::new("spr_hurteffect", x, y);
                p.anim = 0.5;
                p.life = 100.0;
                self.part(p);
            }
        }
    }

    fn slapstar(&mut self, x: f32, y: f32) {
        let frame = (self.rnd() * 5.0).floor();
        let mut p = SpritePart::new("spr_slapstar", x, y);
        p.frame = frame;
        p.vx = (self.rnd() - 0.5) * 10.0;
        p.vy = (self.rnd() - 0.5) * 10.0 - 2.0;
        p.gravity = 0.4;
        p.angle = self.rnd() * 6.28;
        p.spin = (self.rnd() - 0.5) * 0.4;
        p.life = 55.0;
        self.part(p);
    }

    /// obj_baddie/Destroy_0: труп улетает, гибсы, звёзды, облако, всплеск
    fn on_enemy_death(&mut self, e: Enemy) {
        let room_h = self.room.data.height as f32;
        if e.y > room_h + 50.0 {
            return; // упал за пределы комнаты - не убит
        }
        // obj_baddie/Destroy_0: таймер комбо = 60; +1 к комбо, если враг не sunny и не выпущен спавнером
        self.hud.combo.timer = 60.0;
        if e.kind != Kind::Sunny && e.spawn_id != usize::MAX {
            self.hud.combo.amt += 1;
        }
        if e.kind != Kind::Sunny && e.spawn_id != usize::MAX {
            self.killed.insert((self.room.data.name.clone(), e.spawn_id, e.escape));
        }
        let away = if e.x - self.player.x >= 0.0 { 1.0 } else { -1.0 };
        let mut corpse = SpritePart::new(e.spr().4, e.x, e.y);
        corpse.vx = (10.0 + self.rnd() * 2.0) * away;
        corpse.vy = -10.0 - self.rnd() * 4.0;
        corpse.gravity = 0.5;
        corpse.anim = -0.35;
        corpse.life = 150.0;
        corpse.scale = 1.0;
        self.part(corpse);
        for _ in 0..3 {
            let frame = (self.rnd() * 12.0).floor();
            let mut g = SpritePart::new("spr_baddie_gibs", e.x, e.y);
            g.frame = frame;
            g.vx = (self.rnd() - 0.5) * 10.0;
            g.vy = (self.rnd() - 0.5) * 10.0 - 3.0;
            g.gravity = 0.4;
            g.angle = self.rnd() * 6.28;
            g.spin = (self.rnd() - 0.5) * 0.4;
            g.life = 60.0;
            self.part(g);
            self.slapstar(e.x, e.y);
        }
        for (spr, speed) in [("spr_genericpoofeffect", 0.4), ("spr_bangeffect", 0.5)] {
            let mut p = SpritePart::new(spr, e.x, e.y);
            p.anim = speed;
            p.life = 100.0;
            self.part(p);
        }
        if e.kind == Kind::Slosher {
            let mut p = SpritePart::new(if e.plosher { "spr_plosher_splat" } else { "spr_slosher_splat" }, e.x, e.y);
            p.anim = 0.35;
            p.life = 150.0;
            self.part(p);
        }
    }

    // ---------------- топпины ----------------

    fn toppin_kind(content: &str) -> String {
        content.trim_start_matches("obj_potable").to_lowercase()
    }

    /// obj_potable_cage/Collision_obj_player: клетка разбивается, топпин +1000
    fn break_cage(&mut self, c: &world::Cage) {
        self.got_cages.insert((self.room.data.name.clone(), c.x as i32, c.y as i32));
        self.audio.sfx("cagebreak", 0.7);
        self.audio.sfx("toppin", 0.6);
        self.collect += 1000;
        self.save_slot();
        self.hud.combo.timer = 60.0;
        self.hud.tv.anim("spr_tv_happy", 180);
        self.popup("+1000", c.x, c.y - 50.0, Color::from_rgba(255, 230, 90, 255));
        let kind = Self::toppin_kind(&c.content);
        self.toppins.push(Toppin { kind, x: c.x, y: c.y, xs: 1.0, index: 0.0, intro: true, moving: false });
        // обломки клетки (spr_juiceboxcage_debri)
        let n = self.assets.frames("spr_juiceboxcage_debri") as f32;
        for _ in 0..10 {
            let frame = (self.rnd() * n).floor();
            let mut d = SpritePart::new("spr_juiceboxcage_debri", c.x, c.y);
            d.frame = frame;
            d.vx = (self.rnd() - 0.5) * 10.0;
            d.vy = (self.rnd() - 0.5) * 10.0 - 2.0;
            d.gravity = 0.4;
            d.angle = self.rnd() * 6.28;
            d.spin = (self.rnd() - 0.5) * 0.4;
            d.life = 70.0;
            self.part(d);
        }
        // конфетти
        let palette = [(255, 90, 90), (90, 200, 255), (255, 220, 80), (120, 255, 140), (220, 120, 255)];
        for _ in 0..30 {
            let col = palette[(self.rnd() * palette.len() as f32) as usize % palette.len()];
            let (vx, vy) = ((self.rnd() - 0.5) * 12.0, -2.0 - self.rnd() * 9.0);
            let (size, rotv, life) = (4.0 + self.rnd() * 3.0, (self.rnd() - 0.5) * 0.6, 60.0 + self.rnd() * 40.0);
            let rot = self.rnd() * 6.28;
            self.particles.push(Particle {
                x: c.x, y: c.y, vx, vy, size, rot, rotv, life, fade: 25.0,
                color: Color::from_rgba(col.0, col.1, col.2, 255), puff: false, frame: 0.0, gravity: 0.25,
            });
        }
    }

    fn toppin_sprite(&self, t: &Toppin) -> String {
        let k = &t.kind;
        let has = |n: &str| self.assets.sprites.contains_key(n);
        let intro = format!("spr_potable{k}_intro");
        let idle = format!("spr_potable{k}_idle");
        let walk = if has(&format!("spr_potable{k}_move")) { format!("spr_potable{k}_move") } else { format!("spr_potable{k}_walk") };
        let panic = format!("spr_potable{k}_idlepanic");
        if t.intro && has(&intro) {
            intro
        } else if t.moving && has(&walk) {
            walk
        } else if self.escape.active && has(&panic) {
            panic
        } else {
            idle
        }
    }

    /// Топпины идут по следу игрока с задержкой (follower)
    fn update_toppins(&mut self) {
        self.history.push((self.player.x, self.player.y));
        if self.history.len() > 600 {
            self.history.remove(0);
        }
        let len = self.history.len();
        for i in 0..self.toppins.len() {
            let delay = 26 * (i + 1);
            let (tx, ty) = self.history[len.saturating_sub(delay + 1)];
            let spr = self.toppin_sprite(&self.toppins[i]);
            let frames = self.assets.frames(&spr) as f32;
            let t = &mut self.toppins[i];
            let (dx, dy) = (tx - t.x, ty - t.y);
            t.moving = dx.abs() > 0.5 || dy.abs() > 0.5;
            if dx.abs() > 0.5 {
                t.xs = if dx > 0.0 { 1.0 } else { -1.0 };
            }
            t.x = tx;
            t.y = ty;
            t.index += 0.35;
            if t.intro && t.index >= frames {
                t.intro = false;
                t.index = 0.0;
            } else if t.index >= frames {
                t.index -= frames;
            }
        }
    }

    // ---------------- pizza time ----------------

    /// obj_monolith/Destroy_0: монолит разрушен - начинается побег (3:30 для уровня forest)
    fn monolith_destroyed(&mut self, l: f32, t: f32, r: f32, b: f32) {
        if self.escape.active {
            return;
        }
        // куски монолита (spr_mugmonolith_dead) разлетаются
        let n = self.assets.frames("spr_mugmonolith_dead");
        let away = if (l + r) / 2.0 - self.player.x >= 0.0 { 1.0 } else { -1.0 };
        for i in 0..n {
            let mut p = SpritePart::new("spr_mugmonolith_dead", (l + r) / 2.0, (t + b) / 2.0);
            p.frame = i as f32;
            p.vx = (3.0 + self.rnd() * 2.0) * away;
            p.vy = -10.0 - self.rnd() * 4.0;
            p.gravity = 0.5;
            p.life = 150.0;
            self.part(p);
        }
        // global.escape.timer: tutorial -> 3:00 (по умолчанию), forest (wayback) -> 3:30
        let total = if self.room.data.name.starts_with("tutorial") { 60 * 60 * 3 } else { 60 * 60 * 3 + 60 * 30 };
        self.escape = Escape { active: true, timer: total, total };
        self.flash = 40;
        self.hitstop = 8;
        self.audio.sfx("escape_start", 0.9);
        self.toast = Some(("THE FIRST OF MANY - PIZZA TIME! Беги к выходу в wayback_1".to_string(), 300));
        self.update_room_audio();
        self.refresh_block_modes();
        // escape-враги текущей комнаты появляются сразу
        let name = self.room.data.name.clone();
        for (i, sp) in self.room.enemy_spawns.iter().enumerate() {
            if !sp.escape || self.enemies.iter().any(|e| e.escape && e.spawn_id == i) {
                continue;
            }
            if let Some(kind) = Kind::from_obj(&sp.obj) {
                if kind == Kind::Sunny || !self.killed.contains(&(name.clone(), i, true)) {
                    self.next_uid += 1;
                    let mut e = Enemy::new(i, kind, sp.x, sp.y, sp.xscale, sp.plosher, true);
                    e.uid = self.next_uid;
                    self.enemies.push(e);
                }
            }
        }
    }

    /// Куда вернуться после побега: к воротам, через которые вошли (иначе лобби)
    fn return_target(&self) -> (String, Option<(f32, f32)>) {
        match &self.return_to {
            Some((room, x, y)) => (room.clone(), Some((*x, *y))),
            None => ("hotel_1".to_string(), None),
        }
    }

    /// obj_monolithblock_wake/sleep и obj_tutorialBlock включаются/выключаются по состоянию побега
    fn refresh_block_modes(&mut self) {
        let (esc, done) = (self.escape.active, self.tutorial_done);
        for b in self.room.blocks.iter_mut() {
            b.enabled = match b.mode {
                1 => !esc,
                2 => esc,
                3 => !done,
                _ => true,
            };
        }
    }

    fn finish_escape(&mut self) {
        if self.escape.timer > 0 && self.room.data.name.starts_with("tutorial") {
            self.tutorial_done = true; // дошли до выхода - туториал пройден
        }
        self.escape.active = false;
        self.taken.retain(|k| !k.2);
        self.killed.retain(|k| !k.2);
        self.refresh_block_modes();
    }

    // ---------------- отрисовка ----------------

    fn draw(&self) {
        if self.mode == Mode::Menu {
            self.draw_menu();
            return;
        }
        clear_background(Color::from_rgba(12, 10, 20, 255));
        let v = self.view_size();
        let view = Rect::new(self.cam.x - v.x / 2.0, self.cam.y - v.y / 2.0, v.x, v.y);
        // округляем камеру до пикселя игры - без "дрожания" тайлов
        let view = Rect::new(view.x.round(), view.y.round(), v.x, v.y);
        // Ось Y вниз, как в GameMaker: отрицательный zoom.y задаём явно,
        // потому что знак в Camera2D::from_display_rect зависит от версии macroquad.
        // оттенок комнаты: эффект-слой "Tint" (в Waybackyard голубоватый #99CAFF)
        self.assets.set_tint(self.room.data.tint.as_ref().map(|t| t.rgb).unwrap_or([1.0; 3]));
        // шейдер побега: мир рисуется в текстуру, затем накладывается эффект
        let escape_fx = self.escape.active && self.opts.screen_effects && self.escape_mat.is_some() && self.add_mat.is_some();
        if !self.escape.active {
            self.bleed.set(0.0);
        }
        let mut cam = Camera2D {
            target: vec2(view.x + view.w / 2.0, view.y + view.h / 2.0),
            zoom: vec2(2.0 / view.w, 2.0 / view.h),
            ..Default::default()
        };
        if escape_fx {
            let (sw, sh) = (screen_width().max(1.0) as u32, screen_height().max(1.0) as u32);
            let mut rt = self.rt.borrow_mut();
            let stale = rt.as_ref().map(|r| (r.1, r.2)) != Some((sw, sh));
            if stale {
                let target = render_target(sw, sh);
                target.texture.set_filter(FilterMode::Linear);
                *rt = Some((target, sw, sh));
            }
            cam.render_target = rt.as_ref().map(|r| r.0.clone());
        }
        set_camera(&cam);
        if escape_fx {
            clear_background(Color::from_rgba(12, 10, 20, 255));
        }

        for (_, layer) in &self.layers {
            match layer {
                Layer::Bg(_) | Layer::Tiles(_) | Layer::Decor(_) if self.no_tex => {}
                Layer::Bg(i) => self.draw_background(&self.room.data.backgrounds[*i], view),
                Layer::Tiles(i) => self.assets.draw_tile_layer(&self.room.data.tiles[*i], view),
                Layer::Decor(i) => {
                    let d = &self.room.decor[*i];
                    if d.dead {
                        continue;
                    }
                    if let Some(bi) = d.block {
                        if !self.room.blocks[bi].alive {
                            continue;
                        }
                    }
                    let esc = self.escape.active;
                    let hidden = match d.mode {
                        1 => esc,
                        2 => !esc,
                        3 => self.tutorial_done,
                        _ => false,
                    } || (d.hide_in_escape && esc);
                    if hidden {
                        continue;
                    }
                    if let Some(layers) = &d.gate {
                        // ворота: фон внутри силуэта (кадр 1), поверх - арка (кадр 0)
                        if let (Some(tex), Some(gm)) = (self.assets.gate_texture(&d.sprite, layers), self.assets.sprites.get(&d.sprite)) {
                            draw_texture(&tex, d.x - gm.xo as f32, d.y - gm.yo as f32, self.assets.tint_color(1.0));
                        }
                        self.assets.draw_sprite(&d.sprite, 0.0, d.x, d.y, 1.0, 1.0, 1.0);
                    } else if d.tiled {
                        for ty in 0..(d.sy.abs() as i32) {
                            for tx in 0..(d.sx.abs() as i32) {
                                self.assets.draw_sprite(&d.sprite, d.frame, d.x + tx as f32 * 32.0, d.y + ty as f32 * 32.0, 1.0, 1.0, 1.0);
                            }
                        }
                    } else {
                        self.assets.draw_sprite(&d.sprite, d.frame, d.x, d.y, d.sx, d.sy, 1.0);
                    }
                }
                Layer::Books => {
                    for b in &self.room.books {
                        if b.hide_in_escape && self.escape.active {
                            continue;
                        }
                        let spr = if b.talking { "spr_flotalk" } else { "spr_floidle" };
                        self.assets.draw_sprite(spr, b.frame, b.x, b.y, 1.0, 1.0, 1.0);
                    }
                }
                Layer::Spawners => {
                    for sp in &self.room.spawners {
                        self.assets.draw_sprite("spr_enemyspawner", sp.frame, sp.x, sp.y, sp.xscale, 1.0, 1.0);
                    }
                }
                Layer::Pickups => {
                    self.draw_rock();
                    for c in &self.room.cages {
                        self.assets.draw_sprite("spr_juiceboxcage", c.frame, c.x, c.y, 1.0, 1.0, 1.0);
                    }
                    for p in self.room.pickups.iter().filter(|p| p.alive) {
                        // обычные коллекты в pizza time не показываются
                        if !p.escape && self.escape.active {
                            continue;
                        }
                        if let Some(meta) = self.assets.sprites.get(&p.sprite) {
                            let (l0, t0, r0, b0) = inst_bbox(meta, p.x0, p.y0, 1.0, 1.0);
                            let hidden = self.room.blocks.iter().any(|s| {
                                s.kind == world::BlockKind::Solid && s.hits(l0, t0, r0, b0)
                            });
                            if hidden {
                                continue;
                            }
                        }
                        let a = if p.escape && !self.escape.active { 0.5 } else { 1.0 };
                        self.assets.draw_sprite(&p.sprite, p.frame, p.x, p.y, 1.0, 1.0, a);
                    }
                }
                Layer::Enemies => {
                    for e in &self.enemies {
                        // враг, получивший смертельный удар, дрожит (hitstun)
                        let jitter = if e.state == EState::Hit { ((self.tick * 7) % 11 - 5) as f32 } else { 0.0 };
                        self.assets.draw_sprite(&e.sprite, e.index, e.x + jitter, e.y, e.xscale, 1.0, 1.0);
                    }
                    for d in &self.darts {
                        self.assets.draw_sprite("spr_cocoantDart", d.frame, d.x, d.y, d.dir, 1.0, 1.0);
                    }
                }
                Layer::Toppins => {
                    self.draw_paintbuddy();
                    for t in &self.toppins {
                        let spr = self.toppin_sprite(t);
                        self.assets.draw_sprite(&spr, t.index, t.x, t.y, t.xs, 1.0, 1.0);
                    }
                }
                Layer::Player => {
                    self.player.draw(&self.assets, self.palette);
                    self.draw_uparrow();
                }
            }
        }

        self.draw_particles();
        if self.debug {
            self.draw_hitboxes(view);
        }
        for p in &self.sprite_parts {
            let a = (p.life / 20.0).clamp(0.0, 1.0);
            if p.angle != 0.0 {
                self.assets.draw_sprite_rot(&p.spr, p.frame, p.x, p.y, p.scale, a, p.angle);
            } else {
                self.assets.draw_sprite(&p.spr, p.frame, p.x, p.y, p.scale, p.scale, a);
            }
        }
        for t in &self.popups {
            let a = (t.life as f32 / 20.0).clamp(0.0, 1.0);
            draw_text_f(&t.text, t.x - 28.0 + 2.0, t.y + 2.0, 36.0, Color::new(0.0, 0.0, 0.0, a));
            draw_text_f(&t.text, t.x - 28.0, t.y, 36.0, Color::new(t.color.r, t.color.g, t.color.b, a));
        }

        self.assets.set_tint([1.0; 3]);
        if escape_fx {
            self.escape_pass();
        }

        // ---- HUD: телевизор и комбо (координаты 960x540, якорь справа сверху) ----
        {
            let hud_w = view.w;
            set_camera(&Camera2D {
                target: vec2(hud_w / 2.0, 270.0),
                zoom: vec2(2.0 / hud_w, 2.0 / 540.0),
                ..Default::default()
            });
            self.hud.draw(&self.assets, self.palette, hud_w);
            if let Some((txt, x, y, a)) = self.hud.lost_points_label() {
                let w = measure_text_f(&txt, 28).width;
                draw_text_f(&txt, x - w / 2.0, y, 28.0, Color::new(1.0, 1.0, 1.0, a));
            }
        }

        // ---- экранные элементы ----
        set_default_camera();
        let alpha = match self.phase {
            Phase::FadeOut(n) => n as f32 / FADE_FRAMES as f32,
            Phase::FadeIn(n) => 1.0 - n as f32 / FADE_FRAMES as f32,
            Phase::Playing => 0.0,
        };
        if alpha > 0.0 {
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, alpha.clamp(0.0, 1.0)));
        }
        if self.dialog_on {
            self.draw_dialog();
        }
        if self.flash > 0 {
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(1.0, 1.0, 1.0, self.flash as f32 / 40.0));
        }
        draw_text_f(&format!("SCORE {}", self.collect), 16.0, 30.0, 32.0, WHITE);
        if self.escape.active {
            let t = self.escape.timer.max(0);
            let secs = t / 60;
            let txt = format!("{}:{:02}.{:02}", secs / 60, secs % 60, (t % 60) * 100 / 60);
            let hurry = t < 1800 && (self.tick / 10) % 2 == 0;
            let col = if hurry { RED } else { Color::from_rgba(255, 170, 40, 255) };
            let w = measure_text_f(&txt, 56).width;
            draw_text_f("PIZZA TIME!", screen_width() / 2.0 - 90.0, 34.0, 36.0, col);
            draw_text_f(&txt, screen_width() / 2.0 - w / 2.0, 82.0, 56.0, col);
            let (bw, bx) = (screen_width() * 0.5, screen_width() * 0.25);
            draw_rectangle(bx, 94.0, bw, 10.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle(bx, 94.0, bw * (t as f32 / self.escape.total.max(1) as f32), 10.0, col);
        }
        if !self.toppins.is_empty() {
            draw_text_f(&format!("ТОППИНЫ {}/5", self.toppins.len()), 16.0, 78.0, 22.0, Color::from_rgba(255, 230, 90, 255));
        }
        draw_text_f(
            &format!("{}   {:?}  spd {:.1}", self.room.data.name, self.player.state, self.player.movespeed),
            16.0, screen_height() - 14.0, 20.0, Color::new(1.0, 1.0, 1.0, 0.6),
        );
        draw_text_f(
            "←→ ходьба · Z прыжок · Shift бег · X захват/бросок · ↓ присесть/в воздухе pound · ↑ дверь/лестница/superjump(на mach3) · Esc пауза · V ноклип · P палитра · R рестарт · F3 хитбоксы · F4 без текстур",
            16.0, 54.0, 18.0, Color::new(1.0, 1.0, 1.0, 0.6),
        );
        if self.player.noclip {
            draw_text_f("NOCLIP", screen_width() - 130.0, 30.0, 28.0, Color::new(0.4, 1.0, 0.6, 1.0));
        }
        if let Some((msg, _)) = &self.toast {
            let w = measure_text_f(msg, 24).width;
            draw_rectangle(screen_width() / 2.0 - w / 2.0 - 12.0, screen_height() - 110.0, w + 24.0, 40.0, Color::new(0.0, 0.0, 0.0, 0.75));
            draw_text_f(msg, screen_width() / 2.0 - w / 2.0, screen_height() - 82.0, 24.0, YELLOW);
        }

        self.draw_titlecard();

        // ---- пауза и настройки поверх всего (координаты 960x540) ----
        if self.pause.active || self.opt_menu.is_some() {
            set_camera(&Camera2D { target: vec2(480.0, 270.0), zoom: vec2(2.0 / 960.0, 2.0 / 540.0), ..Default::default() });
            self.pause.draw(&self.assets, self.wave.as_ref());
            if let Some(om) = &self.opt_menu {
                om.draw(&self.opts);
            }
            set_default_camera();
        }
    }

    fn draw_background(&self, bg: &assets::Background, view: Rect) {
        let Some(meta) = self.assets.sprites.get(&bg.sprite) else { return };
        let (w, h) = (meta.w as f32, meta.h as f32);
        let ox = bg.x + bg.hspeed * self.tick as f32;
        let oy = bg.y + bg.vspeed * self.tick as f32;
        let x_start = if bg.htiled { ox + ((view.x - ox) / w).floor() * w } else { ox };
        let y_start = if bg.vtiled { oy + ((view.y - oy) / h).floor() * h } else { oy };
        let mut y = y_start;
        loop {
            let mut x = x_start;
            loop {
                self.assets.draw_sprite(&bg.sprite, 0.0, x + meta.xo as f32, y + meta.yo as f32, 1.0, 1.0, 1.0);
                x += w;
                if !bg.htiled || x > view.x + view.w {
                    break;
                }
            }
            y += h;
            if !bg.vtiled || y > view.y + view.h {
                break;
            }
        }
    }
}

fn read_input(latch: &mut Latch) -> Input {
    if is_key_pressed(KeyCode::Z) || is_key_pressed(KeyCode::Space) {
        latch.jump = true;
    }
    if is_key_pressed(KeyCode::X) {
        latch.grab = true;
    }
    if is_key_pressed(KeyCode::LeftShift) || is_key_pressed(KeyCode::RightShift) {
        latch.attack = true;
    }
    if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
        latch.down = true;
    }
    Input {
        left: is_key_down(KeyCode::Left) || is_key_down(KeyCode::A),
        right: is_key_down(KeyCode::Right) || is_key_down(KeyCode::D),
        up: is_key_down(KeyCode::Up) || is_key_down(KeyCode::W),
        down: is_key_down(KeyCode::Down) || is_key_down(KeyCode::S),
        jump: is_key_down(KeyCode::Z) || is_key_down(KeyCode::Space),
        jump_pressed: false, // выставляются в шаге из защёлок
        attack: is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
        attack_pressed: false,
        grab: is_key_down(KeyCode::X),
        grab_pressed: false,
        down_pressed: false,
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    if let Ok(font) = load_ttf_font(&format!("{ASSETS_DIR}/fonts/DejaVuSans-Bold.ttf")).await {
        FONT.with(|f| *f.borrow_mut() = Some(font));
    }
    let audio = Audio::load(ASSETS_DIR).await;
    let mut game = Game::new(audio);
    game.build_palette();
    game.apply_options();
    game.enter_menu(); // игра начинается с главного меню
    let mut acc = 0.0f32;
    let mut mlatch = MenuInput::default();

    loop {
        if is_key_pressed(KeyCode::F3) {
            game.debug = !game.debug;
        }
        if is_key_pressed(KeyCode::F5) {
            game.rt_flip = !game.rt_flip; // если эффект побега перевёрнут - нажмите F5
        }
        if is_key_pressed(KeyCode::F4) {
            game.no_tex = !game.no_tex;
        }
        // нажатия для меню/паузы запоминаются до ближайшего шага логики
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            mlatch.up = true;
        }
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            mlatch.down = true;
        }
        if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
            mlatch.left = true;
        }
        if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
            mlatch.right = true;
        }
        if is_key_pressed(KeyCode::Z) || is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Enter) {
            mlatch.jump = true;
        }
        if is_key_pressed(KeyCode::X) {
            mlatch.grab = true;
        }
        if is_key_pressed(KeyCode::Escape) {
            mlatch.start = true;
        }
        if is_key_pressed(KeyCode::T) {
            mlatch.taunt = true;
        }

        if game.mode == Mode::Play && !game.paused_ui() {
            if is_key_pressed(KeyCode::P) {
                game.palette = (game.palette + 1) % 12;
                game.toast = Some((format!("Палитра {}", game.palette), 90));
                game.save_slot();
            }
            if is_key_pressed(KeyCode::R) {
                // перезапуск с последнего сохранения
                let n = game.slot;
                game = Game::carry_over(&mut game);
                game.start_slot(n);
            }
            if is_key_pressed(KeyCode::V) {
                game.toggle_noclip();
            }
        }

        let mut input = read_input(&mut game.latch);
        acc += get_frame_time().min(0.1);
        while acc >= STEP {
            acc -= STEP;
            input.jump_pressed = game.latch.jump;
            input.grab_pressed = game.latch.grab;
            input.down_pressed = game.latch.down;
            input.attack_pressed = game.latch.attack;
            game.latch = Latch::default();
            let mi = mlatch;
            mlatch = MenuInput::default();
            match game.mode {
                Mode::Menu => {
                    if let Some(req) = game.step_menu(&mi) {
                        match req {
                            MenuRequest::Start(n) => {
                                game = Game::carry_over(&mut game);
                                game.start_slot(n);
                            }
                            MenuRequest::Options => game.opt_menu = Some(OptionsMenu::new()),
                            MenuRequest::Quit => std::process::exit(0),
                        }
                    }
                }
                Mode::Play => {
                    if game.paused_ui() {
                        game.step_pause(&mi);
                    } else {
                        game.step(input);
                        if mi.start && game.can_pause() {
                            game.open_pause();
                        }
                    }
                }
            }
        }

        game.draw();
        if game.debug && game.mode == Mode::Play {
            let (l, t, r, b) = game.player.bbox();
            draw_text_f(
                &format!(
                    "x {:.0} y {:.0} hsp {:.1} vsp {:.1} grounded {} coyote {}  mask [{:.0},{:.0},{:.0},{:.0}]  fps {}",
                    game.player.x, game.player.y, game.player.hsp, game.player.vsp,
                    game.player.grounded, game.player.coyote_time, l, t, r, b, get_fps()
                ),
                16.0, 80.0, 20.0, GREEN,
            );
            draw_text_f(
                &format!("state {:?}  sprite {}  idx {:.1}  movespeed {:.1}  palette {}  [F4 - скрыть текстуры {}]", game.player.state, game.player.sprite, game.player.index, game.player.movespeed, game.palette, if game.no_tex { "ВКЛ" } else { "выкл" }),
                16.0, 102.0, 18.0, GREEN,
            );
        }
        // кадр для паузы снимается после полной отрисовки
        if game.pause.want_capture {
            game.capture_pause();
        }
        next_frame().await;
    }
}
