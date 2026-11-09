#!/usr/bin/env python3
"""
Конвертер ресурсов GameMaker-проекта (whatsshakinbacon) в формат, который читает Rust-порт.

    python3 tools/convert.py <путь к распакованному проекту> <папка assets>

Что делает:
  * rooms/<имя>/<имя>.yy  ->  assets/rooms/<имя>.json  (инстансы, тайлы, фоны)
  * спрайты, на которые есть ссылки  ->  assets/sprites/<имя>/<кадр>.png + assets/sprites.json
  * тайлсеты  ->  assets/tilesets/<имя>.png + assets/tilesets.json
Никакой код GML здесь не исполняется - только данные.
"""
import json, re, shutil, sys
from pathlib import Path

SRC = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve()

# Лобби (хаб) и Waybackyard ("задний двор")
ROOMS = (
    ["hotel_receptionentrance", "hotel_1"]
    + [f"tutorial_{i}" for i in range(1, 8)]
    # Rock Bottom (level "city")
    + ["city_1", "city_2", "city_3", "city_4", "city_4b", "city_5", "city_secret1"]
    + [f"wayback_{i}" for i in range(1, 14)]
    + ["wayback_6c", "wayback_lap", "wayback_secret1", "wayback_secret2",
       "wayback_secret3", "wayback_treasure"]
)

# Невидимые "служебные" объекты - их спрайты не рисуем
INVISIBLE = {
    "obj_solid", "obj_slope", "obj_platform", "obj_hallway", "obj_hallway_vertical",
    "obj_doorA", "obj_doorB", "obj_doorC", "obj_doorD", "obj_doorE", "obj_doorF",
    "obj_doorG", "obj_doorH", "obj_doorS", "obj_ladder", "obj_escape_pointer",
}


def load(p):
    s = Path(p).read_text(encoding="utf-8-sig")
    return json.loads(re.sub(r",(\s*[}\]])", r"\1", s))


def decode_tiles(t):
    """RLE-формат GMS2: n>0 -> n значений подряд, n<0 -> следующее значение |n| раз."""
    data, out, i = t["TileCompressedData"], [], 0
    while i < len(data):
        n = data[i]; i += 1
        if n < 0:
            out += [data[i]] * (-n); i += 1
        else:
            out += data[i:i + n]; i += n
    w, h = t["SerialiseWidth"], t["SerialiseHeight"]
    assert len(out) == w * h, (len(out), w, h)
    return w, h, out


objects_cache, sprites_needed = {}, set()


def obj_info(name):
    if name in objects_cache:
        return objects_cache[name]
    p = SRC / "objects" / name / f"{name}.yy"
    info = {"sprite": None, "mask": None, "parent": None, "image_speed": None, "image_index": None}
    cr = SRC / "objects" / name / "Create_0.gml"
    if cr.exists():
        code = cr.read_text(encoding="utf-8-sig", errors="ignore")
        m = re.search(r"^\s*image_speed\s*=\s*([0-9.]+)", code, re.M)
        if m: info["image_speed"] = float(m.group(1))
        m = re.search(r"^\s*image_index\s*=\s*([0-9.]+)", code, re.M)
        if m: info["image_index"] = float(m.group(1))
    if p.exists():
        d = load(p)
        if d.get("spriteId"):  info["sprite"] = d["spriteId"]["name"]
        if d.get("maskSpriteId"): info["mask"] = d["maskSpriteId"]["name"]
        if d.get("parentObjectId"): info["parent"] = d["parentObjectId"]["name"]
    objects_cache[name] = info
    if info["sprite"]: sprites_needed.add(info["sprite"])
    if info["mask"]: sprites_needed.add(info["mask"])
    if info["parent"]: obj_info(info["parent"])
    return info


def instance_code(room, inst_name):
    """text = "..." у Flo (obj_tutorialbook) и 'if global.escape.active instance_destroy()'."""
    p = SRC / "rooms" / room / f"InstanceCreationCode_{inst_name}.gml"
    out = {}
    if not p.exists():
        return out
    code = p.read_text(encoding="utf-8-sig", errors="ignore")
    m = re.search(r'text\s*=\s*"((?:[^"\\]|\\.)*)"', code)
    if m:
        out["text"] = m.group(1)
    if re.search(r"global\.escape\.active\s*\n?\s*instance_destroy", code):
        out["hide_in_escape"] = True
    layers = re.findall(r"create_gate_layer\(\s*(spr_\w+)\s*,\s*(\d+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)", code)
    if layers:
        out["gate_layers"] = [[a, int(b), float(c), float(d)] for a, b, c, d in layers]
        for a, *_ in layers:
            sprites_needed.add(a)
    return out


def convert_room(name):
    d = load(SRC / "rooms" / name / f"{name}.yy")
    room = {
        "name": name,
        "width": d["roomSettings"]["Width"],
        "height": d["roomSettings"]["Height"],
        "instances": [], "tiles": [], "backgrounds": [],
    }
    views = [v for v in d.get("views", []) if v.get("visible")]
    if views:
        room["view"] = [views[0]["wview"], views[0]["hview"]]
    else:
        room["view"] = [960, 540]
    for layer in d["layers"]:
        kind = layer["resourceType"]
        if kind == "GMRInstanceLayer":
            for i in layer["instances"]:
                oname = i["objectId"]["name"]
                obj_info(oname)
                code = instance_code(name, i["name"]) if i.get("hasCreationCode") else {}
                room["instances"].append({
                    **code,
                    "obj": oname, "x": i["x"], "y": i["y"],
                    "sx": i["scaleX"], "sy": i["scaleY"], "rot": i["rotation"],
                    "depth": layer["depth"],
                    "props": {p["propertyId"]["name"]: p["value"] for p in i["properties"]},
                })
        elif kind == "GMRTileLayer":
            w, h, cells = decode_tiles(layer["tiles"])
            room["tiles"].append({
                "name": layer["name"], "depth": layer["depth"],
                "x": layer["x"], "y": layer["y"], "w": w, "h": h,
                "tileset": layer["tilesetId"]["name"], "cells": cells,
            })
        elif kind == "GMREffectLayer" and layer.get("effectType") == "_filter_tintfilter":
            # эффект-слой "Tint" (шейдер оттенка Waybackyard): g_TintCol = #AARRGGBB
            for prop in layer.get("properties", []):
                if prop.get("name") == "g_TintCol":
                    v = prop["value"].lstrip("#")
                    r, g, b = int(v[2:4], 16), int(v[4:6], 16), int(v[6:8], 16)
                    room["tint"] = {"rgb": [r / 255, g / 255, b / 255], "depth": layer["depth"]}
        elif kind == "GMRBackgroundLayer" and layer.get("spriteId") and layer["visible"]:
            sp = layer["spriteId"]["name"]
            sprites_needed.add(sp)
            room["backgrounds"].append({
                "sprite": sp, "depth": layer["depth"], "x": layer["x"], "y": layer["y"],
                "hspeed": layer["hspeed"], "vspeed": layer["vspeed"],
                "htiled": layer["htiled"], "vtiled": layer["vtiled"],
                "stretch": layer["stretch"],
            })
    return room


def main():
    (OUT / "rooms").mkdir(parents=True, exist_ok=True)
    rooms = {}
    for r in ROOMS:
        rooms[r] = convert_room(r)
        (OUT / "rooms" / f"{r}.json").write_text(json.dumps(rooms[r], separators=(",", ":")))
        print("room", r, len(rooms[r]["instances"]), "instances")

    # Все спрайты игрока (их выбирают состояния) + эффекты/UI, нужные порту
    for p in (SRC / "sprites").iterdir():
        if p.name.startswith(("spr_player_", "spr_cloudeffect", "spr_jumpeffect",
                              "spr_landeffect", "spr_dashcloud", "spr_sparkle",
                              "spr_collect", "spr_bigcollect",
                              # враги, их эффекты, топпины, клетки и побег
                              "spr_cocoant", "spr_doug_", "spr_slosher", "spr_plosher",
                              "spr_sunnyboy", "spr_baddie_gibs", "spr_slapstar",
                              "spr_kungfueffect", "spr_bangeffect", "spr_genericpoofeffect",
                              "spr_hurteffect", "spr_parryeffect", "spr_juiceboxcage",
                              "spr_potable", "spr_escape", "spr_mugmonolith", "spr_stun",
                              # двери, ворота, туториал
                              "spr_uparrow", "spr_gate", "spr_flo", "spr_monolithblock",
                              "spr_enemyspawner", "spr_tutorial", "spr_door", "spr_exitgate",
                              "spr_playerPal", "spr_grabcloud", "spr_ladder",
                              # интерфейс: телевизор, комбо-метр, цифры комбо
                              "spr_tv", "spr_combo", "font_combo",
                              # пауза и главное меню
                              "spr_pause", "spr_mainmenu", "pal_mainmenu", "spr_menu_", "spr_tapestatus",
                              "spr_controls", "spr_quitgame", "spr_deletefile", "spr_treasure_pause",
                              # Rock Bottom: враги, масло, такси, детектив, сердца секретов, переход титульной карты
                              "spr_egghead", "spr_pancake", "spr_butter", "spr_taxi", "spr_detective",
                              "spr_loveportal", "spr_transition_mildred", "spr_paintbuddy", "spr_walk", "spr_idle")):
            sprites_needed.add(p.name)

    # Тайлсеты -> их спрайт-атласы
    tilesets = {}
    used = {t["tileset"] for r in rooms.values() for t in r["tiles"]}
    (OUT / "tilesets").mkdir(exist_ok=True)
    for ts in used:
        d = load(SRC / "tilesets" / ts / f"{ts}.yy")
        shutil.copy(SRC / "tilesets" / ts / "output_tileset.png", OUT / "tilesets" / f"{ts}.png")
        tilesets[ts] = {
            "tile_w": d["tileWidth"], "tile_h": d["tileHeight"],
            "columns": d["out_columns"],
            "border_x": d["out_tilehborder"], "border_y": d["out_tilevborder"],
        }
    (OUT / "tilesets.json").write_text(json.dumps(tilesets, indent=1))

    # Спрайты
    sprites = {}
    for sname in sorted(sprites_needed):
        yy = SRC / "sprites" / sname / f"{sname}.yy"
        if not yy.exists():
            continue
        d = load(yy)
        seq = d["sequence"]
        dst = OUT / "sprites" / sname
        dst.mkdir(parents=True, exist_ok=True)
        frames = []
        for i, f in enumerate(d["frames"]):
            src_png = SRC / "sprites" / sname / f"{f['name']}.png"
            if src_png.exists():
                shutil.copy(src_png, dst / f"{i}.png")
                frames.append(i)
        speed = seq.get("playbackSpeed", 1.0)
        # playbackSpeedType 0 = кадров/сек, 1 = кадров за игровой кадр
        sprites[sname] = {
            "w": d["width"], "h": d["height"],
            "xo": seq["xorigin"], "yo": seq["yorigin"],
            "bl": d["bbox_left"], "br": d["bbox_right"],
            "bt": d["bbox_top"], "bb": d["bbox_bottom"],
            "frames": len(frames), "speed": speed, "speed_type": seq.get("playbackSpeedType", 1),
        }
    (OUT / "sprites.json").write_text(json.dumps(sprites, separators=(",", ":")))
    (OUT / "objects.json").write_text(json.dumps(objects_cache, indent=1))
    (OUT / "rooms.json").write_text(json.dumps(ROOMS))
    print(len(sprites), "спрайтов,", len(tilesets), "тайлсетов")


main()
