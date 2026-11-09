//! Пауза (obj_pause), главное меню (obj_mainmenu) и настройки. Размер экрана меню - 960x540.

use crate::assets::Assets;
use crate::save::{Options, Slot};
use macroquad::prelude::*;

#[derive(Default, Clone, Copy)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub grab: bool,
    pub start: bool,
    pub taunt: bool,
}

fn approach(start: f32, end: f32, inc: f32) -> f32 {
    if start < end { (start + inc).min(end) } else { (start - inc).max(end) }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Текст с тенью (draw_text_oyh)
fn text_shadow(text: &str, x: f32, y: f32, size: f32, color: Color) {
    crate::draw_text_f(text, x + 2.0, y + 2.0, size, Color::new(0.0, 0.0, 0.0, color.a));
    crate::draw_text_f(text, x, y, size, color);
}

fn text_center(text: &str, cx: f32, y: f32, size: f32, color: Color) {
    let w = crate::measure_text_f(text, size as u16).width;
    text_shadow(text, cx - w / 2.0, y, size, color);
}

// ======================================================================
//  ПАУЗА
// ======================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseAction {
    Resume,
    Options,
    Restart,
    Exit,
}

struct Heart {
    x: f32,
    y: f32,
    index: f32,
    alpha: f32,
}

struct PauseOpt {
    label: &'static str,
    action: PauseAction,
    offset_x: f32,
    icon: usize,
    icon_alpha: f32,
}

pub struct Pause {
    pub active: bool,
    /// Game должен сделать снимок экрана и вызвать activate()
    pub want_capture: bool,
    pub screen: Option<Texture2D>,
    selected: usize,
    options: Vec<PauseOpt>,
    backdrop: f32,
    bx: f32,
    by: f32,
    border: f32,
    wave_time: f32,
    hearts: Vec<Heart>,
    heart_timer: i32,
    in_level: bool,
    secrets: [bool; 3],
    secret_shown: [bool; 3],
    secret_scale: [f32; 3],
    treasure: bool,
    treasure_alpha: f32,
    buffer: i32,
    rng: u32,
}

impl Pause {
    pub fn new() -> Pause {
        Pause {
            active: false,
            want_capture: false,
            screen: None,
            selected: 0,
            options: Vec::new(),
            backdrop: 0.0,
            bx: 0.0,
            by: 0.0,
            border: 2.0,
            wave_time: 0.0,
            hearts: Vec::new(),
            heart_timer: 60,
            in_level: false,
            secrets: [false; 3],
            secret_shown: [false; 3],
            secret_scale: [1.0; 3],
            treasure: false,
            treasure_alpha: 0.0,
            buffer: 0,
            rng: 0xBADC_0DED,
        }
    }

    fn rnd(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    /// doPause(): пункты зависят от того, в уровне ли игрок (global.level != noone)
    pub fn open(&mut self, in_level: bool, secrets: [bool; 3], treasure: bool) {
        self.in_level = in_level;
        self.secrets = secrets;
        self.secret_shown = [false; 3];
        self.treasure = treasure;
        self.selected = 0;
        self.backdrop = 0.0;
        self.border = 2.0;
        self.buffer = 0;
        self.hearts.clear();
        self.heart_timer = 60;
        let off = -960.0;
        self.options = vec![
            PauseOpt { label: "RESUME", action: PauseAction::Resume, offset_x: off, icon: 0, icon_alpha: 0.0 },
            PauseOpt { label: "OPTIONS", action: PauseAction::Options, offset_x: off, icon: 1, icon_alpha: 0.0 },
        ];
        if in_level {
            self.options.push(PauseOpt { label: "RESTART", action: PauseAction::Restart, offset_x: off, icon: 2, icon_alpha: 0.0 });
        }
        self.options.push(PauseOpt {
            label: if in_level { "EXIT LEVEL" } else { "MAIN MENU" },
            action: PauseAction::Exit,
            offset_x: off,
            icon: 3,
            icon_alpha: 0.0,
        });
        self.want_capture = true;
        self.active = false;
    }

    pub fn activate(&mut self, screen: Texture2D) {
        self.screen = Some(screen);
        self.want_capture = false;
        self.active = true;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.want_capture = false;
        self.screen = None;
    }

    /// Возвращает выбранное действие и звуки
    pub fn update(&mut self, inp: &MenuInput) -> (Option<PauseAction>, Vec<&'static str>) {
        let mut sounds = Vec::new();
        if !self.active {
            return (None, sounds);
        }
        self.backdrop = approach(self.backdrop, 0.6, 0.06);
        self.bx -= 0.1;
        self.by -= 0.1;
        self.wave_time += 1.0;
        self.border = lerp(self.border, 1.0, 0.3);
        self.treasure_alpha = approach(self.treasure_alpha, 1.0, 0.1);

        let mut result = None;
        if self.buffer == 0 {
            let before = self.selected;
            if inp.down {
                self.selected += 1;
            }
            if inp.up && self.selected > 0 {
                self.selected -= 1;
            }
            self.selected = self.selected.min(self.options.len() - 1);
            if before != self.selected {
                sounds.push("menu_move");
            }
            if inp.grab || inp.start {
                sounds.push("menu_back");
                result = Some(PauseAction::Resume);
            } else if inp.jump {
                sounds.push("menu_select");
                result = Some(self.options[self.selected].action);
                self.buffer = 5;
            }
        }
        self.buffer = (self.buffer - 1).max(0);

        for (i, o) in self.options.iter_mut().enumerate() {
            o.offset_x = lerp(o.offset_x, 0.0, 0.2);
            o.icon_alpha = approach(o.icon_alpha, if i == self.selected { 1.0 } else { 0.0 }, 0.1);
        }

        // сердечки на фоне (alarm[0] = 100)
        self.heart_timer -= 1;
        if self.heart_timer <= 0 {
            self.heart_timer = 100;
            let x = 50.0 + self.rnd() * 860.0;
            self.hearts.push(Heart { x, y: 540.0, index: 0.0, alpha: 0.0 });
        }
        for h in self.hearts.iter_mut() {
            h.y -= 0.5;
            h.index += 0.04;
            if h.y < 270.0 - 32.0 {
                h.alpha -= 0.01;
            } else {
                h.alpha = approach(h.alpha, 1.0, 0.05);
            }
        }
        self.hearts.retain(|h| !(h.y < 270.0 - 32.0 && h.alpha <= 0.0));

        // найденные секреты "загораются" с увеличением
        for i in 0..3 {
            if self.border < 1.05 && self.secret_shown[i] != self.secrets[i] {
                self.secret_scale[i] = 2.0;
                self.secret_shown[i] = self.secrets[i];
            }
            self.secret_scale[i] = approach(self.secret_scale[i], 1.0, 0.1);
        }
        (result, sounds)
    }

    pub fn draw(&self, a: &Assets, wave: Option<&Material>) {
        if !self.active {
            return;
        }
        if let Some(tex) = &self.screen {
            if let Some(m) = wave {
                m.set_uniform("time", self.wave_time);
                m.set_uniform("frequency", 0.01f32);
                m.set_uniform("amplitude", 0.009f32);
                gl_use_material(m);
            }
            draw_texture_ex(tex, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(960.0, 540.0)), ..Default::default() });
            if wave.is_some() {
                gl_use_default_material();
            }
        }
        // затемнение с бегущим узором spr_pause_bg
        let (tw, th) = (300.0, 300.0);
        let mut y = self.by.rem_euclid(th) - th;
        while y < 540.0 {
            let mut x = self.bx.rem_euclid(tw) - tw;
            while x < 960.0 {
                a.draw_sprite("spr_pause_bg", 0.0, x, y, 1.0, 1.0, self.backdrop);
                x += tw;
            }
            y += th;
        }
        for h in &self.hearts {
            let n = a.frames("spr_pause_heart_fly") as f32;
            a.draw_sprite("spr_pause_heart_fly", h.index % n.max(1.0), h.x, h.y, 1.0, 1.0, h.alpha.clamp(0.0, 1.0));
        }
        // пункты меню
        let xx = 480.0;
        let mut yy = 270.0 - 52.0 * (self.options.len() as f32 - 1.0) / 2.0;
        for (i, o) in self.options.iter().enumerate() {
            let sel = i == self.selected;
            let col = if sel { WHITE } else { Color::new(0.6, 0.6, 0.6, 1.0) };
            let w = crate::measure_text_f(o.label, 38).width;
            text_center(o.label, xx + o.offset_x, yy + 30.0, 38.0, col);
            let jx = ((get_time() * 60.0) as i32 % 3 - 1) as f32;
            let jy = (((get_time() * 60.0) as i32 / 3) % 3 - 1) as f32;
            a.draw_sprite(
                "spr_pauseicons",
                o.icon as f32,
                (xx + jx - w / 2.0 - 80.0).floor() + o.offset_x + 42.0,
                (yy + jy).floor() + 20.0,
                1.0,
                1.0,
                o.icon_alpha,
            );
            yy += 52.0;
        }
        a.draw_sprite("spr_pauseBorder", 0.0, 480.0, 270.0, self.border, self.border, 1.0);
        if self.in_level {
            let pos = [(42.0, 114.0), (43.0, 43.0), (95.0, 72.0)];
            for i in 0..3 {
                let spr = if self.secret_shown[i] { "spr_pauseHeart_full" } else { "spr_pauseHeart_empty" };
                let (mut x, mut y) = pos[i];
                if !self.secret_shown[i] {
                    x += ((get_time() * 60.0) as i32 % 3 - 1) as f32;
                    y += (((get_time() * 60.0) as i32 / 3) % 3 - 1) as f32;
                }
                a.draw_sprite(spr, i as f32, x, y, self.secret_scale[i], self.secret_scale[i], (self.backdrop / 0.4).clamp(0.0, 1.0));
            }
            a.draw_sprite("spr_treasure_pause", if self.treasure { 0.0 } else { 1.0 }, 864.0, 462.0, 1.0, 1.0, self.treasure_alpha);
        }
    }
}

// ======================================================================
//  НАСТРОЙКИ (упрощённый obj_option: громкости и полный экран)
// ======================================================================

pub struct OptionsMenu {
    selected: usize,
}

const OPT_ROWS: [&str; 7] = ["MASTER VOLUME", "MUSIC VOLUME", "SFX VOLUME", "AMBIANCE VOLUME", "FULLSCREEN", "SCREEN EFFECTS", "BACK"];

impl OptionsMenu {
    pub fn new() -> OptionsMenu {
        OptionsMenu { selected: 0 }
    }

    /// (настройки изменились, закрыть меню, звуки)
    pub fn update(&mut self, o: &mut Options, inp: &MenuInput) -> (bool, bool, Vec<&'static str>) {
        let mut sounds = Vec::new();
        let mut changed = false;
        if inp.down && self.selected + 1 < OPT_ROWS.len() {
            self.selected += 1;
            sounds.push("menu_move");
        }
        if inp.up && self.selected > 0 {
            self.selected -= 1;
            sounds.push("menu_move");
        }
        let dir = inp.right as i32 - inp.left as i32;
        if dir != 0 {
            let slot = match self.selected {
                0 => Some(&mut o.master),
                1 => Some(&mut o.music),
                2 => Some(&mut o.sfx),
                3 => Some(&mut o.ambience),
                _ => None,
            };
            if let Some(v) = slot {
                let nv = (*v + dir).clamp(0, 10);
                if nv != *v {
                    *v = nv;
                    changed = true;
                    sounds.push("menu_move");
                }
            } else if self.selected == 4 {
                o.fullscreen = !o.fullscreen;
                changed = true;
                sounds.push("menu_select");
            } else if self.selected == 5 {
                o.screen_effects = !o.screen_effects;
                changed = true;
                sounds.push("menu_select");
            }
        }
        let mut close = false;
        if inp.jump {
            match self.selected {
                4 => {
                    o.fullscreen = !o.fullscreen;
                    changed = true;
                    sounds.push("menu_select");
                }
                5 => {
                    o.screen_effects = !o.screen_effects;
                    changed = true;
                    sounds.push("menu_select");
                }
                6 => close = true,
                _ => {}
            }
        }
        if inp.grab || inp.start {
            close = true;
        }
        if close {
            sounds.push("menu_back");
        }
        (changed, close, sounds)
    }

    pub fn draw(&self, o: &Options) {
        draw_rectangle(0.0, 0.0, 960.0, 540.0, Color::new(0.04, 0.02, 0.08, 0.88));
        text_center("OPTIONS", 480.0, 80.0, 52.0, WHITE);
        let vals = [o.master, o.music, o.sfx, o.ambience];
        for (i, label) in OPT_ROWS.iter().enumerate() {
            let y = 150.0 + i as f32 * 48.0;
            let sel = i == self.selected;
            let col = if sel { WHITE } else { Color::new(0.6, 0.6, 0.6, 1.0) };
            if i < 4 {
                text_shadow(label, 190.0, y, 30.0, col);
                for k in 0..10 {
                    let c = if k < vals[i] { Color::from_rgba(255, 190, 60, 255) } else { Color::new(0.25, 0.2, 0.3, 1.0) };
                    draw_rectangle(560.0 + k as f32 * 20.0, y - 22.0, 16.0, 26.0, c);
                }
                if sel {
                    text_shadow("<", 530.0, y, 30.0, col);
                    text_shadow(">", 770.0, y, 30.0, col);
                }
            } else if i == 4 || i == 5 {
                text_shadow(label, 190.0, y, 30.0, col);
                let on = if i == 4 { o.fullscreen } else { o.screen_effects };
                text_shadow(if on { "ON" } else { "OFF" }, 560.0, y, 30.0, col);
            } else {
                text_center(label, 480.0, y, 30.0, col);
            }
        }
        text_center("↑↓ выбор · ←→ изменить · Z подтвердить · X назад", 480.0, 500.0, 20.0, Color::new(1.0, 1.0, 1.0, 0.6));
    }
}

// ======================================================================
//  ГЛАВНОЕ МЕНЮ
// ======================================================================

struct Tape {
    x: f32,
    y: f32,
    save_x: f32,
    save_y: f32,
    state: i32,
    alpha: f32,
    face_alpha: f32,
    status_alpha: f32,
    face_index: f32,
    completion: f32,
    created: bool,
}

impl Tape {
    fn new(i: usize, slot: &Slot) -> Tape {
        let (x, y) = match i {
            1 => (272.0, 239.0),
            2 => (168.0, 246.0),
            _ => (374.0, 232.0),
        };
        let completion = slot.completion();
        Tape {
            x, y, save_x: x, save_y: y, state: 0, alpha: 1.0, face_alpha: 0.0, status_alpha: 0.0,
            face_index: (completion.round() / 100.0) * 7.0,
            completion,
            created: slot.created,
        }
    }
}

pub enum MenuRequest {
    Start(usize),
    Options,
    Quit,
}

pub struct Menus {
    pub scene: i32,
    pub selected: i32,
    pub slots: Vec<Slot>,
    tapes: Vec<Tape>,
    room_pal: f32,
    fade_alpha: f32,
    dark: bool,
    hud_alpha: f32,
    tv_sprite: &'static str,
    tv_back: Option<&'static str>,
    tv_index: f32,
    alarm0: i32,
    alarm1: i32,
    alarm5: i32,
    select_buffer: i32,
    quit_sel: i32,
    quit_text: &'static str,
    byebye: f32,
    tick: i32,
}

const QUIT_TEXTS: [&str; 6] = [
    "ARE YOU SURE YOU WANT TO QUIT?",
    "DONT LEAVE ME! IM SORRY!",
    "YOULL BE BACK, RIGHT?",
    "PLEASE DONT LEAAVE..",
    "I LOVE YOU THOUGH!",
    "DONT QUIT PLEASE!",
];

impl Menus {
    pub fn new() -> Menus {
        let slots: Vec<Slot> = (0..3).map(Slot::load).collect();
        let tapes = slots.iter().enumerate().map(|(i, s)| Tape::new(i, s)).collect();
        Menus {
            scene: -2,
            selected: 0,
            slots,
            tapes,
            room_pal: 0.0,
            fade_alpha: 1.0,
            dark: true,
            hud_alpha: 0.0,
            tv_sprite: "spr_mainmenu_tv_off",
            tv_back: None,
            tv_index: 0.0,
            alarm0: 0,
            alarm1: 0,
            alarm5: -1,
            select_buffer: 0,
            quit_sel: 0,
            quit_text: QUIT_TEXTS[0],
            byebye: 0.0,
            tick: 0,
        }
    }

    fn rebuild_tape(&mut self, i: usize) {
        self.tapes[i] = Tape::new(i, &self.slots[i]);
    }

    pub fn update(&mut self, inp: &MenuInput) -> (Option<MenuRequest>, Vec<&'static str>) {
        let mut snd: Vec<&'static str> = Vec::new();
        self.tick += 1;
        self.fade_alpha = approach(self.fade_alpha, if self.scene == 1 && self.alarm1 < 40 { 1.0 } else { 0.0 }, 0.1);
        self.room_pal = approach(self.room_pal, (self.selected + 1) as f32, 1.0 / 3.0);
        self.tv_index += 0.35;

        match self.scene {
            -2 => {
                if inp.jump {
                    self.dark = false;
                    self.scene = -1;
                    self.alarm0 = 60;
                    snd.push("menu_select");
                }
            }
            -1 => {
                self.alarm0 -= 1;
                if self.alarm0 <= 0 {
                    self.scene = 0;
                    self.selected = 0;
                    self.tv_sprite = "spr_mainmenu_tvStatic";
                    snd.push("tvstatic");
                    if self.slots[0].created {
                        self.alarm5 = 6;
                    }
                }
            }
            0 => {
                if inp.start {
                    return (Some(MenuRequest::Options), snd);
                }
                self.hud_alpha = approach(self.hud_alpha, 1.0, 0.1);
                self.select_buffer = (self.select_buffer - 1).max(0);
                if inp.jump && self.select_buffer == 0 {
                    self.scene = 1;
                    self.alarm1 = 180;
                    self.tv_sprite = "spr_mainmenu_tv_empty";
                    self.tv_back = Some("spr_mainmenu_tv1_pick");
                    self.tv_index = 0.0;
                    snd.push("menu_select");
                    return (None, snd);
                }
                // файлы лежат справа налево: стрелка вправо - предыдущий слот
                let mv = inp.left as i32 - inp.right as i32;
                if mv != 0 {
                    let before = self.selected;
                    self.selected = (self.selected + mv).clamp(0, 2);
                    if before != self.selected {
                        snd.push("menu_move");
                        snd.push("tvstatic");
                    }
                    self.tv_sprite = "spr_mainmenu_tvStatic";
                    self.tv_back = None;
                    self.alarm5 = if self.slots[self.selected as usize].created { 6 } else { -1 };
                }
                let sel = self.selected;
                for (i, t) in self.tapes.iter_mut().enumerate() {
                    let on = sel == i as i32 && t.created;
                    t.face_alpha = approach(t.face_alpha, if on { 1.0 } else { 0.0 }, 0.1);
                    t.status_alpha = approach(t.status_alpha, if on { 1.0 } else { 0.0 }, 0.1);
                    let mut target = t.save_x + 80.0;
                    if sel != i as i32 {
                        t.state = 0;
                    } else {
                        if t.state == 0 {
                            t.state = 1;
                        }
                        if t.state == 1 {
                            target = t.save_x + 130.0;
                            if t.x >= t.save_x + 100.0 {
                                t.state = 2;
                            }
                        }
                        if t.state == 2 {
                            target = t.save_x + 95.0;
                        }
                    }
                    t.x = lerp(t.x, if sel == i as i32 { target } else { t.save_x }, 0.25);
                    t.y = lerp(t.y, if sel == i as i32 { t.save_y - 5.0 } else { t.save_y }, 0.25);
                }
                if inp.grab {
                    self.scene = 3;
                    self.quit_sel = 0;
                    self.quit_text = QUIT_TEXTS[(self.tick as usize / 7) % QUIT_TEXTS.len()];
                    snd.push("menu_select");
                }
                if inp.taunt {
                    // удалить выбранный файл
                    let i = self.selected as usize;
                    Slot::delete(i);
                    self.slots[i] = Slot::default();
                    self.rebuild_tape(i);
                    self.tv_sprite = "spr_mainmenu_tvStatic";
                    self.tv_back = None;
                    snd.push("menu_back");
                    snd.push("tvstatic");
                }
            }
            1 => {
                self.hud_alpha = approach(self.hud_alpha, 0.0, 0.15);
                let sel = self.selected;
                for (i, t) in self.tapes.iter_mut().enumerate() {
                    if sel != i as i32 {
                        t.alpha = approach(t.alpha, 0.0, 0.05);
                        t.x = t.save_x + ((self.tick * 7 + i as i32 * 3) % 3 - 1) as f32;
                        t.y = t.save_y + ((self.tick * 5 + i as i32) % 3 - 1) as f32;
                    }
                }
                self.alarm1 -= 1;
                if self.alarm1 <= 0 {
                    return (Some(MenuRequest::Start(self.selected as usize)), snd);
                }
            }
            3 => {
                if inp.left {
                    self.quit_sel = 1;
                    snd.push("menu_move");
                }
                if inp.right {
                    self.quit_sel = 0;
                    snd.push("menu_move");
                }
                self.byebye += 0.05;
                if inp.grab {
                    self.scene = 0;
                    snd.push("menu_back");
                }
                if inp.jump {
                    if self.quit_sel == 1 {
                        return (Some(MenuRequest::Quit), snd);
                    }
                    self.scene = 0;
                    snd.push("menu_back");
                }
            }
            _ => {}
        }
        // alarm[5]: через 6 кадров после выбора существующего файла телевизор показывает игрока
        if self.alarm5 > 0 {
            self.alarm5 -= 1;
            if self.alarm5 == 0 && self.scene == 0 {
                self.tv_sprite = "spr_mainmenu_tv";
                self.tv_back = Some("spr_mainmenu_tv1_idle");
            }
        }
        (None, snd)
    }

    fn frame(a: &Assets, spr: &str, idx: f32) -> f32 {
        let n = a.frames(spr) as f32;
        idx % n.max(1.0)
    }

    /// Палитры: pal_mainmenu_room (комната, дробный индекс) и spr_playerPal (игрок на телевизоре)
    pub fn draw(&self, a: &Assets) {
        clear_background(BLACK);
        let rp = self.room_pal;
        a.draw_sprite_pal("spr_mainmenu_tvBackground", 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, "pal_mainmenu_room", rp);
        if let Some(back) = self.tv_back {
            let palette = self.slots[self.selected.clamp(0, 2) as usize].palette as f32;
            a.draw_sprite_pal(back, Self::frame(a, back, self.tv_index), 768.0, 288.0, 1.0, 1.0, 1.0, "spr_playerPal", palette);
        }
        a.draw_sprite_pal(self.tv_sprite, Self::frame(a, self.tv_sprite, self.tv_index), 768.0, 288.0, 1.0, 1.0, 1.0, "pal_mainmenu_room", rp);
        a.draw_sprite_pal("spr_mainmenu_shelf", 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, "pal_mainmenu_room", rp);

        for (i, t) in self.tapes.iter().enumerate() {
            a.draw_sprite_pal("spr_mainmenu_movies", i as f32, t.x, t.y, 1.0, 1.0, t.alpha, "pal_mainmenu_room", rp);
            if self.scene != 2 {
                a.draw_sprite("spr_mainmenu_tomatoface", t.face_index, t.x + 63.0 - 42.0, t.y + 83.0 + 24.0, 1.0, 1.0, t.face_alpha * self.hud_alpha);
                let (xx, yy) = (t.x + 63.0, t.y + 83.0);
                a.draw_sprite("spr_tapestatus", 0.0, xx, yy - 10.0, 1.0, 1.0, t.status_alpha * self.hud_alpha);
                crate::draw_text_f(
                    &format!("{}%", t.completion.round()),
                    xx - 12.0,
                    yy + 14.0,
                    26.0,
                    Color::new(1.0, 1.0, 1.0, t.status_alpha * self.hud_alpha),
                );
            }
        }

        // подсказки по кнопкам
        let ha = self.hud_alpha;
        a.draw_sprite("spr_controls", 0.0, 843.0, 77.0, 1.0, 1.0, ha);
        a.draw_sprite("spr_quitgame", 0.0, 0.0, 0.0, 1.0, 1.0, ha);
        a.draw_sprite("spr_deletefile", 0.0, 832.0, 448.0, 1.0, 1.0, ha);
        let c = Color::new(1.0, 1.0, 1.0, ha);
        text_shadow("ESC", 843.0 - 64.0 - 24.0, 40.0, 22.0, c);
        text_shadow("X", 75.0, 56.0, 24.0, c);
        text_shadow("T", 768.0 + 3.0, 456.0, 24.0, c);
        let w = crate::measure_text_f("Rust port", 18).width;
        crate::draw_text_f("Rust port", 960.0 - 32.0 - w, 540.0 - 32.0, 18.0, Color::new(1.0, 1.0, 1.0, ha));

        if self.scene == 3 {
            draw_rectangle(0.0, 0.0, 960.0, 540.0, Color::new(0.0, 0.0, 0.0, 0.5));
            let f = self.byebye % 2.0;
            a.draw_sprite("spr_menu_byebye", f, 480.0 - 224.0, 270.0, 1.0, 1.0, 1.0);
            a.draw_sprite("spr_menu_byebye", f, 480.0 + 224.0, 270.0, 1.0, 1.0, 1.0);
            text_center(self.quit_text, 480.0, 270.0 - 60.0, 36.0, WHITE);
            let yes = if self.quit_sel == 1 { RED } else { MAROON };
            let no = if self.quit_sel == 0 { WHITE } else { GRAY };
            text_center("YES", 480.0 - 64.0, 280.0, 36.0, yes);
            text_center("NO", 480.0 + 64.0, 280.0, 36.0, no);
        }
        if self.dark {
            a.draw_sprite("spr_mainmenu_dark", 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
            if (self.tick / 30) % 2 == 0 {
                text_center("PRESS Z TO START", 480.0, 470.0, 34.0, WHITE);
            }
        }
        if self.fade_alpha > 0.0 {
            draw_rectangle(0.0, 0.0, 960.0, 540.0, Color::new(0.0, 0.0, 0.0, self.fade_alpha.clamp(0.0, 1.0)));
        }
    }
}
