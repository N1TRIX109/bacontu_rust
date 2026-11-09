//! Сохранения в формате ini, как в оригинале: saves/saveDate_1..3.ini (слоты) и saves/saveData.ini (настройки).
//! В слоте: [General] created, [Game] Palette / TutorialDone / Treasure, [forest] Rank / Highscore, секреты, клетки.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

pub const SAVE_DIR: &str = "saves";

#[derive(Default, Clone)]
pub struct Ini {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        let mut cur = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                cur = line[1..line.len() - 1].to_string();
            } else if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"').to_string();
                ini.sections.entry(cur.clone()).or_default().insert(k.trim().to_string(), v);
            }
        }
        ini
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (sec, kv) in &self.sections {
            out.push_str(&format!("[{sec}]\n"));
            for (k, v) in kv {
                out.push_str(&format!("{k}=\"{v}\"\n"));
            }
            out.push('\n');
        }
        out
    }

    pub fn get(&self, sec: &str, key: &str) -> Option<&str> {
        self.sections.get(sec).and_then(|s| s.get(key)).map(|s| s.as_str())
    }

    pub fn get_i(&self, sec: &str, key: &str, default: i32) -> i32 {
        self.get(sec, key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    pub fn set(&mut self, sec: &str, key: &str, val: impl ToString) {
        self.sections.entry(sec.to_string()).or_default().insert(key.to_string(), val.to_string());
    }

    pub fn keys(&self, sec: &str) -> Vec<String> {
        self.sections.get(sec).map(|s| s.keys().cloned().collect()).unwrap_or_default()
    }
}

fn read_ini(path: &str) -> Ini {
    fs::read_to_string(path).map(|t| Ini::parse(&t)).unwrap_or_default()
}

fn write_ini(path: &str, ini: &Ini) {
    let _ = fs::create_dir_all(SAVE_DIR);
    if let Err(e) = fs::write(path, ini.to_text()) {
        eprintln!("не удалось записать {path}: {e}");
    }
}

/// Ранги уровня (obj_hud: Srank; Arank = S/2, Brank = A/2, Crank = B/2)
pub fn rank_for(level: &str, score: i32) -> &'static str {
    let s = match level {
        "forest" => 19000,
        "city" => 30000,
        _ => return "D",
    };
    let (a, b, c) = (s / 2, s / 4, s / 8);
    if score >= s {
        "S"
    } else if score >= a {
        "A"
    } else if score >= b {
        "B"
    } else if score >= c {
        "C"
    } else {
        "D"
    }
}

pub fn rank_num(rank: &str) -> i32 {
    match rank {
        "C" => 1,
        "B" => 2,
        "A" => 3,
        "S" => 4,
        "P" => 5,
        _ => 0,
    }
}

/// Один слот сохранения
#[derive(Default, Clone)]
pub struct Slot {
    pub created: bool,
    pub palette: i32,
    pub tutorial_done: bool,
    pub treasure: bool,
    /// level -> (ранг, рекорд)
    pub levels: BTreeMap<String, (String, i32)>,
    pub secrets: BTreeSet<String>,
    /// разбитые клетки с топпинами: "комната:x:y"
    pub cages: BTreeSet<String>,
}

impl Slot {
    pub fn path(n: usize) -> String {
        format!("{SAVE_DIR}/saveDate_{}.ini", n + 1)
    }

    pub fn load(n: usize) -> Slot {
        let ini = read_ini(&Slot::path(n));
        let mut s = Slot {
            created: ini.get_i("General", "created", 0) != 0,
            palette: ini.get_i("Game", "Palette", 1),
            tutorial_done: ini.get_i("Game", "TutorialDone", 0) != 0,
            treasure: ini.get_i("Game", "Treasure", 0) != 0,
            ..Default::default()
        };
        for lvl in ["forest", "tutorial", "city"] {
            if let Some(r) = ini.get(lvl, "Rank") {
                s.levels.insert(lvl.to_string(), (r.to_string(), ini.get_i(lvl, "Highscore", 0)));
            }
        }
        for k in ini.keys("Secrets") {
            s.secrets.insert(k);
        }
        for k in ini.keys("Cages") {
            s.cages.insert(k);
        }
        s
    }

    pub fn save(&self, n: usize) {
        let mut ini = Ini::default();
        ini.set("General", "created", if self.created { 1 } else { 0 });
        ini.set("Game", "Palette", self.palette);
        ini.set("Game", "TutorialDone", if self.tutorial_done { 1 } else { 0 });
        ini.set("Game", "Treasure", if self.treasure { 1 } else { 0 });
        for (lvl, (rank, score)) in &self.levels {
            ini.set(lvl, "Rank", rank);
            ini.set(lvl, "Highscore", score);
        }
        for s in &self.secrets {
            ini.set("Secrets", s, 1);
        }
        for c in &self.cages {
            ini.set("Cages", c, 1);
        }
        write_ini(&Slot::path(n), &ini);
    }

    pub fn delete(n: usize) {
        let _ = fs::remove_file(Slot::path(n));
    }

    /// Процент прохождения (как в obj_mainmenu: сумма рангов / (число уровней * 5))
    pub fn completion(&self) -> f32 {
        let n: i32 = ["forest", "city"].iter().map(|l| self.levels.get(*l).map(|(r, _)| rank_num(r)).unwrap_or(0)).sum();
        n as f32 / 10.0 * 100.0
    }

    /// Записывает результат уровня: рекорд и лучший ранг
    pub fn record_level(&mut self, level: &str, score: i32) {
        let rank = rank_for(level, score);
        let entry = self.levels.entry(level.to_string()).or_insert(("D".to_string(), 0));
        if rank_num(rank) > rank_num(&entry.0) {
            entry.0 = rank.to_string();
        }
        entry.1 = entry.1.max(score);
    }
}

/// Настройки (saves/saveData.ini): громкости 0..10 и полный экран
#[derive(Clone)]
pub struct Options {
    pub master: i32,
    pub music: i32,
    pub sfx: i32,
    pub ambience: i32,
    pub fullscreen: bool,
    /// шейдеры экрана (эффект побега); global.option_screen_effects
    pub screen_effects: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { master: 10, music: 10, sfx: 10, ambience: 10, fullscreen: false, screen_effects: true }
    }
}

impl Options {
    const PATH: &'static str = "saves/saveData.ini";

    pub fn load() -> Options {
        let ini = read_ini(Options::PATH);
        let d = Options::default();
        Options {
            master: ini.get_i("Audio", "Master", d.master).clamp(0, 10),
            music: ini.get_i("Audio", "Music", d.music).clamp(0, 10),
            sfx: ini.get_i("Audio", "Sfx", d.sfx).clamp(0, 10),
            ambience: ini.get_i("Audio", "Ambience", d.ambience).clamp(0, 10),
            fullscreen: ini.get_i("Video", "Fullscreen", 0) != 0,
            screen_effects: ini.get_i("Video", "ScreenEffects", 1) != 0,
        }
    }

    pub fn save(&self) {
        let mut ini = Ini::default();
        ini.set("Audio", "Master", self.master);
        ini.set("Audio", "Music", self.music);
        ini.set("Audio", "Sfx", self.sfx);
        ini.set("Audio", "Ambience", self.ambience);
        ini.set("Video", "Fullscreen", if self.fullscreen { 1 } else { 0 });
        ini.set("Video", "ScreenEffects", if self.screen_effects { 1 } else { 0 });
        write_ini(Options::PATH, &ini);
    }
}
