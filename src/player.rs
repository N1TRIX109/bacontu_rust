//! Игрок. Физика и состояния портированы 1:1 из:
//!   scr_collision, scr_solid, scr_slope_collideCheck,
//!   scr_player_normal / jump / crouch / mach2 / mach3 / machslide / machturn /
//!   hitwall / climbwall, slope_momentum, scr_player_enterdoor/walkfront (упрощённо).
//! Все числа (grav 0.5, прыжок -11, mach 12/16/20 и т.д.) взяты из исходных GML-скриптов.

use crate::assets::Assets;
use crate::world::{Block, BlockKind, Room};

mod moves;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Normal,
    Jump,
    Crouch,
    Mach2,
    Mach3,
    MachSlide,
    MachTurn,
    HitWall,
    ClimbWall,
    GroundPound,
    FreefallLand,
    Grab,
    Hauling,
    FinishingBlow,
    Uppercut,
    SuperJumpPrep,
    SuperJump,
    Buzzsaw,
    Ladder,
    /// поскользнулся на масле (Rock Bottom)
    Slip,
    /// Скольжение/кувырок на бегу (↓ во время mach), в оригинале states.tumble
    Tumble,
    EnterDoor,
    WalkFront,
    /// получил урон (states.hurt)
    Hurt,
}

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub jump: bool,
    pub jump_pressed: bool,
    pub attack: bool,
    pub attack_pressed: bool,
    pub grab: bool,
    pub grab_pressed: bool,
    pub down_pressed: bool,
}

impl Input {
    /// key_right + key_left
    pub fn move_x(&self) -> f32 {
        (self.right as i32 - self.left as i32) as f32
    }
}

/// GML sign()
fn sign(v: f32) -> f32 {
    if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 }
}

/// GML approach() из scr_math
fn approach(start: f32, end: f32, inc: f32) -> f32 {
    if start < end { (start + inc).min(end) } else { (start - inc).max(end) }
}

/// GML round(): банковское округление (0.5 -> 0, 1.5 -> 2)
fn gm_round(v: f32) -> i32 {
    if (v - v.trunc()).abs() == 0.5 {
        (2.0 * (v / 2.0).round()) as i32
    } else {
        v.round() as i32
    }
}

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub hsp: f32,
    pub vsp: f32,
    pub grav: f32,
    pub movespeed: f32,
    pub vertical_movespeed: f32,
    pub xscale: f32,
    pub grounded: bool,
    pub coyote_time: i32,
    pub jumpstop: bool,
    pub jump_buffer: i32,
    pub freefalling: i32,
    pub state: State,
    pub sprite: String,
    pub index: f32,
    pub speed: f32,
    pub crouch_mask: bool,
    pub idle_timer: i32,
    pub step_timer: i32,
    pub wallgrab: i32,
    /// V: свободный полёт сквозь стены
    pub noclip: bool,
    /// Звуковые события за кадр (читает main.rs)
    pub events: Vec<&'static str>,
    /// Разрушенные за кадр блоки (l, t, r, b, имя объекта) - для частиц
    pub broken: Vec<(f32, f32, f32, f32, String)>,
    pub crouchslip: i32,
    /// кадры неуязвимости после урона
    pub i_frame: i32,
    pub slap_buffer: i32,
    pub down_buffer: i32,
    pub freefallsmash: i32,
    /// uid удерживаемого врага (hauling)
    pub haul_uid: Option<u64>,
    /// бросок: (uid врага, hitHsp, hitVsp) - читает Game
    pub throw: Option<(u64, f32, f32)>,
    /// piledriver приземлился: Game убивает удерживаемого врага
    pub piledriver_hit: bool,
    /// игрок отпустил врага (присел с hauling)
    pub release: bool,
    rng: u32,
    /// Для переходов между комнатами (читает game.rs)
    pub enter_target: Option<(String, String)>,
}

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Player {
            x, y,
            hsp: 0.0, vsp: 0.0, grav: 0.5,
            movespeed: 0.0, vertical_movespeed: 0.0,
            xscale: 1.0,
            grounded: false, coyote_time: 0,
            jumpstop: false, jump_buffer: 0, freefalling: 0,
            state: State::Normal,
            sprite: "spr_player_idle".into(),
            index: 0.0, speed: 0.35,
            crouch_mask: false,
            idle_timer: 150, step_timer: 0, wallgrab: 0,
            noclip: false, events: Vec::new(), broken: Vec::new(), crouchslip: 0, i_frame: 0,
            slap_buffer: 0, down_buffer: 0, freefallsmash: 0, haul_uid: None, throw: None,
            piledriver_hit: false, release: false,
            rng: 0x1234_5678,
            enter_target: None,
        }
    }

    fn rand(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % n
    }

    pub fn set(&mut self, sprite: &str, index: f32) {
        self.sprite = sprite.to_string();
        self.index = index;
    }

    fn anim_end(&self, a: &Assets) -> bool {
        self.index > a.frames(&self.sprite) as f32 - 1.0
    }

    // ------------------------------------------------------------------
    //  Маска столкновения: spr_player_mask / spr_crouchmask (включительные границы)
    // ------------------------------------------------------------------
    pub fn mask_at(&self, x: f32, y: f32) -> (f32, f32, f32, f32) {
        let top = if self.crouch_mask { y + 24.0 } else { y + 1.0 };
        (x - 13.0, top, x + 12.0, y + 45.0)
    }

    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        self.mask_at(self.x.floor(), self.y.floor())
    }

    // ------------------------------------------------------------------
    //  scr_slope_collideCheck
    // ------------------------------------------------------------------
    fn slope_check(&self, s: &Block, x: f32, y: f32) -> bool {
        let (_, cur_top, _, cur_bottom) = self.mask_at(self.x, self.y);
        let player_height = cur_bottom - self.y;
        let angle = (s.b - s.t) / (s.r - s.l);
        let mut side = x + (-13.0);
        let mut slope = s.b + ((side - s.r) * angle);
        if s.sx > 0.0 {
            side = x + 12.0;
            slope = s.b - ((side - s.l) * angle);
        }
        if s.sy < 0.0 {
            let h = self.y - cur_top;
            return y + h < s.t + (s.b - slope);
        }
        y + player_height > slope
    }

    // ------------------------------------------------------------------
    //  scr_solid
    // ------------------------------------------------------------------
    pub fn solid_at(&self, room: &Room, x: f32, y: f32) -> bool {
        let (l, t, r, b) = self.mask_at(x.floor(), y.floor());
        let (_, cur_top, _, cur_bottom) = self.mask_at(self.x, self.y);
        for blk in &room.blocks {
            if !blk.hits(l, t, r, b) {
                continue;
            }
            match blk.kind {
                BlockKind::Solid => return true,
                BlockKind::Slope => {
                    if self.slope_check(blk, x, y) {
                        return true;
                    }
                }
                BlockKind::Platform => {
                    let top = if blk.sy.signum() < 0.0 {
                        (cur_top - 1.0 >= blk.t + 1.0) && self.vsp <= 0.0
                    } else {
                        (cur_bottom - 1.0 <= blk.t + 1.0) && self.vsp >= 0.0
                    };
                    if top {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// place_meeting(x, y, obj_solid) - в GM сюда входят и дети obj_solid (разрушаемые, metalblock...)
    pub fn place_solid(&self, room: &Room, x: f32, y: f32) -> bool {
        let (l, t, r, b) = self.mask_at(x.floor(), y.floor());
        room.blocks.iter().any(|k| k.kind == BlockKind::Solid && k.hits(l, t, r, b))
    }

    fn place_block<F: Fn(&Block) -> bool>(&self, room: &Room, x: f32, y: f32, f: F) -> bool {
        let (l, t, r, b) = self.mask_at(x.floor(), y.floor());
        room.blocks.iter().any(|k| f(k) && k.hits(l, t, r, b))
    }

    fn place_slope(&self, room: &Room, x: f32, y: f32) -> bool {
        self.place_block(room, x, y, |k| k.kind == BlockKind::Slope)
    }

    /// scr_slope(): проверяется только ПЕРВЫЙ перекрывающийся склон (как instance_place)
    fn scr_slope(&self, room: &Room, x: f32, y: f32) -> bool {
        let (l, t, r, b) = self.mask_at(x.floor(), y.floor());
        match room.blocks.iter().find(|k| k.kind == BlockKind::Slope && k.hits(l, t, r, b)) {
            Some(s) => self.slope_check(s, x, y),
            None => false,
        }
    }

    fn slope_momentum(&mut self, room: &Room, accel: f32, accel2: f32) {
        if !self.scr_slope(room, self.x, self.y + 1.0) {
            return;
        }
        let (l, t, r, b) = self.mask_at(self.x.floor(), (self.y + 1.0).floor());
        if let Some(s) = room.blocks.iter().find(|k| k.kind == BlockKind::Slope && k.hits(l, t, r, b)) {
            if sign(s.sx) != self.xscale {
                self.movespeed += if s.sy.abs() >= s.sx.abs() { accel2 } else { accel };
            }
        }
    }

    // ------------------------------------------------------------------
    //  scr_collision
    // ------------------------------------------------------------------
    fn collision(&mut self, room: &Room) {
        self.grounded = false;
        if self.vsp < 20.0 {
            self.vsp += self.grav;
        }
        let mut final_h = self.hsp;
        let mut final_v = self.vsp;

        for _ in 0..gm_round(final_v.abs()) {
            let o_y = self.y;
            self.y += sign(final_v);
            if self.solid_at(room, self.x, self.y) {
                self.y = o_y;
                self.vsp = 0.0;
                final_v = 0.0;
            }
        }

        for _ in 0..gm_round(final_h.abs()) {
            let o_x = self.x;
            self.x += sign(final_h);
            if self.vsp >= -1.0 {
                for i in 0..8 {
                    let fi = i as f32;
                    if !self.solid_at(room, self.x, self.y - fi) {
                        // подъём по склону вверх
                        let mut guard = 0;
                        while self.solid_at(room, self.x, self.y) && guard < 64 {
                            self.y -= 1.0;
                            guard += 1;
                        }
                    }
                    if self.solid_at(room, self.x, self.y + fi + 1.0)
                        && self.solid_at(room, self.x - sign(final_h), self.y + fi)
                    {
                        // спуск по склону вниз
                        let mut guard = 0;
                        while !self.solid_at(room, self.x, self.y + 1.0) && guard < 64 {
                            self.y += 1.0;
                            guard += 1;
                        }
                    }
                }
            }
            if self.solid_at(room, self.x, self.y) {
                self.x = o_x;
                self.hsp = 0.0;
                final_h = 0.0;
            }
        }

        self.grounded |= self.solid_at(room, self.x, self.y + 1.0);
        if self.grounded {
            self.y = self.y.floor();
        }
    }

    /// scr_collide_destructibles
    fn collide_destructibles(&mut self, room: &mut Room) -> bool {
        let horizontal = matches!(
            self.state,
            State::MachSlide | State::MachTurn | State::Mach2 | State::Mach3 | State::Tumble
                | State::Grab | State::Uppercut | State::Buzzsaw
        );
        let vertical = matches!(
            self.state,
            State::MachTurn | State::GroundPound | State::Uppercut | State::SuperJump | State::Buzzsaw | State::ClimbWall
        ) || (self.state == State::Jump && self.vsp < 0.0);
        let mut tx = self.x;
        let mut ty = self.y;
        if horizontal {
            tx = self.x + (self.hsp + self.xscale) * 2.0;
        }
        if vertical {
            ty = self.y + (self.vsp * 2.0) + 1.0;
        }
        let (l, t, r, b) = self.mask_at(tx.floor(), ty.floor());
        let mut any = false;
        for blk in room.blocks.iter_mut() {
            if blk.destructible && blk.hits(l, t, r, b) {
                blk.alive = false;
                any = true;
                self.broken.push((blk.l, blk.t, blk.r, blk.b, blk.obj.clone()));
            }
        }
        any
    }

    // ------------------------------------------------------------------
    //  Шаг игрока (obj_player/Step_0)
    // ------------------------------------------------------------------
    pub fn update(&mut self, a: &Assets, room: &mut Room, k: &Input) {
        // анимация: image_index += image_speed
        self.index += self.speed * a.sprite_speed(&self.sprite);

        if self.noclip {
            self.update_noclip(a, k);
            return;
        }

        if self.i_frame > 0 {
            self.i_frame -= 1;
        }
        // obj_player/Step_1: буферы grab / вниз
        if k.grab_pressed {
            self.slap_buffer = 15;
        }
        if k.down_pressed {
            self.down_buffer = 15;
        }
        self.slap_buffer = (self.slap_buffer - 1).max(0);
        self.down_buffer = (self.down_buffer - 1).max(0);
        self.check_ladder(room, k);
        let prev_state = self.state;
        let prev_vsp = self.vsp;
        self.do_state(a, room, k);

        // --- звуковые события ---
        if self.state != prev_state {
            match self.state {
                State::HitWall => {
                    if self.sprite == "spr_player_mach3hitwall" {
                        self.events.push("mach3bump");
                    } else {
                        let i = self.rand(3) as usize;
                        self.events.push(["mach2bump1", "mach2bump2", "mach2bump3"][i]);
                    }
                }
                State::MachTurn => self.events.push("machturn"),
                State::MachSlide => self.events.push("machslide"),
                State::Mach2 if prev_state == State::Normal => self.events.push("machstart"),
                _ => {}
            }
        }
        if self.vsp <= -8.0
            && prev_vsp > -8.0
            && !matches!(self.state, State::ClimbWall | State::Uppercut | State::SuperJump | State::SuperJumpPrep)
        {
            self.events.push("jump");
        }
        if self.state == State::Normal && self.sprite == "spr_player_move" && self.grounded && self.movespeed > 0.0 {
            self.step_timer += 1;
            if self.step_timer >= 16 {
                self.step_timer = 0;
                self.events.push("step");
            }
        } else {
            self.step_timer = 0;
        }

        // железные блоки и разрушаемые на пути ломаются ДО коллизии (иначе она остановит игрока о блок)
        self.metal_check(room);
        if matches!(self.state, State::GroundPound | State::Buzzsaw | State::SuperJump)
            && self.collide_destructibles(room)
        {
            self.events.push("breakblock");
        }

        self.collision(room);
        if self.collide_destructibles(room) {
            self.events.push("breakblock");
        }

        if self.grounded {
            self.coyote_time = 10;
        } else if self.vsp < 0.0 {
            self.coyote_time = 0;
        }
        self.coyote_time -= 1;

        self.crouch_mask = matches!(self.state, State::Crouch | State::Tumble | State::SuperJumpPrep);

        if k.jump_pressed {
            self.jump_buffer = 15;
        }
        if self.jump_buffer > 0 {
            self.jump_buffer -= 1;
        }

        // wrap анимации (GM делает это сам после того, как Step увидел animation_end)
        let n = a.frames(&self.sprite) as f32;
        if self.index >= n {
            self.index %= n;
        }
    }

    /// Ноуклип: свободный полёт без столкновений. Стрелки - движение, Shift - быстрее.
    fn update_noclip(&mut self, a: &Assets, k: &Input) {
        let speed = if k.attack { 16.0 } else { 6.0 };
        let mx = k.move_x();
        let my = (k.down as i32 - k.up as i32) as f32;
        self.x += mx * speed;
        self.y += my * speed;
        self.hsp = 0.0;
        self.vsp = 0.0;
        self.movespeed = 0.0;
        self.grounded = false;
        self.coyote_time = 0;
        self.jump_buffer = 0;
        self.crouch_mask = false;
        self.state = State::Normal;
        self.speed = 0.35;
        if mx != 0.0 {
            self.xscale = mx;
        }
        let want = if mx != 0.0 || my != 0.0 { "spr_player_move" } else { "spr_player_idle" };
        if self.sprite != want {
            self.set(want, 0.0);
        }
        let n = a.frames(&self.sprite) as f32;
        if self.index >= n {
            self.index %= n;
        }
    }

    /// Включает/выключает ноуклип. Выключить внутри стены нельзя (вернёт false).
    pub fn set_noclip(&mut self, on: bool, room: &Room) -> bool {
        if on {
            self.noclip = true;
            return true;
        }
        if self.solid_at(room, self.x, self.y) || self.place_solid(room, self.x, self.y) {
            return false;
        }
        self.noclip = false;
        self.vsp = 0.0;
        self.state = State::Normal;
        self.set("spr_player_idle", 0.0);
        true
    }

    fn do_state(&mut self, a: &Assets, room: &Room, k: &Input) {
        match self.state {
            State::Normal => self.st_normal(a, room, k),
            State::Jump => self.st_jump(a, room, k),
            State::Crouch => self.st_crouch(a, room, k),
            State::Mach2 => self.st_mach2(a, room, k),
            State::Mach3 => self.st_mach3(a, room, k),
            State::MachSlide => self.st_machslide(a, room, k),
            State::MachTurn => self.st_machturn(a, room, k),
            State::HitWall => self.st_hitwall(a),
            State::ClimbWall => self.st_climbwall(room, k),
            State::GroundPound => self.st_groundpound(a, room, k),
            State::FreefallLand => self.st_freefallland(a),
            State::Grab => self.st_grab(a, room, k),
            State::Hauling => self.st_hauling(a, room, k),
            State::FinishingBlow => self.st_finishingblow(a),
            State::Uppercut => self.st_uppercut(a, room, k),
            State::SuperJumpPrep => self.st_superjumpprep(a, room, k),
            State::SuperJump => self.st_superjump(a, room, k),
            State::Buzzsaw => self.st_buzzsaw(a, room, k),
            State::Ladder => self.st_ladder(room, k),
            State::Slip => self.st_slip(a, room),
            State::Tumble => self.st_tumble(a, room, k),
            State::Hurt => self.st_hurt(),
            State::EnterDoor => {
                self.hsp = 0.0;
                self.vsp = 0.0;
                self.speed = 0.35;
                if self.anim_end(a) {
                    self.index = a.frames(&self.sprite) as f32 - 1.0;
                }
            }
            State::WalkFront => {
                self.hsp = 0.0;
                self.vsp = 0.0;
                self.speed = 0.35;
                if self.anim_end(a) {
                    self.set("spr_player_idle", 0.0);
                    self.state = State::Normal;
                }
            }
        }
    }

    // ---------------------------- normal -----------------------------
    fn st_normal(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        let mv = k.move_x();
        let move_spr = "spr_player_move";
        let idle_spr = "spr_player_idle";

        if mv != 0.0 {
            if self.sprite != move_spr
                && self.sprite != "spr_player_land2"
                && self.sprite != "spr_player_machslideend"
            {
                self.set(move_spr, 0.0);
            }
            self.speed = (self.movespeed / 6.0) * 0.35;
            self.xscale = mv;

            if !self.place_solid(room, self.x + self.xscale, self.y) {
                self.movespeed = approach(self.movespeed, 6.0, 0.5);
            } else {
                self.movespeed = approach(self.movespeed, 0.0, 0.65);
                self.speed = 0.1;
            }
            if self.anim_end(a) && (self.sprite == "spr_player_land2" || self.sprite == "spr_player_machslideend") {
                self.set(move_spr, 0.0);
            }
        } else {
            let idle_variants = [
                "spr_player_idleanim1", "spr_player_idleanim2", "spr_player_idleanim3",
                "spr_player_idleanim4", "spr_player_idleanim5",
            ];
            let busy = self.sprite == idle_spr
                || idle_variants.contains(&self.sprite.as_str())
                || self.sprite == "spr_player_land"
                || self.sprite == "spr_player_machslideend";
            if !busy {
                self.set(idle_spr, 0.0);
                self.speed = 0.35;
                self.idle_timer = 150;
            }
            self.movespeed = 0.0;
            if self.sprite == idle_spr {
                self.idle_timer -= 1;
            }
            if self.idle_timer <= 0 {
                self.idle_timer = 150;
                let i = self.rand(5) as usize;
                self.set(idle_variants[i], 0.0);
                self.speed = 0.35;
            }
            if self.anim_end(a)
                && (idle_variants.contains(&self.sprite.as_str())
                    || self.sprite == "spr_player_land"
                    || self.sprite == "spr_player_machslideend")
            {
                self.set(idle_spr, 0.0);
            }
        }

        if self.coyote_time <= 0 {
            self.freefalling = 0;
            self.set("spr_player_fall", 0.0);
            self.state = State::Jump;
        }
        if self.jump_buffer > 0 && self.coyote_time > 0 {
            self.jump_buffer = 0;
            self.set("spr_player_jump", 0.0);
            self.freefalling = 0;
            self.state = State::Jump;
            self.vsp = -11.0;
            self.jumpstop = false;
        }
        if (k.down && self.grounded) || self.place_solid(room, self.x, self.y - 1.0) {
            self.speed = 0.35;
            self.set("spr_player_crouchstart", 0.0);
            self.state = State::Crouch;
        }
        if k.attack && !self.place_solid(room, self.x + self.xscale, self.y) {
            if self.movespeed < 6.0 {
                self.movespeed = 6.0;
            }
            self.set("spr_player_mach1", 0.0);
            self.state = State::Mach2;
        }
        self.do_grab(k);
    }

    // ----------------------------- jump ------------------------------
    fn st_jump(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        let mv = k.move_x();
        self.speed = 0.35;
        self.freefalling += 1;
        if !self.jumpstop && !k.jump && self.vsp < 0.5 {
            self.jumpstop = true;
            self.vsp /= 10.0;
        }
        if mv != 0.0 {
            self.xscale = mv;
            if !self.place_solid(room, self.x + self.xscale, self.y) {
                self.movespeed = approach(self.movespeed, 6.0, 0.5);
            } else {
                self.movespeed = approach(self.movespeed, 0.0, 0.65);
            }
        } else {
            self.movespeed = 0.0;
        }
        if self.anim_end(a) && self.sprite == "spr_player_jump" {
            self.set("spr_player_fall", 0.0);
        }
        if self.freefalling >= 60 && self.sprite == "spr_player_fall" {
            self.sprite = "spr_player_fallfast1".into();
        }
        if self.freefalling >= 120 && self.sprite == "spr_player_fallfast1" {
            self.sprite = "spr_player_fallfast2".into();
        }
        if k.down_pressed && !self.grounded {
            // doGroundpoundCheck: ↓ в воздухе
            self.start_groundpound();
            return;
        }
        if self.grounded {
            self.set(if mv != 0.0 { "spr_player_land2" } else { "spr_player_land" }, 0.0);
            self.state = State::Normal;
            self.freefalling = 0;
        }
        if self.sprite != "spr_player_grabbump" {
            self.do_grab(k);
        }
    }

    // ---------------------------- crouch -----------------------------
    fn st_crouch(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        let mv = k.move_x();
        if !k.down && !self.solid_at(room, self.x, self.y - 16.0) && self.grounded {
            self.set("spr_player_idle", 0.0);
            self.state = State::Normal;
        }
        if self.anim_end(a) && self.sprite == "spr_player_crouchstart" {
            self.set("spr_player_crouch", 0.0);
        }
        if self.anim_end(a) && self.sprite == "spr_player_crouchhop" {
            self.set("spr_player_crouchfall", 0.0);
        }
        if !self.jumpstop && !k.jump && self.vsp < 0.5 {
            self.jumpstop = true;
            self.vsp /= 10.0;
        }
        if self.grounded && (self.sprite == "spr_player_crouchhop" || self.sprite == "spr_player_crouchfall") {
            self.set("spr_player_crouchstart", 2.0);
        }
        if mv != 0.0 {
            if self.sprite == "spr_player_crouch" {
                self.set("spr_player_crouchmove", 0.0);
            }
            self.xscale = mv;
            self.movespeed = approach(self.movespeed, 4.0, 0.5);
            if self.sprite == "spr_player_crouchmove" {
                self.speed = self.movespeed / 15.0;
            }
        } else {
            self.movespeed = 0.0;
            self.speed = 0.35;
            if self.sprite == "spr_player_crouchmove" {
                self.set("spr_player_crouch", 0.0);
            }
        }
        if !self.grounded && self.sprite != "spr_player_crouchhop" {
            self.set("spr_player_crouchfall", 0.0);
        }
        if self.jump_buffer > 0 && self.grounded {
            self.jump_buffer = 0;
            self.speed = 0.35;
            self.set("spr_player_crouchhop", 0.0);
            self.vsp = -8.0;
            self.jumpstop = false;
        }
    }

    // ----------------------------- mach2 -----------------------------
    fn st_mach2(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        let mv = k.move_x();
        if k.down {
            self.start_tumble();
            return;
        }
        self.do_grab(k);
        if self.state != State::Mach2 {
            return;
        }
        if self.grounded && self.movespeed < 12.0 {
            self.movespeed += 0.1;
        }
        self.slope_momentum(room, 0.1, 0.2);

        let s = self.sprite.clone();
        if self.anim_end(a) {
            match s.as_str() {
                "spr_player_mach1" | "spr_player_rollgetup" => self.set("spr_player_mach2", 0.0),
                "spr_player_mach2jump" => self.set("spr_player_mach2jumpend", 0.0),
                "spr_player_longjump" => self.set("spr_player_longjumpend", 0.0),
                "spr_player_walljumpstart" => self.set("spr_player_walljump", 0.0),
                _ => {}
            }
        }
        if !self.grounded && (self.sprite == "spr_player_mach2" || self.sprite == "spr_player_mach1") {
            self.set("spr_player_mach2jump", 0.0);
        }
        let air_sprites = [
            "spr_player_mach2jump", "spr_player_mach2jumpend", "spr_player_walljump",
            "spr_player_walljumpstart", "spr_player_longjump", "spr_player_longjumpend",
        ];
        if self.grounded && air_sprites.contains(&self.sprite.as_str()) {
            self.set("spr_player_mach2", 3.0);
        }
        self.speed = if self.sprite != "spr_player_longjumpend"
            && self.sprite != "spr_player_longjump"
            && self.sprite != "spr_player_rollgetup"
        {
            self.movespeed / 15.0
        } else {
            0.4
        };
        if !self.jumpstop && !k.jump && self.vsp < 0.5 {
            self.jumpstop = true;
            self.vsp /= 10.0;
        }
        if self.movespeed >= 12.0 && self.grounded {
            self.state = State::Mach3;
            if self.sprite != "spr_player_rollgetup" {
                self.set("spr_player_mach3", 0.0);
            }
            return;
        }
        if self.jump_buffer > 0 && self.coyote_time > 0 {
            self.grounded = false;
            self.jump_buffer = 0;
            self.set("spr_player_mach2jump", 0.0);
            self.vsp = -11.0;
            self.jumpstop = false;
        }
        if !k.attack && self.grounded {
            if self.sprite == "spr_player_mach1" {
                self.state = State::Normal;
                self.set("spr_player_idle", 0.0);
                self.speed = 0.35;
                self.movespeed = 0.0;
            } else {
                self.state = State::MachSlide;
                self.set("spr_player_machslidestart", 0.0);
            }
            return;
        }
        if mv != self.xscale && mv != 0.0 && self.grounded {
            if self.sprite == "spr_player_mach1" {
                self.xscale *= -1.0;
            } else {
                self.set("spr_player_mach2turn", 0.0);
                self.state = State::MachTurn;
                return;
            }
        }
        let dir = sign(self.hsp);
        if self.place_solid(room, self.x + dir, self.y)
            && !self.place_slope(room, self.x, self.y + 1.0)
            && self.grounded
            && !self.place_block(room, self.x + dir, self.y, |b| b.destructible)
        {
            self.state = State::HitWall;
            self.set("spr_player_wallsplat", 0.0);
            return;
        }
        if (self.place_solid(room, self.x + dir, self.y) && !self.grounded)
            || (self.scr_slope(room, self.x, self.y + 4.0)
                && self.place_solid(room, self.x + dir, self.y - 4.0)
                && !self.place_block(room, self.x + self.hsp, self.y, |b| b.obj == "obj_metalblock"))
        {
            self.vertical_movespeed = self.movespeed;
            self.state = State::ClimbWall;
            self.set("spr_player_wallclimb", 0.0);
        }
    }

    // ----------------------------- mach3 -----------------------------
    fn st_mach3(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        let mv = k.move_x();
        if k.down {
            self.start_tumble();
            return;
        }
        if self.try_superjump_prep(room, k) {
            return;
        }
        self.do_grab(k);
        if self.state != State::Mach3 {
            return;
        }
        self.speed = if self.sprite == "spr_player_crazyrun" { 0.75 } else { 0.4 };

        if mv == self.xscale && self.grounded {
            self.movespeed = approach(self.movespeed, 20.0, 0.025);
        }
        if self.anim_end(a)
            && (self.sprite == "spr_player_mach3jump"
                || self.sprite == "spr_player_mach3hit"
                || self.sprite == "spr_player_rollgetup"
                || self.sprite == "spr_player_buzzsawCancel"
                || self.sprite == "spr_player_superjumpcancel")
        {
            self.set("spr_player_mach3", 0.0);
        }
        if self.movespeed >= 16.0 && self.sprite != "spr_player_crazyrun" {
            self.sprite = "spr_player_crazyrun".into();
        }
        if self.movespeed < 18.0 && self.movespeed > 10.0 {
            self.slope_momentum(room, 0.1, 0.2);
        }
        if !self.jumpstop && !k.jump && self.vsp < 0.5 {
            self.jumpstop = true;
            self.vsp /= 10.0;
        }
        if self.jump_buffer > 0 && self.coyote_time > 0 && self.sprite != "spr_player_mach3turn" {
            self.grounded = false;
            self.jump_buffer = 0;
            self.set("spr_player_mach3jump", 0.0);
            self.vsp = -11.0;
            self.jumpstop = false;
        }
        if mv != self.xscale && mv != 0.0 && self.grounded {
            self.movespeed = 12.0;
            self.set("spr_player_mach3turn", 0.0);
            self.state = State::MachTurn;
            return;
        }
        if !k.attack && self.grounded {
            self.state = State::MachSlide;
            self.set("spr_player_machslidestart", 0.0);
            return;
        }
        let dir = sign(self.hsp);
        if self.place_solid(room, self.x + dir, self.y)
            && !self.place_slope(room, self.x, self.y + 1.0)
            && self.grounded
            && !self.place_block(room, self.x + dir, self.y, |b| {
                b.obj == "obj_metalblock" || b.obj == "obj_secretmetalblock" || b.destructible
            })
        {
            self.state = State::HitWall;
            self.set("spr_player_mach3hitwall", 0.0);
            self.vsp = -6.0;
            self.movespeed = -6.0;
            return;
        }
        if ((self.place_solid(room, self.x + dir, self.y) && !self.grounded)
            || (self.scr_slope(room, self.x, self.y + 4.0)
                && self.place_solid(room, self.x + dir, self.y - 4.0)))
            && !self.place_block(room, self.x + self.hsp, self.y, |b| {
                b.obj == "obj_metalblock" || b.obj == "obj_secretmetalblock"
            })
        {
            self.vertical_movespeed = self.movespeed;
            self.state = State::ClimbWall;
            self.set("spr_player_wallclimb", 0.0);
        }
    }

    // --------------------------- machslide ---------------------------
    fn st_machslide(&mut self, a: &Assets, room: &Room, _k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        self.movespeed = approach(self.movespeed, 0.0, 0.4);
        self.speed = 0.35;
        if self.anim_end(a) && self.sprite == "spr_player_machslidestart" {
            self.set("spr_player_machslide", 0.0);
        }
        if self.movespeed == 0.0 && self.sprite != "spr_player_machslideend" {
            self.set("spr_player_machslideend", 0.0);
            self.state = State::Normal;
        }
        if self.place_solid(room, self.x + sign(self.hsp), self.y) {
            self.state = State::HitWall;
            self.set("spr_player_wallsplat", 0.0);
        }
    }

    // ---------------------------- machturn ---------------------------
    fn st_machturn(&mut self, a: &Assets, _room: &Room, _k: &Input) {
        self.speed = 0.35;
        self.hsp = self.movespeed * self.xscale;
        self.movespeed = approach(self.movespeed, 0.0, 0.4);
        if self.anim_end(a) && self.sprite == "spr_player_mach2turn" {
            self.sprite = "spr_player_mach2turnfall".into();
        }
        if self.anim_end(a) && self.sprite == "spr_player_mach3turn" {
            self.sprite = "spr_player_mach3turnfall".into();
        }
        if self.grounded {
            let s = self.sprite.clone();
            let done = |end: bool, name: &str| s == name && end;
            let end = self.anim_end(a);
            if done(end, "spr_player_mach2turn") || s == "spr_player_mach2turnfall" {
                self.xscale *= -1.0;
                self.movespeed = 8.0;
                self.state = State::Mach2;
                self.set("spr_player_mach2", 0.0);
            } else if done(end, "spr_player_mach3turn") || s == "spr_player_mach3turnfall" {
                self.xscale *= -1.0;
                self.movespeed = 12.0;
                self.state = State::Mach3;
                self.set("spr_player_mach3", 0.0);
            }
        }
    }

    // ---------------------------- tumble -----------------------------
    /// scr_player_mach2/mach3: ↓ на бегу. На земле - скольжение (rollstart -> rolling),
    /// в воздухе - пике (spr_player_dive, vsp = 10).
    fn start_tumble(&mut self) {
        if !self.grounded {
            self.events.push("dive");
            self.set("spr_player_dive", 0.0);
            self.vsp = 10.0;
        } else {
            self.events.push("machslide");
            self.set("spr_player_rollstart", 0.0);
        }
        self.state = State::Tumble;
    }

    /// scr_player_tumble (ветка не-Milton)
    fn st_tumble(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        self.speed = if self.sprite != "spr_player_dive" { self.movespeed / 18.0 } else { 0.5 };
        self.slope_momentum(room, 0.1, 0.2);
        self.crouchslip -= 1;

        let rolling = self.sprite == "spr_player_rolling"
            || self.sprite == "spr_player_crouchslip"
            || self.sprite == "spr_player_rollstart";
        if rolling && !self.grounded {
            self.set("spr_player_dive", 0.0);
            self.vsp = 10.0;
        }
        if self.sprite == "spr_player_dive" && self.grounded {
            self.set("spr_player_rollstart", 0.0);
        }
        if self.sprite == "spr_player_rollstart" && self.anim_end(a) {
            self.set("spr_player_rolling", 0.0);
        }
        if !k.down && !self.solid_at(room, self.x, self.y - 16.0) && self.grounded && self.crouchslip <= 0 {
            self.set("spr_player_rollgetup", 0.0);
            self.state = State::Mach2;
            return;
        }
        let dir = sign(self.hsp);
        if self.place_solid(room, self.x + dir, self.y)
            && !self.place_block(room, self.x + dir, self.y, |b| b.destructible)
        {
            self.crouchslip = 0;
            self.state = State::HitWall;
            self.set("spr_player_wallsplat", 0.0);
        }
    }

    // ---------------------------- hitwall ----------------------------
    fn st_hitwall(&mut self, a: &Assets) {
        self.speed = 0.35;
        self.hsp = self.movespeed * self.xscale;
        if self.grounded {
            self.movespeed = approach(self.movespeed, 0.0, 1.0);
        }
        if self.sprite == "spr_player_wallsplat" {
            self.movespeed = 0.0;
            self.hsp = 0.0;
            self.vsp = 0.0;
        }
        if self.anim_end(a) {
            self.set("spr_player_land", 0.0);
            self.state = State::Normal;
        }
    }

    // --------------------------- climbwall ---------------------------
    // Исходник для Mildred на вершине стены переходит в buzzsaw (в порт он не входит),
    // поэтому используется ветка "не Mildred": возврат в mach2/mach3 с сохранением скорости.
    fn st_climbwall(&mut self, room: &Room, k: &Input) {
        self.hsp = 0.0;
        self.speed = 0.6;
        self.vsp = -self.vertical_movespeed;
        self.vertical_movespeed = approach(self.vertical_movespeed, 20.0, 0.15);

        if (!k.attack && self.wallgrab <= 0) || self.vertical_movespeed <= 0.0 {
            self.vsp = 0.0;
            self.state = State::Jump;
            self.freefalling = 0;
            self.movespeed = -5.0;
            self.set("spr_player_fall", 0.0);
        }
        self.wallgrab -= 1;

        if !self.place_solid(room, self.x + 32.0 * self.xscale, self.y) {
            // вершина стены: персонаж Mildred уходит в buzzsaw
            self.wallgrab = 0;
            self.state = State::Buzzsaw;
            self.set("spr_player_buzzsawBump", 0.0);
            if self.vertical_movespeed < 13.0 {
                self.vertical_movespeed = 13.0;
            }
            self.movespeed = self.vertical_movespeed;
            self.vsp = -8.0;
            self.events.push("machstart");
        }
        if self.jump_buffer > 8 {
            self.jump_buffer = 0;
            self.vsp = -11.0;
            self.xscale *= -1.0;
            self.movespeed = 10.0;
            self.state = State::Mach2;
            self.jumpstop = false;
            self.set("spr_player_walljumpstart", 0.0);
        }
    }

    /// Переход в состояние "вошёл в дверь" (obj_door / obj_levelgate)
    pub fn begin_enter(&mut self, sprite: &str, target_room: &str, target_door: &str) {
        self.movespeed = 0.0;
        self.vsp = 0.0;
        self.hsp = 0.0;
        self.state = State::EnterDoor;
        self.set(sprite, 0.0);
        self.enter_target = Some((target_room.to_string(), target_door.to_string()));
    }

    /// obj_baddie: какие состояния убивают врага мгновенно (instakill из obj_player/Step_0)
    pub fn instakill(&self) -> bool {
        matches!(
            self.state,
            State::Mach3 | State::SuperJump | State::Uppercut | State::GroundPound | State::Buzzsaw
        ) || (self.state == State::MachTurn && self.sprite == "spr_player_mach3turn")
    }

    /// hurt_player(): отбрасывание, 100 кадров неуязвимости. false - урон не прошёл.
    pub fn hurt(&mut self, from_x: f32) -> bool {
        if self.i_frame > 0 || self.noclip || self.state == State::EnterDoor || self.state == State::WalkFront {
            return false;
        }
        self.i_frame = 100;
        self.haul_uid = None;
        let facing = if self.x - from_x >= 0.0 { 1.0 } else { -1.0 };
        let same_face = self.xscale == facing;
        self.hsp = 8.0 * facing;
        self.vsp = -12.0;
        self.movespeed = 0.0;
        self.speed = 0.35;
        self.state = State::Hurt;
        self.set(if !same_face { "spr_player_hurt" } else { "spr_player_hurt2" }, 0.0);
        self.events.push("hurt");
        true
    }

    /// scr_player_hurt
    fn st_hurt(&mut self) {
        self.speed = 0.35;
        if self.grounded && self.vsp >= 0.0 {
            self.movespeed = 0.0;
            self.set("spr_player_land", 0.0);
            self.state = State::Normal;
        }
    }

    /// obj_ballofbeer/Step_0: игрок "падает" в бочку
    pub fn begin_beer(&mut self, up: bool, x: f32, y: f32, target_room: &str, target_door: &str) {
        self.begin_enter(if up { "spr_player_upbox" } else { "spr_player_downbox" }, target_room, target_door);
        self.x = x;
        self.y = y;
    }

    pub fn begin_walkfront(&mut self) {
        self.state = State::WalkFront;
        self.set("spr_player_walkfront", 0.0);
    }

    pub fn draw(&self, a: &Assets, palette: i32) {
        let mut alpha = if self.noclip { 0.55 } else { 1.0 };
        if self.i_frame > 0 && (self.i_frame / 3) % 2 == 0 {
            alpha = 0.35; // мигание после урона
        }
        // палитра игрока (spr_playerPal, по умолчанию столбец 1)
        a.draw_sprite_pal(&self.sprite, self.index, self.x.floor(), self.y.floor(), self.xscale, 1.0, alpha, "spr_playerPal", palette as f32);
    }
}
