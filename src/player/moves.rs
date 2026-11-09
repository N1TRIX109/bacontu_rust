//! Остальной мувсет, нужный для прохождения уровней (порт из GML):
//! grab, hauling (+бросок врага), finishingblow, uppercut, groundpound (+piledriver), freefallland,
//! superjump (prep + прыжок), buzzsaw, лестницы, ломание железных блоков.
//! Скрипты оригинала: scr_player_grab / hauling / groundpound / uppercut / buzzsaw / ladder,
//! state_player_superjump, obj_metalblock/Step_0, doGrab() из playerUtil.

use super::*;

impl Player {
    // ------------------------------------------------------------------
    //  doGrab()
    // ------------------------------------------------------------------
    pub(super) fn do_grab(&mut self, k: &Input) {
        if self.slap_buffer <= 0 {
            return;
        }
        if !k.up {
            // grab dash
            self.events.push("grab");
            self.slap_buffer = 0;
            self.state = State::Grab;
            self.set("spr_player_grab", 0.0);
            if self.movespeed < 8.0 {
                self.movespeed = 8.0;
            }
        } else {
            // uppercut (↑ + grab)
            self.events.push("uppercut");
            self.slap_buffer = 0;
            self.vsp = -16.0;
            let mach = self.state == State::Mach3
                || self.sprite == "spr_player_longjump"
                || self.sprite == "spr_player_longjumpend";
            self.set(if mach { "spr_player_mach_uppercut" } else { "spr_player_uppercut" }, 0.0);
            self.state = State::Uppercut;
        }
    }

    fn grab_out(&mut self, k: &Input) {
        if k.attack {
            self.state = State::Mach2;
            self.set("spr_player_mach2", 0.0);
        } else {
            self.set("spr_player_idle", 0.0);
            self.state = State::Normal;
            if k.move_x() == 0.0 {
                self.movespeed = 0.0;
            }
        }
    }

    // ------------------------------- grab -----------------------------
    pub(super) fn st_grab(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        if !self.jumpstop && !k.jump && self.vsp < 0.5 {
            self.jumpstop = true;
            self.vsp /= 20.0;
        }
        let mv = k.move_x();
        if self.movespeed < 10.0 {
            self.movespeed = approach(self.movespeed, 10.0, 0.5);
        }
        self.speed = 0.35;
        if !self.grounded && self.sprite == "spr_player_grab" {
            self.set("spr_player_airgrabstart", 0.0);
        }
        if self.grounded && self.sprite == "spr_player_airgrab" {
            self.slap_buffer = 0;
            self.grab_out(k);
            return;
        }
        if self.anim_end(a) && self.sprite == "spr_player_grab" {
            self.grab_out(k);
            return;
        }
        if self.anim_end(a) && self.sprite == "spr_player_airgrabstart" {
            self.set("spr_player_airgrab", 0.0);
        }
        if self.down_buffer > 0 && self.grounded {
            // ↓ во время grab: перекат под низкими проходами
            self.down_buffer = 0;
            self.crouchslip = 25;
            self.set("spr_player_crouchslip", 0.0);
            self.state = State::Tumble;
            self.movespeed = 12.0;
            return;
        }
        if mv != 0.0 && mv != self.xscale {
            self.movespeed = 0.0;
            self.set("spr_player_grabcancel", 0.0);
            self.state = State::Jump;
            self.freefalling = 0;
            return;
        }
        if self.jump_buffer > 0 && self.coyote_time > 0 {
            // long jump
            self.jump_buffer = 0;
            self.set("spr_player_longjump", 0.0);
            self.state = State::Mach2;
            self.vsp = -11.0;
            self.jumpstop = false;
        }
        let dir = sign(self.hsp);
        if self.place_solid(room, self.x + dir, self.y) && !self.grounded {
            self.wallgrab = 10;
            self.vertical_movespeed = self.movespeed;
            self.state = State::ClimbWall;
            self.set("spr_player_wallclimb", 0.0);
        }
        if self.place_solid(room, self.x + dir, self.y)
            && self.grounded
            && !self.place_block(room, self.x + dir, self.y, |b| b.destructible)
        {
            self.movespeed = 0.0;
            self.hsp = 0.0;
            self.vsp = -4.0;
            self.state = State::Jump;
            self.set("spr_player_grabbump", 0.0);
            self.events.push("bump");
        }
    }

    // ----------------------------- uppercut ---------------------------
    pub(super) fn st_uppercut(&mut self, a: &Assets, room: &Room, k: &Input) {
        let mach = self.sprite == "spr_player_mach_uppercut";
        let mv = if mach { self.xscale } else { k.move_x() };
        self.hsp = approach(self.hsp, self.movespeed * mv, 0.4);
        if !mach && mv != 0.0 {
            self.movespeed = 4.0;
        }
        self.speed = 0.4;
        if mach
            && self.place_solid(room, self.x + sign(self.hsp), self.y)
            && !self.place_block(room, self.x + sign(self.hsp), self.y, |b| b.destructible)
        {
            self.state = State::HitWall;
            self.set("spr_player_wallsplat", 0.0);
            return;
        }
        if self.anim_end(a) {
            self.index = a.frames(&self.sprite) as f32 - 1.0;
        }
        if self.grounded && self.vsp > 0.0 {
            self.set(if mv != 0.0 { "spr_player_land2" } else { "spr_player_land" }, 0.0);
            self.state = State::Normal;
            self.movespeed = self.hsp.abs();
            if mv != 0.0 {
                self.xscale = mv;
            }
            self.hsp = self.movespeed * self.xscale;
        }
    }

    // --------------------------- groundpound --------------------------
    /// doGroundpoundCheck: ↓ (нажатие) в воздухе
    pub(super) fn start_groundpound(&mut self) {
        self.vsp = -6.0;
        self.set("spr_player_groundpoundstart", 0.0);
        self.state = State::GroundPound;
        self.movespeed = self.hsp;
        self.freefalling = 0;
        self.events.push("dive");
    }

    pub(super) fn st_groundpound(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.speed = if self.sprite != "spr_player_groundpoundstart" { 0.35 } else { 0.4 };
        let mv = k.move_x();
        self.hsp = self.movespeed;
        let (accel, deaccel) = (0.25, 0.05);
        if mv == 0.0 {
            self.movespeed = 0.0;
        } else if mv > 0.0 {
            if self.movespeed < 0.0 {
                self.movespeed = 0.0;
            }
            if self.movespeed < 7.0 {
                self.movespeed += accel;
            } else {
                self.movespeed -= deaccel;
            }
        } else {
            if self.movespeed > 0.0 {
                self.movespeed = 0.0;
            }
            if self.movespeed > -7.0 {
                self.movespeed -= accel;
            } else {
                self.movespeed += deaccel;
            }
        }
        if self.vsp > 0.0 {
            self.freefallsmash += 1;
        } else if self.vsp < 0.0 {
            self.freefallsmash = -14;
        }
        if self.vsp >= 2.0 {
            self.vsp += 0.5;
        }
        if self.sprite == "spr_player_groundpoundstart" && self.anim_end(a) {
            self.set("spr_player_groundpound", 0.0);
        }
        let below = self.y + self.vsp + 1.0;
        if self.grounded
            && self.vsp >= 0.0
            && !self.place_block(room, self.x, below, |b| b.destructible)
            && (self.freefallsmash < 10 || !self.place_block(room, self.x, below, |b| b.metal))
        {
            if self.scr_slope(room, self.x, self.y + 1.0) && self.sprite != "spr_player_piledriver" {
                // на склоне groundpound переходит в скольжение
                let (l, t, r, b) = self.mask_at(self.x.floor(), (self.y + 1.0).floor());
                if let Some(s) = room.blocks.iter().find(|q| q.kind == BlockKind::Slope && q.hits(l, t, r, b)) {
                    self.xscale = -sign(s.sx);
                }
                self.movespeed = if self.freefallsmash > 20 { 12.0 } else { 8.0 };
                self.state = State::Tumble;
                self.set("spr_player_rolling", 0.0);
                self.freefallsmash = -14;
                return;
            }
            self.freefallsmash = -14;
            if self.sprite == "spr_player_piledriver" {
                self.set("spr_player_piledriverend", 0.0);
                self.piledriver_hit = true; // Game убьёт удерживаемого врага
            } else {
                self.set("spr_player_groundpoundland", 0.0);
            }
            self.vsp = 0.0;
            self.hsp = 0.0;
            self.state = State::FreefallLand;
            self.movespeed = 0.0;
            self.events.push("divelanding");
        }
    }

    pub(super) fn st_freefallland(&mut self, a: &Assets) {
        self.speed = 0.35;
        self.vsp = 0.0;
        self.hsp = 0.0;
        self.movespeed = 0.0;
        if self.anim_end(a) {
            match self.sprite.as_str() {
                "spr_player_hitceiling" => {
                    self.state = State::Jump;
                    self.freefalling = 0;
                    self.set("spr_player_spinout", 0.0);
                    self.vsp = 2.0;
                }
                "spr_player_groundpoundland" | "spr_player_divebombland" => {
                    self.state = State::Normal;
                    self.set("spr_player_land", 0.0);
                }
                "spr_player_piledriverend" => {
                    self.state = State::Jump;
                    self.freefalling = 0;
                    self.vsp = -15.0;
                    self.jumpstop = false;
                    self.set("spr_player_piledrivergetup", 0.0);
                }
                _ => {}
            }
        }
    }

    // --------------------------- superjump ----------------------------
    /// scr_player_mach3: ↑ на земле в mach3 -> заряд супер-прыжка
    pub(super) fn try_superjump_prep(&mut self, room: &Room, k: &Input) -> bool {
        if k.up && self.grounded && !self.place_solid(room, self.x, self.y - 32.0) {
            self.movespeed = 12.0;
            self.set("spr_player_superjumpprepstart", 0.0);
            self.state = State::SuperJumpPrep;
            self.events.push("superjumpprep");
            return true;
        }
        false
    }

    pub(super) fn st_superjumpprep(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        self.speed = 0.35;
        if self.anim_end(a) && self.sprite == "spr_player_superjumpprepstart" {
            self.set("spr_player_superjumpprep", 0.0);
        }
        if self.sprite != "spr_player_superjumpprepstart" {
            let mv = k.move_x();
            if mv != 0.0 {
                self.sprite = "spr_player_superjumpprepwalk".to_string();
                self.xscale = mv;
                self.movespeed = approach(self.movespeed, 3.0, 0.5);
            } else {
                self.sprite = "spr_player_superjumpprep".to_string();
                self.movespeed = 0.0;
            }
            if !k.up && !self.place_solid(room, self.x, self.y - 1.0) {
                self.state = State::SuperJump;
                self.set("spr_player_superjump", 0.0);
                self.vertical_movespeed = -12.0;
                self.vsp = self.vertical_movespeed;
                self.events.push("superjump");
            }
        } else {
            self.movespeed = approach(self.movespeed, 0.0, 1.0);
        }
    }

    pub(super) fn st_superjump(&mut self, a: &Assets, room: &Room, k: &Input) {
        let mv = k.move_x();
        if self.sprite != "spr_player_superjumpcancelstart" {
            self.speed = 0.4;
            self.vsp = self.vertical_movespeed;
            self.vertical_movespeed -= 0.1;
            self.hsp = self.movespeed * mv;
            if mv != 0.0 && self.sprite != "spr_player_hitceiling" && self.sprite != "spr_player_spinout" {
                self.movespeed = approach(self.movespeed, 3.0, 0.5);
            } else {
                self.movespeed = 0.0;
            }
            if k.down_pressed {
                // персонаж Mildred: ↓ в супер-прыжке -> buzzsaw вниз
                if mv != 0.0 {
                    self.xscale = mv;
                }
                self.state = State::Buzzsaw;
                self.set("spr_player_buzzsawFastFall", 0.0);
                self.movespeed = 8.0;
                self.vsp = 14.0;
                return;
            }
            if (k.attack_pressed || k.grab_pressed) && self.sprite != "spr_player_spinout" {
                self.set("spr_player_superjumpcancelstart", 0.0);
            }
            let ceiling = self.place_solid(room, self.x, self.y - 1.0)
                || self.place_block(room, self.x, self.y - 1.0, |b| b.kind == BlockKind::Platform && b.sy < 0.0);
            if ceiling
                && !self.place_block(room, self.x, self.y - 1.0, |b| b.obj == "obj_ballofbeer")
                && !self.place_block(room, self.x, self.y - 1.0, |b| b.destructible)
            {
                self.set("spr_player_hitceiling", 0.0);
                self.state = State::FreefallLand;
                self.events.push("bump");
            }
        } else {
            self.speed = 0.6;
            self.vertical_movespeed = 0.0;
            self.vsp = 0.0;
            self.hsp = 0.0;
            if mv != 0.0 {
                self.xscale = mv;
            }
            if self.anim_end(a) {
                self.movespeed = 12.0;
                self.state = State::Mach3;
                self.jumpstop = true;
                self.vsp = -5.0;
                self.set("spr_player_superjumpcancel", 6.0);
                self.speed = 1.0;
            }
        }
    }

    // ----------------------------- buzzsaw ----------------------------
    pub(super) fn st_buzzsaw(&mut self, a: &Assets, room: &Room, k: &Input) {
        self.speed = 0.7;
        self.hsp = self.movespeed * self.xscale;
        if self.movespeed > 13.0 {
            self.movespeed = 13.0;
        }
        let mv = k.move_x();
        if self.down_buffer > 0 {
            self.down_buffer = 0;
            if self.vsp < 10.0 {
                self.vsp = 10.0;
            }
            self.sprite = "spr_player_buzzsawFastFall".to_string();
        }
        if self.vsp > 0.0 {
            self.vsp += 0.1;
        }
        let dir = sign(self.hsp);
        if self.place_solid(room, self.x + dir, self.y)
            && !self.grounded
            && !self.place_block(room, self.x + dir, self.y, |b| b.metal)
            && !self.place_block(room, self.x + self.hsp, self.y, |b| b.destructible)
        {
            if self.vsp < 10.0 {
                self.vsp = 10.0;
            }
            self.movespeed = 9.0;
            self.xscale *= -1.0;
            self.set("spr_player_buzzsawBump", 0.0);
            self.events.push("bump");
        }
        if self.anim_end(a) && self.sprite == "spr_player_buzzsawBump" {
            self.sprite = "spr_player_buzzsaw".to_string();
        }
        if self.grounded {
            if mv != 0.0 {
                self.xscale = mv;
            }
            if self.movespeed < 12.0 {
                self.movespeed = 12.0;
            }
            self.state = State::Mach3;
            self.set("spr_player_rollgetup", 0.0);
            return;
        }
        if self.slap_buffer > 8 {
            // отмена buzzsaw кнопкой grab -> mach3
            self.slap_buffer = 0;
            if mv != 0.0 {
                self.xscale = mv;
            }
            if self.movespeed < 13.0 {
                self.movespeed = 13.0;
            }
            self.state = State::Mach3;
            self.jumpstop = true;
            self.vsp = -8.0;
            self.set("spr_player_buzzsawCancel", 0.0);
        }
    }

    // ------------------------------ ladder ----------------------------
    /// obj_ladder/Step_0: вход на лестницу
    pub(super) fn check_ladder(&mut self, room: &Room, k: &Input) {
        if !matches!(
            self.state,
            State::Normal | State::Jump | State::Crouch | State::Uppercut | State::Mach2 | State::Mach3
        ) {
            return;
        }
        let (l, t, r, b) = self.bbox();
        let touching = |dy: f32| room.ladders.iter().find(|q| q[0] <= r && q[2] >= l && q[1] <= b + dy && q[3] >= t + dy).copied();
        let up_enter = k.up && !k.down && self.state != State::Crouch && touching(0.0).is_some();
        let down_enter = k.down
            && self.grounded
            && !k.up
            && touching(1.0).is_some()
            && !self.place_solid(room, self.x, self.y + 1.0);
        if up_enter || down_enter {
            self.state = State::Ladder;
            self.hsp = 0.0;
            self.movespeed = 0.0;
            if down_enter {
                if let Some(q) = touching(1.0) {
                    self.y = q[1] + 1.0;
                }
            }
        }
    }

    pub(super) fn st_ladder(&mut self, room: &Room, k: &Input) {
        self.movespeed = 0.0;
        self.speed = 0.35;
        self.hsp = 0.0;
        if k.up {
            self.vsp = -6.0;
            self.sprite = "spr_player_laddermove".to_string();
        } else if k.down {
            self.vsp = 10.0;
            self.sprite = "spr_player_ladderdown".to_string();
        } else {
            self.vsp = 0.0;
            self.sprite = "spr_player_ladder".to_string();
        }
        let (l, t, r, b) = self.bbox();
        let ladder = room.ladders.iter().find(|q| q[0] <= r && q[2] >= l && q[1] <= b && q[3] >= t).copied();
        match ladder {
            Some(q) => {
                let cx = (q[0] + q[2] + 1.0) / 2.0;
                self.x = approach(self.x, cx, 4.0);
            }
            None => {
                self.state = State::Normal;
                self.vsp = 0.0;
            }
        }
        if k.down && self.grounded && !self.place_block(room, self.x, self.y, |b| b.kind == BlockKind::Platform) {
            self.state = State::Normal;
            self.vsp = 0.0;
        }
        if self.jump_buffer > 0 {
            self.jump_buffer = 0;
            self.set("spr_player_jump", 0.0);
            self.state = State::Jump;
            self.freefalling = 0;
            self.vsp = -12.0;
            self.jumpstop = false;
        }
    }

    // ------------------------------ hauling ---------------------------
    pub(super) fn st_hauling(&mut self, a: &Assets, _room: &Room, k: &Input) {
        self.hsp = self.movespeed * self.xscale;
        let mv = k.move_x();
        if self.anim_end(a) && self.sprite == "spr_player_haulingland" {
            self.set("spr_player_haulingidle", 0.0);
        }
        if self.anim_end(a) && self.sprite == "spr_player_haulingjump" {
            self.set("spr_player_haulingfall", 0.0);
        }
        if !self.jumpstop && !k.jump && self.vsp < 0.5 {
            self.jumpstop = true;
            self.vsp /= 10.0;
        }
        if self.grounded && (self.sprite == "spr_player_haulingfall" || self.sprite == "spr_player_haulingjump") {
            self.set("spr_player_haulingland", 4.0);
        }
        if mv != 0.0 {
            if self.sprite == "spr_player_haulingidle" {
                self.set("spr_player_haulingwalk", 0.0);
            }
            self.xscale = mv;
            self.movespeed = approach(self.movespeed, 6.0, 0.5);
            if self.sprite == "spr_player_haulingwalk" {
                self.speed = self.movespeed / 15.0;
            }
        } else {
            self.movespeed = 0.0;
            self.speed = 0.35;
            if self.sprite == "spr_player_haulingwalk" {
                self.set("spr_player_haulingidle", 0.0);
            }
        }
        if !self.grounded && self.sprite != "spr_player_haulingjump" && self.sprite != "spr_player_haulingfall" {
            self.set("spr_player_haulingfall", 0.0);
        }
        if self.jump_buffer > 0 && self.grounded {
            self.jump_buffer = 0;
            self.speed = 0.35;
            self.set("spr_player_haulingjump", 0.0);
            self.vsp = -12.0;
            self.jumpstop = false;
            self.events.push("jump");
        }
        if self.slap_buffer > 0 {
            // бросок: grab ещё раз (↑ - вверх, иначе вперёд)
            self.slap_buffer = 0;
            self.state = State::FinishingBlow;
            if k.up {
                self.set("spr_player_finisheruppercut", 0.0);
            } else {
                let n = 1 + self.rand(4);
                self.set(&format!("spr_player_finisher{n}"), 0.0);
            }
            return;
        }
        if k.down && self.grounded {
            // отпустить врага и присесть
            self.speed = 0.35;
            self.set("spr_player_crouch", 0.0);
            self.state = State::Crouch;
            self.release = true;
            return;
        }
        if self.down_buffer > 0 && !self.grounded {
            // piledriver
            self.down_buffer = 0;
            self.vsp = -6.0;
            self.set("spr_player_piledriver", 0.0);
            self.state = State::GroundPound;
            self.movespeed = self.hsp;
        }
        if self.haul_uid.is_none() {
            self.state = State::Normal;
            self.set("spr_player_idle", 0.0);
        }
    }

    pub(super) fn st_finishingblow(&mut self, a: &Assets) {
        self.speed = 0.4;
        self.hsp = self.movespeed * self.xscale;
        if self.index.floor() < 4.0 {
            self.movespeed = approach(self.movespeed, 0.0, 1.0);
        } else {
            self.movespeed = approach(self.movespeed, -4.0, 0.5);
        }
        if let Some(uid) = self.haul_uid {
            if self.index.floor() > 4.0 {
                self.vsp = -6.0;
                let (hh, hv) = if self.sprite == "spr_player_finisheruppercut" {
                    (0.0, -25.0)
                } else {
                    (25.0 * self.xscale, 0.0)
                };
                self.throw = Some((uid, hh, hv));
                self.haul_uid = None;
            }
        }
        if self.anim_end(a) {
            self.slap_buffer = 0;
            self.state = State::Jump;
            self.freefalling = 0;
            self.set("spr_player_fall", 0.0);
            self.jumpstop = true;
        }
    }

    // ------------------------ железные блоки --------------------------
    /// obj_metalblock / obj_secretmetalblock Step_0: ломаются на mach3, в buzzsaw
    /// и при groundpound, если падение длилось >= 10 кадров.
    pub(super) fn metal_check(&mut self, room: &mut Room) {
        let smash = self.state == State::GroundPound && self.freefallsmash >= 10;
        if !(self.state == State::Mach3 || self.state == State::Buzzsaw || smash) {
            return;
        }
        let (mut px, mut py) = (self.x + self.hsp + self.xscale, self.y);
        if self.state == State::GroundPound {
            px = self.x;
            py = self.y + self.vsp;
        }
        if self.state == State::Buzzsaw {
            py = self.y + self.vsp;
        }
        let (l, t, r, b) = self.mask_at(px.floor(), py.floor());
        let mut any = false;
        for blk in room.blocks.iter_mut() {
            if blk.metal && blk.hits(l, t, r, b) {
                blk.alive = false;
                any = true;
                self.broken.push((blk.l, blk.t, blk.r, blk.b, blk.obj.clone()));
            }
        }
        if any {
            self.events.push("breakblock");
            if self.state == State::GroundPound {
                self.set("spr_player_groundpoundland", 0.0);
                self.vsp = 0.0;
                self.hsp = 0.0;
                self.state = State::FreefallLand;
                self.movespeed = 0.0;
            }
        }
    }

    // ------------------------------ slip ------------------------------
    /// obj_slipperybutter/Step_0: наступил на масло
    pub fn slip_on_butter(&mut self) {
        self.state = State::Slip;
        self.movespeed = (self.movespeed + 2.0).min(14.0);
        self.vsp = -11.0;
        self.set("spr_player_slipsyrup", 0.0);
        self.haul_uid = None;
        self.events.push("jump");
    }

    pub(super) fn st_slip(&mut self, a: &Assets, room: &Room) {
        self.hsp = self.movespeed * self.xscale;
        self.speed = 0.35;
        if self.anim_end(a) && (self.sprite == "spr_player_slipsyrup" || self.sprite == "spr_player_beakerintro") {
            self.index = a.frames(&self.sprite) as f32 - 1.0;
        }
        if self.place_solid(room, self.x + self.hsp, self.y) {
            self.sprite = "spr_player_spinout".to_string();
            self.xscale *= -1.0;
        }
        if !self.grounded && self.sprite == "spr_player_beakerintro" {
            self.sprite = "spr_player_spinout".to_string();
        }
        if self.grounded {
            if self.sprite == "spr_player_slipsyrup" {
                self.vsp = -5.0;
                self.set("spr_player_spinout", 0.0);
            }
            if self.sprite != "spr_player_beakerintro" && self.sprite == "spr_player_spinout" {
                self.set("spr_player_beakerintro", 0.0);
            }
            self.movespeed = approach(self.movespeed, 0.0, 0.5);
            // в GML `animation_end` без скобок - ссылка на функцию (всегда истина), поэтому условие только по скорости
            if self.movespeed == 0.0 && self.sprite == "spr_player_beakerintro" {
                self.state = State::Normal;
                self.movespeed = 0.0;
                self.set("spr_player_land", 0.0);
            }
        }
    }
}
