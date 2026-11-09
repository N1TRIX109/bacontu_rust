//! Загрузка данных, сгенерированных tools/convert.py, и отрисовка спрайтов.

use macroquad::prelude::*;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize, Clone, Debug)]
pub struct SpriteMeta {
    pub w: i32,
    pub h: i32,
    pub xo: i32,
    pub yo: i32,
    pub bl: i32,
    pub br: i32,
    pub bt: i32,
    pub bb: i32,
    pub frames: usize,
    pub speed: f32,
    pub speed_type: i32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ObjInfo {
    pub sprite: Option<String>,
    pub mask: Option<String>,
    pub parent: Option<String>,
    #[serde(default)]
    pub image_speed: Option<f32>,
    #[serde(default)]
    pub image_index: Option<f32>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TilesetMeta {
    pub tile_w: i32,
    pub tile_h: i32,
    pub columns: i32,
    pub border_x: i32,
    pub border_y: i32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct InstData {
    pub obj: String,
    pub x: f32,
    pub y: f32,
    pub sx: f32,
    pub sy: f32,
    pub depth: i32,
    pub props: HashMap<String, serde_json::Value>,
    /// text из InstanceCreationCode (реплика Flo)
    #[serde(default)]
    pub text: Option<String>,
    /// `if global.escape.active instance_destroy()` в creation code
    #[serde(default)]
    pub hide_in_escape: bool,
    /// create_gate_layer(...) для obj_levelgate: (спрайт фона, кадр, x, y)
    #[serde(default)]
    pub gate_layers: Option<Vec<(String, i32, f32, f32)>>,
}

impl InstData {
    pub fn prop(&self, key: &str) -> Option<String> {
        self.props
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct TileLayer {
    pub depth: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub tileset: String,
    pub cells: Vec<i64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Background {
    pub sprite: String,
    pub depth: i32,
    pub x: f32,
    pub y: f32,
    pub hspeed: f32,
    pub vspeed: f32,
    pub htiled: bool,
    pub vtiled: bool,
}

/// Эффект-слой GameMaker "Tint" (_filter_tintfilter): цвет умножается на всё, что глубже слоя
#[derive(Deserialize, Clone, Debug)]
pub struct Tint {
    pub rgb: [f32; 3],
    pub depth: i32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RoomData {
    #[serde(default)]
    pub tint: Option<Tint>,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub view: [i32; 2],
    pub instances: Vec<InstData>,
    pub tiles: Vec<TileLayer>,
    pub backgrounds: Vec<Background>,
}

pub struct Assets {
    root: PathBuf,
    pub sprites: HashMap<String, SpriteMeta>,
    pub objects: HashMap<String, ObjInfo>,
    pub tilesets: HashMap<String, TilesetMeta>,
    tex: RefCell<HashMap<(String, usize), Option<Texture2D>>>,
    tileset_tex: RefCell<HashMap<String, Option<Texture2D>>>,
    gate_tex: RefCell<HashMap<String, Option<Texture2D>>>,
    glyphs: RefCell<HashMap<(String, usize), (f32, f32)>>,
    /// множитель цвета для всего, что рисуется в мире (оттенок комнаты)
    tint: Cell<[f32; 3]>,
    /// перекрашенные текстуры: (спрайт, кадр, спрайт палитры, номер палитры * 3)
    pal_cache: RefCell<HashMap<(String, usize, String, i32), Option<Texture2D>>>,
    pal_data: RefCell<HashMap<String, Option<(usize, usize, Vec<[f32; 4]>)>>>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: PathBuf) -> T {
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("не могу прочитать {}: {e} (запускали tools/convert.py?)", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("битый JSON {}: {e}", path.display()))
}

fn load_png(path: &PathBuf) -> Option<Texture2D> {
    let bytes = std::fs::read(path).ok()?;
    let img = Image::from_file_with_format(&bytes, None).ok()?;
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Nearest);
    Some(t)
}

impl Assets {
    pub fn load(root: &str) -> Self {
        let root = PathBuf::from(root);
        Assets {
            sprites: read_json(root.join("sprites.json")),
            objects: read_json(root.join("objects.json")),
            tilesets: read_json(root.join("tilesets.json")),
            root,
            tex: RefCell::new(HashMap::new()),
            tileset_tex: RefCell::new(HashMap::new()),
            gate_tex: RefCell::new(HashMap::new()),
            glyphs: RefCell::new(HashMap::new()),
            tint: Cell::new([1.0; 3]),
            pal_cache: RefCell::new(HashMap::new()),
            pal_data: RefCell::new(HashMap::new()),
        }
    }

    pub fn room(&self, name: &str) -> Option<RoomData> {
        let p = self.root.join("rooms").join(format!("{name}.json"));
        if !p.exists() {
            return None;
        }
        Some(read_json(p))
    }

    /// true, если `obj` сам является `ancestor` или его потомком (цепочка parentObjectId).
    pub fn is_a(&self, obj: &str, ancestor: &str) -> bool {
        let mut cur = Some(obj.to_string());
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.objects.get(&c).and_then(|o| o.parent.clone());
        }
        false
    }

    /// image_speed / image_index из Create_0 объекта (с учётом родителей)
    pub fn obj_anim(&self, obj: &str) -> (Option<f32>, Option<f32>) {
        let (mut speed, mut index) = (None, None);
        let mut cur = Some(obj.to_string());
        while let Some(c) = cur {
            if let Some(o) = self.objects.get(&c) {
                speed = speed.or(o.image_speed);
                index = index.or(o.image_index);
                cur = o.parent.clone();
            } else {
                break;
            }
        }
        (speed, index)
    }

    fn load_image(&self, rel: &str) -> Option<Image> {
        let bytes = std::fs::read(self.root.join(rel)).ok()?;
        Image::from_file_with_format(&bytes, None).ok()
    }

    /// Ворота уровня (obj_levelgate/Draw_0): фон-слои, обрезанные силуэтом (кадр 1 спрайта ворот).
    /// В оригинале силуэт - маска для surface; рисовать его напрямую = белое пятно.
    pub fn gate_texture(&self, gate_sprite: &str, layers: &[(String, i32, f32, f32)]) -> Option<Texture2D> {
        let key = format!(
            "{gate_sprite}|{}",
            layers.iter().map(|l| format!("{}:{}:{}:{}", l.0, l.1, l.2, l.3)).collect::<Vec<_>>().join(";")
        );
        if let Some(t) = self.gate_tex.borrow().get(&key) {
            return t.clone();
        }
        let tex = (|| {
            let gm = self.sprites.get(gate_sprite)?;
            let mask = self.load_image(&format!("sprites/{gate_sprite}/1.png"))?;
            let (w, h) = (mask.width() as u32, mask.height() as u32);
            let mut out = Image::gen_image_color(w as u16, h as u16, Color::new(0.0, 0.0, 0.0, 0.0));
            for (spr, idx, bx, by) in layers {
                let Some(bm) = self.sprites.get(spr) else { continue };
                let Some(img) = self.load_image(&format!("sprites/{spr}/{idx}.png")) else { continue };
                let off_y = bm.yo as f32 + gm.h as f32;
                for py in 0..h {
                    for px in 0..w {
                        let u = ((px as f32 - bx + bm.xo as f32).rem_euclid(bm.w as f32)) as u32;
                        let v = ((py as f32 - (off_y + by) + bm.yo as f32).rem_euclid(bm.h as f32)) as u32;
                        let c = img.get_pixel(u.min(bm.w as u32 - 1), v.min(bm.h as u32 - 1));
                        if c.a <= 0.0 {
                            continue;
                        }
                        let o = out.get_pixel(px, py);
                        let na = c.a + o.a * (1.0 - c.a);
                        let mix = |cc: f32, oc: f32| (cc * c.a + oc * o.a * (1.0 - c.a)) / na;
                        out.set_pixel(px, py, Color::new(mix(c.r, o.r), mix(c.g, o.g), mix(c.b, o.b), na));
                    }
                }
            }
            // маска: видно только внутри силуэта ворот
            for py in 0..h {
                for px in 0..w {
                    let m = mask.get_pixel(px, py).a;
                    let mut c = out.get_pixel(px, py);
                    c.a *= m;
                    out.set_pixel(px, py, c);
                }
            }
            let t = Texture2D::from_image(&out);
            t.set_filter(FilterMode::Nearest);
            Some(t)
        })();
        self.gate_tex.borrow_mut().insert(key, tex.clone());
        tex
    }

    /// Границы закрашенных пикселей кадра по горизонтали: (минимальный x, максимальный x + 1).
    /// Нужны для шрифта из спрайта (font_combo): ширина символа = ширина его пикселей.
    pub fn glyph_bounds(&self, sprite: &str, frame: usize) -> (f32, f32) {
        let key = (sprite.to_string(), frame);
        if let Some(b) = self.glyphs.borrow().get(&key) {
            return *b;
        }
        let mut bounds = (0.0, self.sprites.get(sprite).map(|m| m.w as f32).unwrap_or(32.0));
        if let Some(img) = self.load_image(&format!("sprites/{sprite}/{frame}.png")) {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let (mut lo, mut hi) = (w, 0);
            for y in 0..h {
                for x in 0..w {
                    if img.get_pixel(x, y).a > 0.0 {
                        lo = lo.min(x);
                        hi = hi.max(x + 1);
                    }
                }
            }
            if hi > lo {
                bounds = (lo as f32, hi as f32);
            }
        }
        self.glyphs.borrow_mut().insert(key, bounds);
        bounds
    }

    pub fn sprite_of(&self, obj: &str) -> Option<&str> {
        self.objects.get(obj).and_then(|o| o.sprite.as_deref())
    }

    pub fn frames(&self, sprite: &str) -> usize {
        self.sprites.get(sprite).map(|s| s.frames.max(1)).unwrap_or(1)
    }

    /// Множитель скорости анимации, заданный в самом спрайте.
    pub fn sprite_speed(&self, sprite: &str) -> f32 {
        match self.sprites.get(sprite) {
            Some(m) if m.speed_type == 1 => m.speed,
            Some(m) => m.speed / 60.0,
            None => 1.0,
        }
    }

    pub fn texture(&self, sprite: &str, frame: usize) -> Option<Texture2D> {
        let key = (sprite.to_string(), frame);
        if let Some(t) = self.tex.borrow().get(&key) {
            return t.clone();
        }
        let path = self.root.join("sprites").join(sprite).join(format!("{frame}.png"));
        let t = load_png(&path);
        self.tex.borrow_mut().insert(key, t.clone());
        t
    }

    pub fn tileset_texture(&self, name: &str) -> Option<Texture2D> {
        if let Some(t) = self.tileset_tex.borrow().get(name) {
            return t.clone();
        }
        let t = load_png(&self.root.join("tilesets").join(format!("{name}.png")));
        self.tileset_tex.borrow_mut().insert(name.to_string(), t.clone());
        t
    }

    /// draw_sprite_ext с углом (радианы, вокруг центра спрайта) - для обломков и частиц
    pub fn draw_sprite_rot(&self, sprite: &str, frame: f32, x: f32, y: f32, scale: f32, alpha: f32, angle: f32) {
        let Some(meta) = self.sprites.get(sprite) else { return };
        let n = meta.frames.max(1);
        let f = (frame.floor().max(0.0) as usize).min(n - 1);
        let Some(tex) = self.texture(sprite, f) else { return };
        let (w, h) = (meta.w as f32 * scale, meta.h as f32 * scale);
        draw_texture_ex(
            &tex,
            x - w / 2.0,
            y - h / 2.0,
            self.tint_color(alpha),
            DrawTextureParams { dest_size: Some(vec2(w, h)), rotation: angle, ..Default::default() },
        );
    }

    /// Оттенок мира (эффект-слой "Tint"); [1, 1, 1] - без оттенка
    pub fn set_tint(&self, rgb: [f32; 3]) {
        self.tint.set(rgb);
    }

    pub fn tint_color(&self, alpha: f32) -> Color {
        let t = self.tint.get();
        Color::new(t[0], t[1], t[2], alpha)
    }

    /// Палитра как массив пикселей: (ширина, высота, rgba)
    fn palette_pixels(&self, pal: &str) -> Option<(usize, usize, Vec<[f32; 4]>)> {
        if let Some(p) = self.pal_data.borrow().get(pal) {
            return p.clone();
        }
        let data = (|| {
            let img = self.load_image(&format!("sprites/{pal}/0.png"))?;
            let (w, h) = (img.width(), img.height());
            let mut px = Vec::with_capacity(w * h);
            for y in 0..h {
                for x in 0..w {
                    let c = img.get_pixel(x as u32, y as u32);
                    px.push([c.r, c.g, c.b, c.a]);
                }
            }
            Some((w, h, px))
        })();
        self.pal_data.borrow_mut().insert(pal.to_string(), data.clone());
        data
    }

    /// pal_swap на CPU: цвета спрайта из столбца 0 палитры заменяются цветами столбца `id`
    /// (дробный id - смешивание соседних столбцов). Результат кэшируется.
    fn pal_texture(&self, sprite: &str, frame: usize, pal: &str, id: f32) -> Option<Texture2D> {
        let q = (id * 3.0).round() as i32;
        let key = (sprite.to_string(), frame, pal.to_string(), q);
        if let Some(t) = self.pal_cache.borrow().get(&key) {
            return t.clone();
        }
        let tex = (|| {
            let (pw, ph, px) = self.palette_pixels(pal)?;
            let mut img = self.load_image(&format!("sprites/{sprite}/{frame}.png"))?;
            let idf = q as f32 / 3.0;
            let id0 = (idf.floor() as usize).min(pw - 1);
            let id1 = (id0 + 1).min(pw - 1);
            let f = idf - idf.floor();
            let (w, h) = (img.width() as u32, img.height() as u32);
            for y in 0..h {
                for x in 0..w {
                    let c = img.get_pixel(x, y);
                    if c.a <= 0.0 {
                        continue;
                    }
                    for row in 0..ph {
                        let b = px[row * pw];
                        // допуск как в шейдере (сумма разностей каналов)
                        if (b[0] - c.r).abs() + (b[1] - c.g).abs() + (b[2] - c.b).abs() < 0.04 {
                            let (c0, c1) = (px[row * pw + id0], px[row * pw + id1]);
                            let m = |a: f32, b: f32| a + (b - a) * f;
                            img.set_pixel(x, y, Color::new(m(c0[0], c1[0]), m(c0[1], c1[1]), m(c0[2], c1[2]), c.a));
                            break;
                        }
                    }
                }
            }
            let t = Texture2D::from_image(&img);
            t.set_filter(FilterMode::Nearest);
            Some(t)
        })();
        self.pal_cache.borrow_mut().insert(key, tex.clone());
        tex
    }

    /// draw_sprite_ext с подменой палитры (pal_swap_set)
    pub fn draw_sprite_pal(&self, sprite: &str, frame: f32, x: f32, y: f32, xs: f32, ys: f32, alpha: f32, pal: &str, id: f32) {
        let Some(meta) = self.sprites.get(sprite) else { return };
        let n = meta.frames.max(1);
        let f = (frame.floor().max(0.0) as usize).min(n - 1);
        let Some(tex) = self.pal_texture(sprite, f, pal, id).or_else(|| self.texture(sprite, f)) else { return };
        let a = x + (0 - meta.xo) as f32 * xs;
        let b = x + (meta.w - meta.xo) as f32 * xs;
        let c = y + (0 - meta.yo) as f32 * ys;
        let d = y + (meta.h - meta.yo) as f32 * ys;
        draw_texture_ex(
            &tex,
            a.min(b),
            c.min(d),
            self.tint_color(alpha),
            DrawTextureParams {
                dest_size: Some(vec2((b - a).abs(), (d - c).abs())),
                flip_x: xs < 0.0,
                flip_y: ys < 0.0,
                ..Default::default()
            },
        );
    }

    /// Инвертированный по альфе чёрный силуэт спрайта: чёрное везде, кроме формы спрайта.
    /// Нужен для "дырки" перехода титульной карты (spr_transition_mildred вычитается из чёрного кадра).
    pub fn hole_texture(&self, sprite: &str) -> Option<Texture2D> {
        let key = format!("hole:{sprite}");
        if let Some(t) = self.gate_tex.borrow().get(&key) {
            return t.clone();
        }
        let tex = (|| {
            let mut img = self.load_image(&format!("sprites/{sprite}/0.png"))?;
            let (w, h) = (img.width() as u32, img.height() as u32);
            for y in 0..h {
                for x in 0..w {
                    let a = img.get_pixel(x, y).a;
                    img.set_pixel(x, y, Color::new(0.0, 0.0, 0.0, 1.0 - a));
                }
            }
            let t = Texture2D::from_image(&img);
            t.set_filter(FilterMode::Linear);
            Some(t)
        })();
        self.gate_tex.borrow_mut().insert(key, tex.clone());
        tex
    }

    /// draw_sprite с оттенком (image_blend)
    pub fn draw_sprite_col(&self, sprite: &str, frame: f32, x: f32, y: f32, xs: f32, ys: f32, color: Color) {
        let Some(meta) = self.sprites.get(sprite) else { return };
        let n = meta.frames.max(1);
        let f = (frame.floor().max(0.0) as usize).min(n - 1);
        let Some(tex) = self.texture(sprite, f) else { return };
        let a = x + (0 - meta.xo) as f32 * xs;
        let b = x + (meta.w - meta.xo) as f32 * xs;
        let c = y + (0 - meta.yo) as f32 * ys;
        let d = y + (meta.h - meta.yo) as f32 * ys;
        draw_texture_ex(
            &tex,
            a.min(b),
            c.min(d),
            {
                let t = self.tint.get();
                Color::new(color.r * t[0], color.g * t[1], color.b * t[2], color.a)
            },
            DrawTextureParams {
                dest_size: Some(vec2((b - a).abs(), (d - c).abs())),
                flip_x: xs < 0.0,
                flip_y: ys < 0.0,
                ..Default::default()
            },
        );
    }

    /// Аналог draw_sprite_ext: origin спрайта, отрицательный масштаб = отражение.
    pub fn draw_sprite(&self, sprite: &str, frame: f32, x: f32, y: f32, xs: f32, ys: f32, alpha: f32) {
        let Some(meta) = self.sprites.get(sprite) else { return };
        let n = meta.frames.max(1);
        let f = (frame.floor().max(0.0) as usize).min(n - 1);
        let Some(tex) = self.texture(sprite, f) else { return };
        let a = x + (0 - meta.xo) as f32 * xs;
        let b = x + (meta.w - meta.xo) as f32 * xs;
        let c = y + (0 - meta.yo) as f32 * ys;
        let d = y + (meta.h - meta.yo) as f32 * ys;
        draw_texture_ex(
            &tex,
            a.min(b),
            c.min(d),
            self.tint_color(alpha),
            DrawTextureParams {
                dest_size: Some(vec2((b - a).abs(), (d - c).abs())),
                flip_x: xs < 0.0,
                flip_y: ys < 0.0,
                ..Default::default()
            },
        );
    }

    /// Рисует слой тайлов, пропуская всё, что вне экрана.
    pub fn draw_tile_layer(&self, layer: &TileLayer, view: Rect) {
        let Some(meta) = self.tilesets.get(&layer.tileset) else { return };
        let Some(tex) = self.tileset_texture(&layer.tileset) else { return };
        let (tw, th) = (meta.tile_w, meta.tile_h);
        let c0 = (((view.x - layer.x as f32) / tw as f32).floor() as i32).max(0);
        let c1 = (((view.x + view.w - layer.x as f32) / tw as f32).ceil() as i32).min(layer.w);
        let r0 = (((view.y - layer.y as f32) / th as f32).floor() as i32).max(0);
        let r1 = (((view.y + view.h - layer.y as f32) / th as f32).ceil() as i32).min(layer.h);
        for row in r0..r1 {
            for col in c0..c1 {
                let v = layer.cells[(row * layer.w + col) as usize];
                if v <= 0 {
                    continue; // пусто (в т.ч. -2147483648)
                }
                let idx = (v & 0x7FFFF) as i32;
                if idx == 0 {
                    continue;
                }
                let mirror = v & (1 << 28) != 0;
                let flip = v & (1 << 29) != 0;
                let rotate = v & (1 << 30) != 0;
                let sx = (idx % meta.columns) * (tw + 2 * meta.border_x) + meta.border_x;
                let sy = (idx / meta.columns) * (th + 2 * meta.border_y) + meta.border_y;
                draw_texture_ex(
                    &tex,
                    (layer.x + col * tw) as f32,
                    (layer.y + row * th) as f32,
                    self.tint_color(1.0),
                    DrawTextureParams {
                        source: Some(Rect::new(sx as f32, sy as f32, tw as f32, th as f32)),
                        dest_size: Some(vec2(tw as f32, th as f32)),
                        flip_x: mirror,
                        flip_y: flip,
                        rotation: if rotate { std::f32::consts::FRAC_PI_2 } else { 0.0 },
                        ..Default::default()
                    },
                );
            }
        }
    }
}
