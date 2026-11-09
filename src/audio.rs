//! Звук: короткие эффекты, зацикленная музыка комнат, атмосфера и петля бега (mach).
//! Файлы лежат в assets/audio/<ключ>.ogg (их делает tools/convert_audio.py).

use macroquad::audio::{load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound};
use std::collections::HashMap;

const KEYS: &[&str] = &[
    "music_hub", "music_wayback", "music_wayback_secret", "amb_wayback",
    "jump", "step", "mach2loop", "mach3loop", "maxspeed", "machstart", "bump",
    "mach2bump1", "mach2bump2", "mach2bump3", "mach3bump", "machturn", "machslide",
    "noclip_on", "door", "breakblock", "collect", "bigcollect", "secret",
    "killenemy", "dive", "divelanding",
    "music_escape", "escape_start", "hurt", "punch", "toppin", "cagebreak", "tick",
    "grab", "superjump", "superjumpprep", "uppercut",
    "music_pause", "music_menu", "music_city", "music_city_secret", "jingle_city", "menu_move", "menu_select", "menu_back", "tvstatic",
];

pub struct Audio {
    sounds: HashMap<&'static str, Sound>,
    music: Option<&'static str>,
    ambience: Option<&'static str>,
    looped: Option<&'static str>,
    /// громкости из настроек: master, music, sfx, ambience (0..1)
    pub vol: [f32; 4],
}

const MUSIC_VOL: f32 = 0.55;
const AMB_VOL: f32 = 0.25;
const LOOP_VOL: f32 = 0.4;

impl Audio {
    pub async fn load(root: &str) -> Audio {
        let mut sounds = HashMap::new();
        for &key in KEYS {
            let path = format!("{root}/audio/{key}.ogg");
            match std::fs::read(&path) {
                Ok(bytes) => match load_sound_from_bytes(&bytes).await {
                    Ok(s) => {
                        sounds.insert(key, s);
                    }
                    Err(e) => eprintln!("звук {path}: не декодируется ({e:?})"),
                },
                Err(_) => eprintln!("звук {path}: файл не найден (tools/convert_audio.py)"),
            }
        }
        Audio { sounds, music: None, ambience: None, looped: None, vol: [1.0; 4] }
    }

    pub fn empty() -> Audio {
        Audio { sounds: HashMap::new(), music: None, ambience: None, looped: None, vol: [1.0; 4] }
    }

    /// Меняет громкости (0..10 из настроек) и сразу применяет к играющим трекам
    pub fn set_volumes(&mut self, master: i32, music: i32, sfx: i32, ambience: i32) {
        self.vol = [master as f32 / 10.0, music as f32 / 10.0, sfx as f32 / 10.0, ambience as f32 / 10.0];
        let apply = |name: Option<&'static str>, v: f32| {
            if let Some(n) = name {
                if let Some(s) = self.sounds.get(n) {
                    set_sound_volume(s, v);
                }
            }
        };
        apply(self.music, MUSIC_VOL * self.vol[0] * self.vol[1]);
        apply(self.ambience, AMB_VOL * self.vol[0] * self.vol[3]);
        apply(self.looped, LOOP_VOL * self.vol[0] * self.vol[2]);
    }

    pub fn stop_all(&mut self) {
        for s in self.sounds.values() {
            stop_sound(s);
        }
        self.music = None;
        self.ambience = None;
        self.looped = None;
    }

    pub fn sfx(&self, name: &str, volume: f32) {
        if let Some(s) = self.sounds.get(name) {
            play_sound(s, PlaySoundParams { looped: false, volume: volume * self.vol[0] * self.vol[2] });
        }
    }

    fn switch(&self, old: Option<&'static str>, new: Option<&'static str>, volume: f32) {
        if let Some(o) = old {
            if let Some(s) = self.sounds.get(o) {
                stop_sound(s);
            }
        }
        if let Some(n) = new {
            if let Some(s) = self.sounds.get(n) {
                play_sound(s, PlaySoundParams { looped: true, volume });
                set_sound_volume(s, volume);
            }
        }
    }

    /// Музыка не перезапускается, пока ключ тот же (как FMOD-событие внутри одного уровня)
    pub fn set_music(&mut self, name: Option<&'static str>) {
        if self.music != name {
            self.switch(self.music, name, MUSIC_VOL * self.vol[0] * self.vol[1]);
            self.music = name;
        }
    }

    pub fn set_ambience(&mut self, name: Option<&'static str>) {
        if self.ambience != name {
            self.switch(self.ambience, name, AMB_VOL * self.vol[0] * self.vol[3]);
            self.ambience = name;
        }
    }

    pub fn set_loop(&mut self, name: Option<&'static str>) {
        if self.looped != name {
            self.switch(self.looped, name, LOOP_VOL * self.vol[0] * self.vol[2]);
            self.looped = name;
        }
    }
}
