//! Интерфейс: телевизор (obj_hud.tv) и комбо (global.combo, obj_hud.combometer, obj_comboTitle).
//! Порт obj_hud/Step_0 + Draw_64, obj_player/Step_0 (таймер и потеря комбо), obj_comboTitle.

use crate::assets::Assets;
use crate::player::State;
use macroquad::prelude::*;

/// scr_math/wave()
pub fn wave(from: f32, to: f32, duration: f32, offset: f32) -> f32 {
    let a4 = (to - from) * 0.5;
    let t = get_time() as f32 * 1000.0;
    from + a4 + ((((t - offset) * 0.001) + duration) / duration * std::f32::consts::TAU).sin() * a4
}

fn approach(start: f32, end: f32, inc: f32) -> f32 {
    if start < end { (start + inc).min(end) } else { (start - inc).max(end) }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// global.combo
pub struct Combo {
    pub amt: i32,
    /// 60 = полный, убывает на 0.15 за кадр
    pub timer: f32,
    pub dropped: bool,
    previous: i32,
    pub highest: i32,
}

pub struct Tv {
    moving: bool,
    sprite: String,
    index: f32,
    speed: f32,
    expression: String,
    timer: i32,
    switch_index: f32,
    offset: f32,
    idle: i32,
}

impl Tv {
    /// tv_anim(): сменить выражение телевизора (с переключением "помехи")
    pub fn anim(&mut self, sprite: &str, timer: i32) {
        if self.expression != sprite {
            self.switch_index = 0.0;
            self.moving = true;
            self.expression = sprite.to_string();
            self.timer = timer;
        }
    }
}

struct Light {
    off: bool,
    x: f32,
    y: f32,
    index: f32,
}

struct Meter {
    x: f32,
    y: f32,
    arrowx: f32,
    vsp: f32,
    state: i32,
    lights: [Light; 4],
    shake: f32,
    shown: i32,
}

/// obj_comboTitle: плашка "COMBO!" / "COMBO LOST"
struct Title {
    x: f32,
    y: f32,
    scale: f32,
    alpha: f32,
    fade: bool,
    lost: bool,
    very: bool,
    combo: i32,
    index: f32,
    points: i32,
    life: i32,
}

impl Title {
    fn new(hud_w: f32, amt: i32, lost: bool, points: i32) -> Title {
        let combo = amt / 5;
        Title {
            x: hud_w - 149.0,
            y: 350.0,
            scale: 1.0,
            alpha: 1.0,
            fade: false,
            lost,
            very: combo > 15,
            combo: combo % 15,
            index: ((combo % 15) * 2) as f32,
            points,
            life: 120,
        }
    }
}

/// Данные из игры, нужные HUD на каждом шаге
pub struct Ctx<'a> {
    pub player_state: State,
    pub movespeed: f32,
    pub escape: bool,
    pub room: &'a str,
    pub player_x: f32,
    pub player_y: f32,
    pub room_w: f32,
    /// таймер комбо стоит на месте (вход в дверь, переход между комнатами)
    pub freeze_timer: bool,
    pub hud_w: f32,
}

pub struct Hud {
    pub tv: Tv,
    meter: Meter,
    pub combo: Combo,
    title: Option<Title>,
    pub visible: bool,
    rng: u32,
}

impl Hud {
    pub fn new() -> Hud {
        let light = || Light { off: false, x: 0.0, y: 0.0, index: 0.0 };
        Hud {
            tv: Tv {
                moving: false,
                sprite: "spr_tv_off".into(),
                index: 0.0,
                speed: 0.35,
                expression: "spr_tv_idle".into(),
                timer: 0,
                switch_index: 0.0,
                offset: 0.0,
                idle: 100,
            },
            meter: Meter {
                x: 811.0,
                y: 237.0,
                arrowx: 811.0,
                vsp: 0.0,
                state: -1,
                lights: [light(), light(), light(), light()],
                shake: 0.0,
                shown: 0,
            },
            combo: Combo { amt: 0, timer: 60.0, dropped: false, previous: 0, highest: 0 },
            title: None,
            visible: false,
            rng: 0xC0FF_EE11,
        }
    }

    fn irand(&mut self, lo: i32, hi: i32) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (lo + (self.rng % (hi - lo + 1) as u32) as i32) as f32
    }

    /// obj_hud.hudVisible(): скрыт в хабе (без побега) и в туториале
    fn hud_visible(room: &str, escape: bool) -> bool {
        if room.starts_with("tutorial") {
            return false;
        }
        if room.starts_with("hotel") {
            return escape;
        }
        true
    }

    /// Возвращает очки, начисленные за потерянное комбо
    pub fn update(&mut self, a: &Assets, c: &Ctx) -> i32 {
        self.visible = Self::hud_visible(c.room, c.escape);
        let mut gained = 0;

        // ---------- комбо (obj_player/Step_0) ----------
        if !c.freeze_timer {
            self.combo.timer = (self.combo.timer - 0.15).max(0.0);
        }
        if self.combo.previous != self.combo.amt {
            self.combo.previous = self.combo.amt;
            if self.combo.amt > 0 && self.combo.amt % 5 == 0 {
                self.title = Some(Title::new(c.hud_w, self.combo.amt, false, 0));
            }
        }
        if self.combo.timer == 0.0 && self.combo.amt > 0 {
            let save = self.combo.amt;
            self.combo.highest = self.combo.highest.max(save);
            self.combo.amt = 0;
            self.combo.dropped = true;
            // в GML здесь стоит `savecombo ^ 2` - это побитовый XOR, а не возведение в квадрат; порт сохраняет поведение
            let points = (((save ^ 2) as f32) * 0.25 + 10.0 * save as f32).round() as i32;
            let mut t = Title::new(c.hud_w, save, true, points);
            t.lost = true;
            self.title = Some(t);
            gained = points;
        }

        // ---------- плашка комбо (obj_comboTitle/Step_0) ----------
        let amt = self.combo.amt;
        let meter_y = self.meter.y;
        let mut remove = false;
        if let Some(t) = self.title.as_mut() {
            if t.y > 240.0 && !(t.lost && amt != 0) {
                t.y = meter_y + 110.0;
            } else if t.lost && amt != 0 {
                t.x = lerp(t.x, c.hud_w - 310.0, 0.25);
                t.y = lerp(t.y, 240.0, 0.25);
            }
            t.scale = lerp(t.scale, 1.0, 0.2);
            t.life -= 1;
            if t.life <= 0 {
                t.fade = true;
            }
            if t.fade {
                t.alpha = lerp(t.alpha, 0.0, 0.2);
                if t.alpha < 0.1 {
                    remove = true;
                }
            }
            t.index += 0.25;
            let frame = (t.combo * 2) as f32;
            if t.index >= frame + 2.0 {
                t.index = frame;
            }
        }
        if remove {
            self.title = None;
        }

        // ---------- комбо-метр (obj_hud/Draw_64: with combometer) ----------
        let perc = self.combo.timer / 60.0;
        let base_x = c.hud_w - 149.0;
        {
            let (state, amt) = (self.meter.state, self.combo.amt);
            let (r2, r5) = (self.irand(-2, 2), self.irand(-5, 5));
            let m = &mut self.meter;
            match state {
                -1 => {
                    m.x = base_x + wave(-5.0, 5.0, 2.0, 0.0);
                    m.y = approach(m.y, -128.0, 5.0);
                    if amt > 0 {
                        m.state = 0;
                        m.vsp = 15.0;
                    }
                }
                0 => {
                    m.x = base_x + wave(-5.0, 5.0, 2.0, 0.0);
                    m.vsp += 0.5;
                    m.y += m.vsp;
                    if m.y > 210.0 {
                        m.state = 1;
                    }
                }
                _ => {
                    if perc <= 0.5 && perc > 0.2 {
                        m.x = base_x + wave(-5.0, 5.0, 2.0, 0.0) + r2;
                        m.vsp += 0.3;
                        m.y += m.vsp;
                        if m.y > 200.0 {
                            m.y = 200.0;
                            m.vsp = -2.0;
                        }
                    } else if perc <= 0.2 {
                        m.x = base_x + wave(-5.0, 5.0, 1.0, 0.0) + r5;
                        m.vsp += 0.3;
                        m.y += m.vsp;
                        if m.y > 200.0 {
                            m.y = 200.0;
                            m.vsp = -4.0;
                        }
                    } else {
                        m.x = base_x + wave(-5.0, 5.0, 2.0, 0.0);
                        m.y = lerp(m.y, 200.0, 0.1);
                    }
                    if amt == 0 {
                        m.state = -1;
                    }
                }
            }
            // лампочки: гаснут справа налево по мере падения таймера
            for i in 0..4 {
                let yoff = if (i + 1) % 2 == 0 { -5.0 } else { 5.0 };
                let lx = m.x - 72.0 + 45.0 * i as f32;
                let off = m.lights[i].off;
                if m.arrowx < lx && !off {
                    m.lights[i].off = true;
                } else if m.arrowx >= lx && off {
                    m.lights[i].off = false;
                }
                m.lights[i].x = lx;
                m.lights[i].y = m.y - 5.0 + wave(yoff, -yoff, 5.0, 0.0);
                m.lights[i].index += 0.1;
            }
            let start_pos = m.lights[3].x + 15.0;
            let end_pos = m.lights[0].x - 5.0;
            m.arrowx = lerp(m.arrowx, end_pos + (start_pos - end_pos) * perc, 0.25);
            if m.shown != amt {
                m.shown = amt;
                m.shake = 6.0;
            }
            m.shake = approach(m.shake, 0.0, 6.0 / 10.0);
        }
        let jitter: Vec<f32> = (0..4).map(|_| self.irand(-1, 1)).collect();
        for (i, l) in self.meter.lights.iter_mut().enumerate() {
            if l.off {
                l.y += jitter[i];
            }
        }

        // ---------- телевизор (obj_hud/Step_0: with tv) ----------
        self.update_tv(a, c);
        gained
    }

    fn update_tv(&mut self, a: &Assets, c: &Ctx) {
        let secret = c.room.contains("secret");
        let mut idle_spr = "spr_tv_idle";
        if secret {
            idle_spr = "spr_tv_secret";
        }
        if c.escape && !secret {
            idle_spr = "spr_tv_escape";
        }
        if self.combo.amt >= 10 && !secret {
            idle_spr = "spr_tv_combo";
        }
        if self.combo.amt >= 50 && !secret {
            idle_spr = "spr_tv_heat";
        }
        let tv = &mut self.tv;
        if !self.visible {
            tv.moving = false;
            tv.sprite = "spr_tv_off".into();
            tv.index = 0.0;
            return;
        }
        // телевизор уезжает вверх, когда игрок у правого края (чтобы не мешал)
        if c.player_x > c.room_w - 270.0 && c.player_y < 320.0 {
            tv.offset = approach(tv.offset, 400.0, 100.0);
        } else {
            tv.offset = approach(tv.offset, 0.0, 100.0);
        }

        tv.index += tv.speed;
        if tv.moving {
            let last = a.frames("spr_tv_switch") as f32 - 1.0;
            tv.switch_index = approach(tv.switch_index, last, 0.35);
            if tv.switch_index >= last {
                tv.sprite = tv.expression.clone();
                tv.switch_index = 0.0;
                tv.moving = false;
                tv.speed = 0.35;
            }
        } else {
            match tv.sprite.clone().as_str() {
                "spr_tv_off" => {
                    tv.index = 0.0;
                    tv.sprite = "spr_tv_turnon".into();
                }
                "spr_tv_turnon" => {
                    if tv.index >= a.frames("spr_tv_turnon") as f32 {
                        tv.idle = 60 * 5;
                        tv.index = 0.0;
                        tv.sprite = idle_spr.into();
                    }
                }
                "spr_tv_hurt" | "spr_tv_happy" => {
                    tv.timer -= 1;
                    if tv.timer <= 0 {
                        tv.anim(idle_spr, 0);
                    }
                }
                "spr_tv_mach" | "spr_tv_crazyrun" => {
                    if !matches!(c.player_state, State::Mach3 | State::ClimbWall | State::Buzzsaw) {
                        tv.anim(idle_spr, 0);
                    }
                    let mach4 = c.movespeed >= 16.0;
                    if mach4 && tv.expression == "spr_tv_mach" {
                        tv.anim("spr_tv_crazyrun", 0);
                    }
                    if !mach4 && tv.expression == "spr_tv_crazyrun" {
                        tv.anim("spr_tv_mach", 0);
                    }
                }
                "spr_tv_secret" | "spr_tv_party" | "spr_tv_escape" | "spr_tv_idle" | "spr_tv_combo" | "spr_tv_heat"
                | "spr_tv_ski" | "spr_tv_idle_anim1" => {
                    tv.idle -= 1;
                    if tv.expression != idle_spr {
                        tv.anim(idle_spr, 0);
                    }
                    if c.player_state == State::Mach3 {
                        tv.anim("spr_tv_mach", 0);
                    }
                    if tv.sprite == "spr_tv_idle" && tv.idle <= 0 {
                        tv.sprite = "spr_tv_idle_anim1".into();
                        tv.index = 0.0;
                    }
                    if tv.sprite == "spr_tv_idle_anim1" && tv.index >= a.frames("spr_tv_idle_anim1") as f32 {
                        tv.sprite = "spr_tv_idle".into();
                        tv.index = 0.0;
                        tv.idle = 60 * 5;
                    }
                }
                _ => {}
            }
        }
        // зацикливаем анимацию (анимации turnon/idle_anim1 уже обработаны выше)
        let n = a.frames(&tv.sprite) as f32;
        if tv.index >= n {
            tv.index %= n;
        }
    }

    // ------------------------------------------------------------------
    //  Отрисовка (координаты 960x540, якорь - правый верхний угол)
    // ------------------------------------------------------------------
    pub fn draw(&self, a: &Assets, palette_index: i32, hud_w: f32) {
        if !self.visible {
            return;
        }
        let off = self.tv.offset;
        self.draw_meter(a, off);
        self.draw_tv(a, palette_index, hud_w, off);
        self.draw_title(a, hud_w);
    }

    fn draw_tv(&self, a: &Assets, palette_index: i32, hud_w: f32, off: f32) {
        let tv = &self.tv;
        let (x, y) = (hud_w - 132.0, 73.0 - off);
        if tv.sprite != "spr_tv_turnon" {
            a.draw_sprite("spr_tv_bg", 0.0, x, y, 1.0, 1.0, 1.0);
        }
        // выражение - в цветах палитры игрока
        a.draw_sprite_pal(&tv.sprite, tv.index, x, y, 1.0, 1.0, 1.0, "spr_playerPal", palette_index as f32);
        if tv.moving {
            a.draw_sprite("spr_tv_switch", tv.switch_index, x, y, 1.0, 1.0, 1.0);
        }
        // корпус телевизора - в цветах spr_tv_palette (столбец character + 1; Mildred = 0)
        if tv.sprite != "spr_tv_turnon" {
            a.draw_sprite_pal("spr_tv_empty", 0.0, x, y, 1.0, 1.0, 1.0, "spr_tv_palette", 1.0);
        } else {
            a.draw_sprite_pal("spr_tv_turnon", tv.index, x, y, 1.0, 1.0, 1.0, "spr_tv_palette", 1.0);
        }
    }

    /// Строка цифрами из спрайтового шрифта font_combo ("1234567890" -> кадры 0..9)
    fn digit_frame(ch: char) -> usize {
        match ch {
            '1'..='9' => ch as usize - '1' as usize,
            _ => 9,
        }
    }

    fn draw_meter(&self, a: &Assets, off: f32) {
        let m = &self.meter;
        a.draw_sprite("spr_combo_meter", 1.0, m.x, m.y - off, 1.0, 1.0, 1.0);
        let dropped = if self.combo.dropped { 1.0 } else { 0.0 };
        for l in &m.lights {
            let spr = if l.off { "spr_comboLightsOff" } else { "spr_comboLights" };
            let n = a.frames(spr) as f32;
            a.draw_sprite_pal(spr, l.index % n.max(1.0), l.x, l.y - off, 1.0, 1.0, 1.0, "spr_comboPalette", dropped);
        }
        a.draw_sprite("spr_combo_arrow", 0.0, m.arrowx, m.y - 34.0 - off, 1.0, 1.0, 1.0);
        a.draw_sprite("spr_combo_meter", 0.0, m.x, m.y - off, 1.0, 1.0, 1.0);

        // число комбо: каждая цифра чуть ниже предыдущей и дрожит при изменении
        let text = self.combo.amt.to_string();
        let widths: Vec<(f32, f32)> = text
            .chars()
            .map(|c| a.glyph_bounds("font_combo", Self::digit_frame(c)))
            .collect();
        let total: f32 = widths.iter().map(|(lo, hi)| hi - lo).sum();
        let mut xx = m.x - 9.0 - total;
        let mut yy = m.y + 32.0 - off;
        let mut seed = (get_time() * 1000.0) as u32 ^ 0x9E37_79B9;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 1000) as f32 / 1000.0
        };
        if let Some(meta) = a.sprites.get("font_combo") {
            for (ch, (lo, hi)) in text.chars().zip(widths.iter()) {
                let (sx, sy) = ((rnd() * 2.0 - 1.0) * m.shake, (rnd() * 2.0 - 1.0) * m.shake);
                // верхний левый угол кадра - в (xx - lo, yy), как у шрифта из спрайта
                a.draw_sprite(
                    "font_combo",
                    Self::digit_frame(ch) as f32,
                    xx - lo + meta.xo as f32 + sx,
                    yy + meta.yo as f32 + sy,
                    1.0,
                    1.0,
                    1.0,
                );
                xx += hi - lo;
                yy += 2.0;
            }
        }
    }

    fn draw_title(&self, a: &Assets, _hud_w: f32) {
        let Some(t) = &self.title else { return };
        a.draw_sprite("spr_combo_titles", t.index, t.x, t.y, t.scale, t.scale, t.alpha);
        if t.very {
            a.draw_sprite("spr_combomucho", t.index, t.x - 64.0, t.y - 32.0, t.scale, t.scale, t.alpha);
        }
        if t.lost {
            a.draw_sprite("spr_comboend", t.index, t.x, t.y - 32.0, t.scale, t.scale, t.alpha);
            // очки за потерянное комбо рисует Game (шрифт DejaVu)
        }
    }

    /// Параметры подписи очков под плашкой потерянного комбо: (текст, x, y, прозрачность)
    pub fn lost_points_label(&self) -> Option<(String, f32, f32, f32)> {
        self.title.as_ref().filter(|t| t.lost).map(|t| (t.points.to_string(), t.x, t.y + 32.0, t.alpha))
    }
}
