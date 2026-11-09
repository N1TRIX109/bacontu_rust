//! Рантайм-представление комнаты: блоки столкновений, двери, переходы, подбираемые предметы.

use crate::assets::{Assets, RoomData, SpriteMeta};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    Solid,
    Slope,
    Platform,
}

/// Блок столкновения. Границы включительные (как bbox_* в GameMaker).
#[derive(Clone, Debug)]
pub struct Block {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
    pub kind: BlockKind,
    pub sx: f32,
    pub sy: f32,
    pub destructible: bool,
    /// Железный ящик: ломается только пикированием (Dive) или бегом на mach3
    pub metal: bool,
    pub alive: bool,
    /// 0 - всегда, 1 - только вне побега (monolithblock_wake), 2 - только в побег (monolithblock_sleep),
    /// 3 - obj_tutorialBlock (пока туториал не пройден). Game выставляет `enabled`.
    pub mode: u8,
    pub enabled: bool,
    pub obj: String,
}

impl Block {
    pub fn hits(&self, l: f32, t: f32, r: f32, b: f32) -> bool {
        self.alive && self.enabled && self.l <= r && self.r >= l && self.t <= b && self.b >= t
    }
}

/// bbox экземпляра: спрайт/маска, origin и image_xscale/yscale (включая отрицательные).
pub fn inst_bbox(m: &SpriteMeta, x: f32, y: f32, sx: f32, sy: f32) -> (f32, f32, f32, f32) {
    let a = x + (m.bl - m.xo) as f32 * sx;
    let b = x + (m.br + 1 - m.xo) as f32 * sx;
    let c = y + (m.bt - m.yo) as f32 * sy;
    let d = y + (m.bb + 1 - m.yo) as f32 * sy;
    (a.min(b), c.min(d), a.max(b) - 1.0, c.max(d) - 1.0)
}

#[derive(Clone, Debug)]
pub struct Door {
    pub letter: String,
    pub x: f32,
    pub y: f32,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Hallway {
    pub rect: [f32; 4],
    pub target_room: String,
    pub target_door: String,
    pub vertical: bool,
    /// sign(image_xscale) для горизонтальных, sign(image_yscale) для вертикальных
    pub dir: f32,
    pub width: f32,
    pub x: f32,
}

/// obj_door (обычная дверь в комнату) и obj_levelgate (врата уровня).
#[derive(Clone, Debug)]
pub struct Portal {
    pub rect: [f32; 4],
    pub x: f32,
    pub y: f32,
    pub target_room: String,
    pub target_door: String,
    pub is_gate: bool,
    /// obj_exitgate: выход из уровня (работает только во время побега)
    pub is_exit: bool,
    /// obj_door с `if global.escape.active instance_destroy()`
    pub hide_in_escape: bool,
    /// буква двери под воротами (куда возвращаться после уровня)
    pub home_door: String,
}

/// obj_sprayWall: стена для покраски (Rock Bottom)
#[derive(Clone, Debug)]
pub struct Spray {
    pub rect: [f32; 4],
    pub x: f32,
    pub y: f32,
    pub active: bool,
    pub points: i32,
    pub alarm: i32,
}

/// obj_bus: такси - выход из этапа Rock Bottom
#[derive(Clone, Debug)]
pub struct Bus {
    pub x: f32,
    pub y: f32,
    pub start_x: f32,
    pub target_room: String,
    pub target_door: String,
}

/// obj_secret_heart: вход в секрет / выход из него
#[derive(Clone, Debug)]
pub struct SecretHeart {
    pub x: f32,
    pub y: f32,
    pub rect: [f32; 4],
    pub target_room: String,
}

/// obj_ballofbeer: бочка-портал. dir = 1 - вход сверху (↓), dir = -1 - висит на потолке (↑)
#[derive(Clone, Debug)]
pub struct Beer {
    pub rect: [f32; 4],
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub target_room: String,
    pub target_door: String,
}

/// Точка появления врага (obj_cocoant / dougknight / slosher / sunny)
#[derive(Clone, Debug)]
pub struct EnemySpawn {
    pub obj: String,
    pub x: f32,
    pub y: f32,
    pub xscale: f32,
    pub plosher: bool,
    /// появляется только во время побега (instance var `escape`)
    pub escape: bool,
}

/// obj_potable_cage: клетка с топпином
#[derive(Clone, Debug)]
pub struct Cage {
    pub x: f32,
    pub y: f32,
    pub rect: [f32; 4],
    pub content: String,
    pub frame: f32,
}

/// obj_tutorialbook: Flo - NPC с подсказками туториала
#[derive(Clone, Debug)]
pub struct Book {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub hide_in_escape: bool,
    pub frame: f32,
    pub talking: bool,
}

/// obj_enemyspawner: выпускает врага, когда игрок ближе 300 px
#[derive(Clone, Debug)]
pub struct Spawner {
    pub x: f32,
    pub y: f32,
    pub xscale: f32,
    pub content: String,
    pub frame: f32,
    pub speed: f32,
}

#[derive(Clone, Debug)]
pub struct Pickup {
    /// коллект побега (obj_collect_escape / obj_bigcollect_escape): виден всегда, собирается только в pizza time
    pub escape: bool,
    pub x: f32,
    pub y: f32,
    pub x0: f32,
    pub y0: f32,
    pub homing: bool,
    pub speed: f32,
    pub value: i32,
    pub sprite: String,
    pub frame: f32,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub struct Decor {
    pub sprite: String,
    pub x: f32,
    pub y: f32,
    pub sx: f32,
    pub sy: f32,
    pub depth: i32,
    pub frame: f32,
    pub animate: bool,
    /// Враг (потомок obj_baddie): можно убить пикированием
    pub enemy: bool,
    pub dead: bool,
    pub rect: [f32; 4],
    /// Индекс блока, чьё разрушение скрывает этот спрайт
    pub block: Option<usize>,
    /// image_speed объекта (0 - статичный кадр)
    pub speed: f32,
    /// спрайт повторяется плиткой 32x32 по размеру инстанса (obj_monolithblock_*)
    pub tiled: bool,
    /// как Block.mode: 1 вне побега, 2 в побег, 3 до конца туториала
    pub mode: u8,
    pub hide_in_escape: bool,
    /// слои фона ворот (obj_levelgate)
    pub gate: Option<Vec<(String, i32, f32, f32)>>,
}

pub struct Room {
    pub data: RoomData,
    pub blocks: Vec<Block>,
    pub doors: Vec<Door>,
    pub hallways: Vec<Hallway>,
    pub portals: Vec<Portal>,
    pub beers: Vec<Beer>,
    pub enemy_spawns: Vec<EnemySpawn>,
    pub cages: Vec<Cage>,
    pub books: Vec<Book>,
    pub butters: Vec<(f32, f32)>,
    pub paint_pickups: Vec<(f32, f32)>,
    pub sprays: Vec<Spray>,
    pub buses: Vec<Bus>,
    pub detectives: Vec<(f32, f32)>,
    pub hearts: Vec<SecretHeart>,
    pub heart_enters: Vec<(f32, f32)>,
    pub ladders: Vec<[f32; 4]>,
    pub spawners: Vec<Spawner>,
    pub pickups: Vec<Pickup>,
    pub decor: Vec<Decor>,
}

fn rect_hit(a: &[f32; 4], l: f32, t: f32, r: f32, b: f32) -> bool {
    a[0] <= r && a[2] >= l && a[1] <= b && a[3] >= t
}

const NEVER_DRAWN: &[&str] = &[
    "obj_solid", "obj_slope", "obj_platform", "obj_hallway", "obj_hallway_vertical",
    "obj_escape_pointer", "obj_secretManager", "obj_treasureViewer", "obj_checkpoint",
];

impl Room {
    pub fn build(assets: &Assets, data: RoomData) -> Room {
        let mut blocks = Vec::new();
        let mut doors = Vec::new();
        let mut hallways = Vec::new();
        let mut portals = Vec::new();
        let mut beers: Vec<Beer> = Vec::new();
        let mut enemy_spawns: Vec<EnemySpawn> = Vec::new();
        let mut cages: Vec<Cage> = Vec::new();
        let mut books: Vec<Book> = Vec::new();
        let mut ladders: Vec<[f32; 4]> = Vec::new();
        let mut butters: Vec<(f32, f32)> = Vec::new();
        let mut paint_pickups: Vec<(f32, f32)> = Vec::new();
        let mut sprays: Vec<Spray> = Vec::new();
        let mut buses: Vec<Bus> = Vec::new();
        let mut detectives: Vec<(f32, f32)> = Vec::new();
        let mut hearts: Vec<SecretHeart> = Vec::new();
        let mut heart_enters: Vec<(f32, f32)> = Vec::new();
        let mut spawners: Vec<Spawner> = Vec::new();
        let mut pickups = Vec::new();
        let mut decor = Vec::new();

        for inst in &data.instances {
            let obj = inst.obj.as_str();
            let sprite = assets
                .objects
                .get(obj)
                .and_then(|o| o.mask.clone().or(o.sprite.clone()));
            let meta = sprite.as_ref().and_then(|s| assets.sprites.get(s));
            let bbox = meta.map(|m| inst_bbox(m, inst.x, inst.y, inst.sx, inst.sy));

            // --- столкновения ---
            let kind = if assets.is_a(obj, "obj_slope") {
                Some(BlockKind::Slope)
            } else if assets.is_a(obj, "obj_platform") {
                Some(BlockKind::Platform)
            } else if assets.is_a(obj, "obj_solid") {
                Some(BlockKind::Solid)
            } else {
                None
            };
            let mut block_index = None;
            if let (Some(kind), Some((l, t, r, b))) = (kind, bbox) {
                block_index = Some(blocks.len());
                blocks.push(Block {
                    l, t, r, b, kind,
                    sx: inst.sx,
                    sy: inst.sy,
                    destructible: assets.is_a(obj, "obj_destructibles"),
                    metal: obj == "obj_metalblock" || obj == "obj_secretmetalblock",
                    alive: true,
                    mode: match obj {
                        "obj_monolithblock_wake" => 1,
                        "obj_monolithblock_sleep" => 2,
                        "obj_tutorialBlock" => 3,
                        _ => 0,
                    },
                    enabled: true,
                    obj: obj.to_string(),
                });
            }

            // --- логика по типам ---
            if let Some(letter) = obj.strip_prefix("obj_door") {
                if letter.len() == 1 {
                    if let Some((l, t, r, b)) = bbox {
                        doors.push(Door { letter: letter.to_string(), x: inst.x, y: inst.y, rect: [l, t, r, b] });
                    }
                    continue;
                }
            }
            if assets.is_a(obj, "obj_baddie") {
                enemy_spawns.push(EnemySpawn {
                    obj: obj.to_string(),
                    x: inst.x,
                    y: inst.y,
                    xscale: inst.sx,
                    plosher: inst.prop("plosher").as_deref() == Some("True"),
                    escape: inst.prop("escape").as_deref() == Some("True"),
                });
                continue;
            }
            // --- Rock Bottom ---
            match obj {
                "obj_slipperybutter" => {
                    butters.push((inst.x, inst.y));
                    continue;
                }
                "obj_paintbuddy_pickup" => {
                    paint_pickups.push((inst.x, inst.y));
                    continue;
                }
                "obj_sprayWall" => {
                    let (l, t, r, b) = bbox.unwrap_or((inst.x, inst.y, inst.x, inst.y));
                    sprays.push(Spray { rect: [l, t, r, b], x: inst.x, y: inst.y, active: false, points: 150, alarm: 0 });
                    continue;
                }
                "obj_bus" => {
                    buses.push(Bus {
                        x: inst.x,
                        y: inst.y,
                        start_x: inst.x,
                        target_room: inst.prop("targetRoom").unwrap_or_default(),
                        target_door: String::new(),
                    });
                    continue;
                }
                "obj_detectivePickup" => {
                    detectives.push((inst.x, inst.y));
                    continue;
                }
                "obj_secret_heart" => {
                    let (l, t, r, b) = bbox.unwrap_or((inst.x, inst.y, inst.x, inst.y));
                    hearts.push(SecretHeart { x: inst.x, y: inst.y, rect: [l, t, r, b], target_room: inst.prop("targetRoom").unwrap_or_default() });
                    continue;
                }
                "obj_secret_heart_enter" => {
                    heart_enters.push((inst.x, inst.y));
                    continue;
                }
                _ => {}
            }
            if obj == "obj_ladder" {
                if let Some((l, t, r, b)) = bbox {
                    ladders.push([l, t, r, b]);
                }
                // спрайт лестницы рисуется как обычный декор (ниже)
            }
            if obj == "obj_tutorialbook" {
                books.push(Book {
                    x: inst.x,
                    y: inst.y,
                    text: inst.text.clone().unwrap_or_else(|| "Hey bro!".to_string()),
                    hide_in_escape: inst.hide_in_escape,
                    frame: 0.0,
                    talking: false,
                });
                continue;
            }
            if obj == "obj_enemyspawner" {
                spawners.push(Spawner {
                    x: inst.x,
                    y: inst.y,
                    xscale: inst.sx,
                    content: inst.prop("content").unwrap_or_else(|| "obj_slosher".to_string()),
                    frame: 0.0,
                    speed: 0.0,
                });
                continue;
            }
            if obj == "obj_potable_cage" {
                let (l, t, r, b) = bbox.unwrap_or((inst.x, inst.y, inst.x, inst.y));
                cages.push(Cage {
                    x: inst.x,
                    y: inst.y,
                    rect: [l, t, r, b],
                    content: inst.prop("content").unwrap_or_else(|| "obj_potableBean".to_string()),
                    frame: 0.0,
                });
                continue;
            }
            match obj {
                "obj_hallway" | "obj_hallway_vertical" => {
                    if let Some((l, t, r, b)) = bbox {
                        let vertical = obj == "obj_hallway_vertical";
                        hallways.push(Hallway {
                            rect: [l, t, r, b],
                            target_room: inst.prop("targetRoom").unwrap_or_default(),
                            target_door: String::new(), // заполняется ниже
                            vertical,
                            dir: if vertical { inst.sy.signum() } else { inst.sx.signum() },
                            width: r - l + 1.0,
                            x: inst.x,
                        });
                    }
                    continue;
                }
                "obj_door" | "obj_levelgate" | "obj_exitgate" => {
                    let is_gate = obj == "obj_levelgate";
                    let (l, t, r, b) = bbox.unwrap_or((inst.x, inst.y, inst.x + 1.0, inst.y + 1.0));
                    portals.push(Portal {
                        rect: [l, t, r, b],
                        x: inst.x,
                        y: inst.y,
                        // значения по умолчанию взяты из obj_levelgate.yy
                        target_room: inst
                            .prop("targetRoom")
                            .unwrap_or_else(|| if is_gate { "wayback_1".into() } else { String::new() }),
                        target_door: if is_gate { "A".into() } else { inst.prop("targetDoor").unwrap_or_default() },
                        is_gate,
                        is_exit: obj == "obj_exitgate",
                        hide_in_escape: inst.hide_in_escape,
                        home_door: String::new(),
                    });
                }
                "obj_ballofbeer" => {
                    if let Some((l, t, r, b)) = bbox {
                        beers.push(Beer {
                            rect: [l, t, r, b],
                            x: inst.x,
                            y: inst.y,
                            dir: inst.sy.signum(),
                            target_room: inst.prop("targetRoom").unwrap_or_default(),
                            target_door: String::new(),
                        });
                    }
                    // дальше - как обычный блок со спрайтом (декор)
                }
                "obj_collect" | "obj_bigcollect" | "obj_collect_escape" | "obj_bigcollect_escape" => {
                    let big = obj == "obj_bigcollect";
                    let escape = obj.ends_with("_escape");
                    pickups.push(Pickup {
                        escape,
                        x: inst.x,
                        y: inst.y,
                        x0: inst.x,
                        y0: inst.y,
                        homing: false,
                        speed: 8.0,
                        value: if big { 100 } else if obj == "obj_bigcollect_escape" { 20 } else { 10 },
                        sprite: assets.sprite_of(obj).unwrap_or("").to_string(),
                        frame: 0.0,
                        alive: true,
                    });
                    continue;
                }
                _ => {}
            }

            // --- декор / враги / блоки со спрайтами ---
            if NEVER_DRAWN.contains(&obj) || obj.ends_with("_escape") || (obj.len() == 9 && obj.starts_with("obj_door")) {
                continue;
            }
            if let Some(s) = assets.sprite_of(obj) {
                if assets.sprites.contains_key(s) {
                    let (spd, idx) = assets.obj_anim(obj);
                    let mut sprite_name = s.to_string();
                    let mut speed = spd.unwrap_or(0.35);
                    let mut frame = idx.unwrap_or(0.0);
                    let mut mode = 0u8;
                    let mut tiled = false;
                    let mut gate = None;
                    match obj {
                        "obj_monolithblock_wake" => {
                            mode = 1;
                            tiled = true;
                        }
                        "obj_monolithblock_sleep" => {
                            // в побег этот блок рисуется спрайтом wake, кадр 0
                            sprite_name = "spr_monolithblock_wake".to_string();
                            speed = 0.0;
                            frame = 0.0;
                            mode = 2;
                            tiled = true;
                        }
                        "obj_tutorialBlock" => mode = 3,
                        "obj_levelgate" => {
                            speed = 0.0;
                            gate = Some(inst.gate_layers.clone().unwrap_or_else(|| {
                                // значения по умолчанию из obj_levelgate/Create_0 (уровень wayback)
                                vec![
                                    ("spr_gate_waybackBG".to_string(), 3, 0.0, 100.0),
                                    ("spr_gate_waybackBG".to_string(), 2, 0.0, 0.0),
                                    ("spr_gate_waybackBG".to_string(), 1, 0.0, 0.0),
                                    ("spr_gate_waybackBG".to_string(), 0, 0.0, 0.0),
                                ]
                            }));
                        }
                        _ => {}
                    }
                    decor.push(Decor {
                        animate: speed > 0.0 && assets.frames(&sprite_name) > 1,
                        sprite: sprite_name,
                        x: inst.x,
                        y: inst.y,
                        sx: inst.sx,
                        sy: inst.sy,
                        depth: inst.depth,
                        frame,
                        enemy: false,
                        dead: false,
                        rect: bbox.map(|b| [b.0, b.1, b.2, b.3]).unwrap_or([inst.x, inst.y, inst.x, inst.y]),
                        block: block_index,
                        speed,
                        tiled,
                        mode,
                        hide_in_escape: inst.hide_in_escape,
                        gate,
                    });
                }
            }
        }

        // Как obj_hallway/Other_4: какую букву двери накрывает холлвей
        for h in hallways.iter_mut() {
            for d in &doors {
                let [l, t, r, b] = d.rect;
                if rect_hit(&h.rect, l, t, r, b) {
                    h.target_door = d.letter.clone();
                }
            }
        }

        // obj_door/Other_4: targetDoor = буква obj_doorX, которую накрывает сама дверь.
        // Игрок появится у двери с этой буквой в целевой комнате. Ворота: так же ищем "домашнюю" дверь.
        for q in portals.iter_mut().filter(|q| !q.is_exit) {
            for letter in ["A", "B", "C", "D", "E", "F", "G", "H", "S"] {
                for d in doors.iter().filter(|d| d.letter == letter) {
                    if rect_hit(&d.rect, q.rect[0], q.rect[1], q.rect[2], q.rect[3]) {
                        if q.is_gate {
                            q.home_door = letter.to_string();
                        } else {
                            q.target_door = letter.to_string();
                        }
                    }
                }
            }
        }

        // obj_bus/Other_4: целевая дверь = буква двери под такси (позиция по умолчанию)
        for bus in buses.iter_mut() {
            for letter in ["A", "B", "C", "D", "E", "F", "G", "H", "S"] {
                for d in doors.iter().filter(|d| d.letter == letter) {
                    if rect_hit(&d.rect, bus.x - 32.0, bus.y - 32.0, bus.x + 32.0, bus.y + 32.0) {
                        bus.target_door = letter.to_string();
                    }
                }
            }
        }

        // obj_ballofbeer/Other_4: ищем дверь в пределах 12 px над/под бочкой
        for beer in beers.iter_mut() {
            for letter in ["A", "B", "C", "D", "E", "F", "G", "H", "S"] {
                for d in doors.iter().filter(|d| d.letter == letter) {
                    for off in [-12.0, 12.0] {
                        if rect_hit(&d.rect, beer.rect[0], beer.rect[1] + off, beer.rect[2], beer.rect[3] + off) {
                            beer.target_door = letter.to_string();
                        }
                    }
                }
            }
        }

        Room { data, blocks, doors, hallways, portals, beers, enemy_spawns, cages, books, butters, paint_pickups, sprays, buses, detectives, hearts, heart_enters, ladders, spawners, pickups, decor }
    }

    pub fn door(&self, letter: &str) -> Option<&Door> {
        self.doors.iter().find(|d| d.letter == letter)
    }
}
