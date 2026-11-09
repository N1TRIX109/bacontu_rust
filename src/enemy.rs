//! Враги: obj_cocoant, obj_dougknight, obj_slosher (+plosher), obj_sunny.
//! Порт scr_enemy_move / turn / stun / hit, obj_baddie/Step_2 (столкновения с игроком) и obj_dart.

use crate::assets::Assets;
use crate::player::{Player, State};
use crate::world::{BlockKind, Room};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Cocoant,
    Dougknight,
    Slosher,
    Sunny,
    /// obj_egghead: стоит, бросается на игрока (Rock Bottom)
    Egghead,
    /// obj_pancake: спит, просыпается, бегает и роняет масло
    Pancake,
}

impl Kind {
    pub fn from_obj(obj: &str) -> Option<Kind> {
        match obj {
            "obj_cocoant" => Some(Kind::Cocoant),
            "obj_dougknight" => Some(Kind::Dougknight),
            "obj_slosher" => Some(Kind::Slosher),
            "obj_sunny" => Some(Kind::Sunny),
            "obj_egghead" => Some(Kind::Egghead),
            "obj_pancake" => Some(Kind::Pancake),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EState {
    Move,
    Turn,
    Stun,
    /// получил смертельный удар - умирает после хитстопа
    Hit,
    /// cocoant стреляет дротиком (states.uppercut)
    Shoot,
    /// враг у игрока на руках (hauling)
    Grabbed,
    /// брошен игроком: летит по прямой, убивает всё на пути
    Thrown,
    /// egghead разбегается и атакует
    Charge,
    /// pancake просыпается
    Wake,
    /// pancake бегает туда-сюда
    Run,
}

/// obj_dart: летит со скоростью 12 px/кадр, исчезает в стене, ранит игрока
pub struct Dart {
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub frame: f32,
}

pub enum EnemyFx {
    /// облачко пыли под ногами
    Puff(f32, f32),
    /// выстрел: x, y, направление
    Shoot(f32, f32, f32),
    /// pancake роняет масло: x, y, направление
    Butter(f32, f32, f32),
}

/// Результат столкновения игрока с врагом (для частиц/звука)
pub enum Hit {
    Bump(f32, f32),
    Bash(f32, f32),
    Kill(f32, f32),
    Hurt,
    Grab(f32, f32),
}

pub struct Enemy {
    pub spawn_id: usize,
    pub escape: bool,
    pub kind: Kind,
    pub plosher: bool,
    pub x: f32,
    pub y: f32,
    pub hsp: f32,
    pub vsp: f32,
    pub xscale: f32,
    pub state: EState,
    pub sprite: String,
    pub index: f32,
    speed: f32,
    movespeed: f32,
    grounded: bool,
    pub scared: i32,
    attack_timer: i32,
    step_buf: i32,
    pub dead: bool,
    /// индекс obj_enemyspawner, который выпустил врага
    pub spawner: Option<usize>,
    pub uid: u64,
    pub hit_hsp: f32,
    pub hit_vsp: f32,
    butter_done: bool,
}

fn sign(v: f32) -> f32 {
    if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 }
}

fn approach(start: f32, end: f32, inc: f32) -> f32 {
    if start < end { (start + inc).min(end) } else { (start - inc).max(end) }
}

fn gm_round(v: f32) -> i32 {
    if (v - v.trunc()).abs() == 0.5 { (2.0 * (v / 2.0).round()) as i32 } else { v.round() as i32 }
}

/// scr_solid для врага: та же маска, что у игрока (spr_player_mask), без приседания
fn actor_solid(room: &Room, x: f32, y: f32, cur_y: f32, vsp: f32) -> bool {
    let (xf, yf) = (x.floor(), y.floor());
    let (l, t, r, b) = (xf - 13.0, yf + 1.0, xf + 12.0, yf + 45.0);
    let cur_bottom = cur_y + 45.0;
    for blk in &room.blocks {
        if !blk.hits(l, t, r, b) {
            continue;
        }
        match blk.kind {
            BlockKind::Solid => return true,
            BlockKind::Slope => {
                let angle = (blk.b - blk.t) / (blk.r - blk.l);
                let mut side = x - 13.0;
                let mut slope = blk.b + ((side - blk.r) * angle);
                if blk.sx > 0.0 {
                    side = x + 12.0;
                    slope = blk.b - ((side - blk.l) * angle);
                }
                let hit = if blk.sy < 0.0 { y - 1.0 < blk.t + (blk.b - slope) } else { y + 45.0 > slope };
                if hit {
                    return true;
                }
            }
            BlockKind::Platform => {
                if blk.sy.signum() > 0.0 && cur_bottom - 1.0 <= blk.t + 1.0 && vsp >= 0.0 {
                    return true;
                }
            }
        }
    }
    false
}

/// place_meeting(x, y, <любой блок>) по bbox
fn any_block_at(room: &Room, x: f32, y: f32) -> bool {
    let (xf, yf) = (x.floor(), y.floor());
    room.blocks.iter().any(|b| b.hits(xf - 13.0, yf + 1.0, xf + 12.0, yf + 45.0))
}

fn solid_block_at(room: &Room, x: f32, y: f32) -> bool {
    let (xf, yf) = (x.floor(), y.floor());
    room.blocks
        .iter()
        .any(|b| b.kind == BlockKind::Solid && b.hits(xf - 13.0, yf + 1.0, xf + 12.0, yf + 45.0))
}

/// collision_line(...obj_solid): грубая проверка линии видимости
pub fn line_blocked(room: &Room, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    for i in 0..=24 {
        let t = i as f32 / 24.0;
        let (px, py) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        if room.blocks.iter().any(|b| b.kind == BlockKind::Solid && b.hits(px, py, px, py)) {
            return true;
        }
    }
    false
}

impl Enemy {
    pub fn new(spawn_id: usize, kind: Kind, x: f32, y: f32, xscale: f32, plosher: bool, escape: bool) -> Enemy {
        let mut e = Enemy {
            spawn_id, escape, kind, plosher, x, y,
            hsp: 0.0, vsp: 0.0, xscale: if xscale < 0.0 { -1.0 } else { 1.0 },
            state: EState::Move, sprite: String::new(), index: 0.0, speed: 0.35,
            movespeed: 0.0, grounded: false, scared: 0, attack_timer: 0, step_buf: 0, dead: false, spawner: None, uid: 0, hit_hsp: 0.0, hit_vsp: 0.0, butter_done: false,
        };
        e.sprite = e.spr().0.to_string();
        if kind == Kind::Pancake {
            e.sprite = "spr_pancake_sleep".to_string();
        }
        e
    }

    /// (move, turn, stun, scared, dead)
    pub fn spr(&self) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
        match (self.kind, self.plosher) {
            (Kind::Cocoant, _) => ("spr_cocoant_walk", "spr_doug_turn", "spr_cocoant_stun", "spr_cocoant_scared", "spr_cocoant_dead"),
            (Kind::Dougknight, _) => ("spr_doug_walk", "spr_doug_turn", "spr_doug_stun", "spr_doug_scared", "spr_doug_dead"),
            (Kind::Slosher, true) => ("spr_plosher", "spr_plosher_turn", "spr_plosher_stun", "spr_plosher_scared", "spr_plosher_dead"),
            (Kind::Slosher, false) => ("spr_slosher", "spr_slosher_turn", "spr_slosher_stun", "spr_slosher_scared", "spr_slosher_dead"),
            (Kind::Egghead, _) => ("spr_egghead", "spr_slosher_turn", "spr_egghead_stun", "spr_egghead_scared", "spr_egghead_dead"),
            (Kind::Pancake, _) => ("spr_pancake_run", "spr_slosher_turn", "spr_pancake_stun", "spr_pancake_scared", "spr_pancake_dead"),
            (Kind::Sunny, _) => ("spr_sunnyboy", "spr_sunnyboy", "spr_sunnyboy", "spr_sunnyboy", "spr_sunnyboy"),
        }
    }

    /// scr_enemy_grabbed: враг висит на игроке
    fn follow_player(&mut self, p: &Player) {
        self.hsp = 0.0;
        self.vsp = 0.0;
        self.movespeed = 0.0;
        self.xscale = p.xscale;
        let xs = p.xscale;
        match p.state {
            State::Hauling => {
                self.x = p.x - 10.0 * xs;
                self.y = p.y - 40.0;
                if p.sprite == "spr_player_haulingjump" || p.sprite == "spr_player_haulingfall" {
                    self.x = p.x - 5.0 * xs;
                    self.y = p.y - 50.0;
                }
                if p.sprite == "spr_player_haulingland" {
                    self.x = p.x - 10.0 * xs;
                    self.y = p.y - [15.0, 24.0, 34.0, 37.0, 40.0][(p.index.floor() as usize).min(4)];
                }
            }
            State::GroundPound => {
                self.x = p.x + 45.0 * xs;
                self.y = p.y - 25.0;
            }
            State::FinishingBlow => {
                self.x = p.x + 50.0 * xs;
                self.y = p.y - 5.0;
            }
            _ => {
                // игрок больше не держит врага - тот оглушён
                self.scared = 120;
                self.state = EState::Stun;
                self.x = p.x;
                self.y = p.y;
                self.sprite = self.spr().2.to_string();
            }
        }
    }

    pub fn rect(&self) -> (f32, f32, f32, f32) {
        let (x, y) = (self.x.floor(), self.y.floor());
        (x - 13.0, y + 1.0, x + 12.0, y + 45.0)
    }

    fn anim_end(&self, a: &Assets) -> bool {
        self.index > a.frames(&self.sprite) as f32 - 1.0
    }

    fn solid_at(&self, room: &Room, x: f32, y: f32) -> bool {
        actor_solid(room, x, y, self.y, self.vsp)
    }

    /// scr_collision (та же схема, что у игрока)
    fn collision(&mut self, room: &Room) {
        self.grounded = false;
        if self.vsp < 20.0 {
            self.vsp += 0.5;
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
                        let mut g = 0;
                        while self.solid_at(room, self.x, self.y) && g < 64 {
                            self.y -= 1.0;
                            g += 1;
                        }
                    }
                    if self.solid_at(room, self.x, self.y + fi + 1.0)
                        && self.solid_at(room, self.x - sign(final_h), self.y + fi)
                    {
                        let mut g = 0;
                        while !self.solid_at(room, self.x, self.y + 1.0) && g < 64 {
                            self.y += 1.0;
                            g += 1;
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
        if self.y > room.data.height as f32 + 100.0 {
            self.dead = true;
        }
    }

    pub fn update(&mut self, a: &Assets, room: &Room, p: &Player, fx: &mut Vec<EnemyFx>) {
        if self.state == EState::Hit {
            self.dead = true; // хитстоп закончился (его держит Game)
            return;
        }
        let (mv, turn, stun, scared, _) = self.spr();
        self.index += self.speed * a.sprite_speed(&self.sprite);
        if self.state == EState::Grabbed {
            self.speed = 0.35;
            self.follow_player(p);
            let n = a.frames(&self.sprite) as f32;
            if self.index >= n {
                self.index %= n;
            }
            return;
        }
        if self.state == EState::Thrown {
            // движение и столкновения броска обрабатывает Game
            self.speed = 0.35;
            self.sprite = stun.to_string();
            if self.hit_hsp != 0.0 {
                self.xscale = sign(-self.hit_hsp);
            }
            let n = a.frames(&self.sprite) as f32;
            if self.index >= n {
                self.index %= n;
            }
            return;
        }
        self.collision(room);

        match self.state {
            EState::Move => {
                if self.kind == Kind::Sunny {
                    self.speed = 0.35;
                    self.hsp = 0.0;
                } else if self.kind == Kind::Egghead {
                    // стоит; если игрок ближе 320x160 - разбег (chargeBuffer не используется)
                    self.speed = 0.35;
                    self.movespeed = 0.0;
                    self.hsp = 0.0;
                    self.sprite = "spr_egghead".to_string();
                    if (p.x - self.x).abs() <= 320.0 && (p.y - self.y).abs() <= 160.0 {
                        self.index = 0.0;
                        self.sprite = "spr_egghead_chargestart".to_string();
                        if p.x != self.x {
                            self.xscale = sign(p.x - self.x);
                        }
                        self.state = EState::Charge;
                    }
                } else if self.kind == Kind::Pancake {
                    // спит; просыпается, если игрок в пределах 512x96
                    self.speed = 0.35;
                    self.movespeed = 0.0;
                    self.hsp = 0.0;
                    self.sprite = "spr_pancake_sleep".to_string();
                    if (p.x - self.x).abs() <= 512.0 && (p.y - self.y).abs() <= 96.0 {
                        self.index = 0.0;
                        self.sprite = "spr_pancake_getup".to_string();
                        if p.x != self.x {
                            self.xscale = sign(p.x - self.x);
                        }
                        self.state = EState::Wake;
                        self.butter_done = false;
                    }
                } else {
                    self.speed = 0.35;
                    self.hsp = self.movespeed * self.xscale;
                    self.movespeed = 1.0;
                    if self.step_buf <= 0 {
                        self.step_buf = 45;
                        fx.push(EnemyFx::Puff(self.x, self.y + 42.0));
                    }
                    self.step_buf -= 1;
                    self.sprite = mv.to_string();

                    let wall = solid_block_at(room, self.x + self.xscale, self.y);
                    let ledge = !any_block_at(room, self.x + self.xscale * 18.0, self.y + 31.0) && self.grounded;
                    if wall || ledge {
                        if self.kind == Kind::Cocoant {
                            self.xscale *= -1.0;
                            self.index = 0.0;
                        } else {
                            self.speed = 0.35;
                            self.state = EState::Turn;
                            self.sprite = turn.to_string();
                            self.index = 0.0;
                        }
                    }
                    // пугается игрока на mach3
                    if (p.x - self.x).abs() <= 320.0
                        && (p.y - self.y).abs() <= 160.0
                        && p.state == State::Mach3
                        && !line_blocked(room, self.x, self.y, p.x, p.y)
                    {
                        self.vsp = -5.0;
                        if p.x != self.x {
                            self.xscale = sign(p.x - self.x);
                        }
                        self.sprite = scared.to_string();
                        self.scared = 120;
                        self.state = EState::Stun;
                    }
                    if self.kind == Kind::Cocoant {
                        self.attack_timer -= 1;
                    }
                }
            }
            EState::Turn => {
                self.hsp = 0.0;
                if self.anim_end(a) {
                    self.xscale *= -1.0;
                    self.sprite = mv.to_string();
                    self.index = 0.0;
                    self.state = EState::Move;
                    self.hsp = self.movespeed * self.xscale;
                }
            }
            EState::Stun => {
                self.scared -= 1;
                self.speed = 0.35;
                self.hsp = self.movespeed * self.xscale;
                if self.grounded {
                    self.movespeed = approach(self.movespeed, 0.0, 0.35);
                }
                if self.scared <= 0 {
                    self.sprite = mv.to_string();
                    self.index = 0.0;
                    self.state = EState::Move;
                    self.hsp = self.movespeed * self.xscale;
                }
                let _ = stun;
            }
            EState::Shoot => {
                self.speed = 0.35;
                if self.anim_end(a) && self.sprite == "spr_cocoant_shootprep" {
                    if p.x != self.x {
                        self.xscale = sign(p.x - self.x);
                    }
                    self.sprite = "spr_cocoant_shoot".to_string();
                    self.index = 0.0;
                    self.vsp = -4.0;
                    self.hsp = -4.0 * self.xscale;
                    fx.push(EnemyFx::Shoot(self.x, self.y, self.xscale));
                }
                if self.anim_end(a) && self.sprite == "spr_cocoant_shoot" {
                    self.index = 0.0;
                    self.sprite = mv.to_string();
                    self.state = EState::Move;
                }
                self.movespeed = 0.0;
            }
            EState::Charge => {
                self.speed = 0.4;
                self.hsp = self.movespeed * self.xscale;
                if self.sprite == "spr_egghead_charge" {
                    self.movespeed = 8.0;
                    if solid_block_at(room, self.x + self.hsp, self.y - 7.0) {
                        // врезался в стену - оглушён
                        self.scared = 120;
                        self.vsp = -5.0;
                        self.state = EState::Stun;
                        self.movespeed = -5.0;
                        self.sprite = stun.to_string();
                    }
                }
                if self.anim_end(a) && self.sprite == "spr_egghead_chargestart" {
                    self.sprite = "spr_egghead_charge".to_string();
                    self.index = 0.0;
                }
            }
            EState::Wake => {
                self.speed = 0.4;
                self.sprite = "spr_pancake_getup".to_string();
                if self.index.floor() >= 10.0 && !self.butter_done {
                    self.butter_done = true;
                    fx.push(EnemyFx::Butter(self.x, self.y, self.xscale));
                }
                if self.anim_end(a) {
                    if p.x != self.x {
                        self.xscale = -sign(p.x - self.x);
                    }
                    self.state = EState::Run;
                    self.sprite = mv.to_string();
                    self.index = 0.0;
                }
            }
            EState::Run => {
                self.speed = 0.35;
                self.hsp = self.movespeed * self.xscale;
                self.movespeed = 8.0;
                self.sprite = mv.to_string();
                if solid_block_at(room, self.x + self.xscale, self.y) && self.grounded {
                    self.xscale *= -1.0;
                }
            }
            EState::Hit | EState::Grabbed | EState::Thrown => {}
        }

        // cocoant: начинает атаку, если игрок рядом
        if self.kind == Kind::Cocoant
            && self.attack_timer <= 0
            && (p.x - self.x).abs() <= 320.0
            && (p.y - self.y).abs() <= 42.0
            && self.state == EState::Move
        {
            self.attack_timer = 120;
            self.state = EState::Shoot;
            self.sprite = "spr_cocoant_shootprep".to_string();
            self.index = 0.0;
            if p.x != self.x {
                self.xscale = sign(p.x - self.x);
            }
        }

        let n = a.frames(&self.sprite) as f32;
        if self.index >= n {
            self.index %= n;
        }
    }
}

/// obj_baddie/Step_2: столкновения врагов с игроком
pub fn interact(enemies: &mut [Enemy], player: &mut Player, input: &crate::player::Input, out: &mut Vec<Hit>) {
    let jump_held = input.jump;
    if player.noclip {
        return;
    }
    let (pl, pt, pr, pb) = player.bbox();
    let insta = player.instakill();
    for e in enemies.iter_mut() {
        if e.dead || matches!(e.state, EState::Hit | EState::Grabbed | EState::Thrown) {
            continue;
        }
        let (el, et, er, eb) = e.rect();
        let overlaps = |dx: f32, dy: f32| pl + dx <= er && pr + dx >= el && pt + dy <= eb && pb + dy >= et;
        let stun_spr = e.spr().2;

        // прыжок сверху: отскок, враг оглушён
        if player.state == State::Jump && player.vsp > 0.0 && player.y < e.y && overlaps(0.0, player.vsp * 2.0) {
            player.vsp = if jump_held && !player.grounded { -15.0 } else { -10.0 };
            player.jumpstop = true;
            player.set("spr_player_bumpenemystart", 0.0);
            e.vsp = -5.0;
            e.xscale = player.xscale;
            e.scared = 120;
            e.state = EState::Stun;
            e.movespeed = 5.0 * e.xscale;
            e.sprite = stun_spr.to_string();
            e.index = 0.0;
            out.push(Hit::Bump(e.x, e.y));
            continue;
        }

        if overlaps(player.hsp * 2.0, player.vsp) {
            // grab dash: враг оказывается на руках (hauling)
            if player.state == State::Grab && player.haul_uid.is_none() {
                player.state = State::Hauling;
                player.set("spr_player_haulingland", 0.0);
                player.haul_uid = Some(e.uid);
                if !player.grounded {
                    player.jumpstop = true;
                    player.vsp = -6.0;
                }
                e.state = EState::Grabbed;
                e.sprite = stun_spr.to_string();
                if input.up {
                    // ↑ при захвате: сразу piledriver
                    player.vsp = -14.0;
                    player.set("spr_player_piledriver", 0.0);
                    player.state = State::GroundPound;
                    player.movespeed = player.hsp;
                }
                out.push(Hit::Grab(e.x, e.y));
                continue;
            }
            let bashing = matches!(player.state, State::Mach2 | State::Tumble);
            if bashing && e.scared < 165 && !insta {
                // mach2 / скольжение: враг отлетает и оглушён
                e.vsp = (player.y - 180.0 - e.y) / 60.0;
                e.xscale = -player.xscale;
                e.scared = 180;
                e.state = EState::Stun;
                e.movespeed = -12.0;
                e.sprite = stun_spr.to_string();
                e.index = 0.0;
                out.push(Hit::Bash(e.x, e.y));
            } else if insta {
                // mach3 / пике / разворот на mach3: мгновенное убийство
                e.state = EState::Hit;
                e.sprite = stun_spr.to_string();
                if player.state == State::Mach3 && player.sprite == "spr_player_mach3" {
                    player.set("spr_player_mach3hit", 0.0);
                }
                if jump_held && !player.grounded {
                    player.vsp = -10.0;
                }
                out.push(Hit::Kill(e.x, e.y));
            } else if ((e.kind == Kind::Dougknight && e.state == EState::Move)
                || (e.kind == Kind::Egghead && e.state == EState::Charge && e.sprite == "spr_egghead_charge"))
                && player.hurt(e.x)
            {
                out.push(Hit::Hurt);
            }
        }
    }
}

pub fn update_darts(darts: &mut Vec<Dart>, room: &Room, a: &Assets, player: &mut Player, out: &mut Vec<Hit>) {
    let meta = a.sprites.get("spr_cocoantDart");
    for d in darts.iter_mut() {
        d.frame += 0.35;
        d.x += 12.0 * d.dir;
    }
    let (pl, pt, pr, pb) = player.bbox();
    darts.retain(|d| {
        let (l, t, r, b) = match meta {
            Some(m) => crate::world::inst_bbox(m, d.x, d.y, d.dir, 1.0),
            None => (d.x, d.y, d.x, d.y),
        };
        // исчезает в стене
        if room.blocks.iter().any(|k| matches!(k.kind, BlockKind::Solid | BlockKind::Slope) && k.hits(l, t, r, b)) {
            return false;
        }
        // ранит игрока
        if !player.noclip && l <= pr && r >= pl && t <= pb && b >= pt && player.hurt(d.x) {
            out.push(Hit::Hurt);
            return false;
        }
        true
    });
}
