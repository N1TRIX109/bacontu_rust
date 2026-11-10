# Fidelity Hotel — Rust Port

An unofficial port of the lobby, the **Waybackyard** (Monolith Mangrove), the **Rock Bottom** level and the tutorial from the GameMaker fan game *Fidelity Hotel* (`whatsshakinbacon`) to **Rust** using [macroquad](https://macroquad.rs).

The port recreates the original game logic directly from the GameMaker source (player physics, moveset, enemy AI, level objects, HUD, menus, saves). It does not wrap or emulate the GameMaker runtime.

> **Status:** early release. The code was written without access to a Rust compiler in the environment where the port was developed, so expect to fix small compile errors on a first build. Room geometry and spawn points were validated with scripts against the converted data.

## Quick start

Requirements: a recent stable Rust toolchain. On Linux you also need the ALSA headers (`libasound2-dev`).

```bash
git clone [repo URL]
cd fidelity_rs
cargo run --release
```

Run the game from the project root: the `assets/` folder must be next to where you launch it. Save files are created in `saves/`.

### Regenerating the assets

The `assets/` folder is generated from the original GameMaker project and sound archive:

```bash
# rooms, sprites, tilesets -> JSON + PNG
python3 tools/convert.py <path/to/whatsshakinbacon-main> assets

# music and sound effects -> .ogg (requires ffmpeg)
python3 tools/convert_audio.py <path/to/sfxs> assets/audio
```

Requirements for the scripts: Python 3 with `Pillow`, and `ffmpeg` for audio. The event → sound file mapping lives in the table at the top of `tools/convert_audio.py`.

## Controls

| Action | Key |
|---|---|
| Move | Arrow keys / WASD |
| Jump | `Z` / `Space` |
| Run (mach) | `Shift` |
| Grab / throw enemy | `X` |
| Enter door / use ladder / superjump (on mach 3) | `↑` |
| Crouch / ground pound (in the air) / roll (while running) | `↓` |
| Pause | `Esc` |
| Cycle player palette | `P` |
| Restart from last save | `R` |
| Noclip (free flight through walls; `Shift` = faster) | `V` |
| Hitbox overlay and debug info | `F3` |
| Hide textures (hitboxes only) | `F4` |
| Flip the escape effect if it renders upside down | `F5` |

Menus: arrows to navigate, `Z` to confirm, `X` to go back, `T` to delete a save file (main menu).

## What is ported

### Levels
- **Hub:** reception lobby (`hotel_receptionentrance`) and main hall (`hotel_1`) with level gates
- **Tutorial** (`tutorial_1–7`): Flo's hints, enemy spawners, wake/sleep monolith blocks, the tutorial block in the lobby (disappears once the tutorial is finished)
- **Waybackyard** (`wayback_1–13`, `wayback_6c`, `wayback_lap`, secret rooms, treasure room)
- **Rock Bottom** (`city_1–5`, `city_4b`, `city_secret1`)
- Level **title cards** (fade, name card for 3.5 s, iris-wipe transition, level load)

### Player moveset
Walk, jump, crouch, mach 2 / mach 3 (+ crazy run), mach turn, mach slide, hit wall, wall climb, **grab dash**, long jump, **uppercut**, **hauling** (carry, throw, piledriver), **ground pound**, **superjump**, **buzzsaw**, **tumble/roll**, **ladders**, slipping on butter, hurt state. Physics constants and conditions are taken from the original scripts (`scr_collision`, `scr_solid`, `scr_player_*`), including GameMaker's banker's rounding.

### World objects
Solids, slopes, platforms, destructible blocks, metal blocks (break on mach 3, buzzsaw and ground pound after ≥10 frames of falling), barrel portals (`obj_ballofbeer`), doors (target door chosen by the `obj_doorX` letter), horizontal and vertical hallways, level gates with masked backgrounds, exit gates, ladders, collectibles (magnet behaviour, hidden behind blocks), secret hearts.

### Enemies
Slosher/Plosher, Dougknight, Cocoant (darts), Sunny, **Egghead**, **Pancake** (drops slippery butter), enemy spawners. Enemies patrol, turn at ledges, get scared by mach 3, can be stomped, bashed (mach 2 / roll), killed (mach 3, ground pound, buzzsaw, superjump, uppercut), grabbed and thrown. Kills spawn corpses, gibs, stars, effects and hit-stop. Hurting the player costs 100 points and grants 100 invincibility frames.

### Level systems
- **Toppin cages** (+1000 points, followers) and spray-wall **painting** with the paintbuddy and the **bus** exit (Rock Bottom)
- **Pizza time:** destroying the monolith starts the escape timer (3:30 in Waybackyard, 3:00 in the tutorial), escape enemies and collectibles appear, the exit gate ends the run; running out of time sends you back to the hub
- Level results: score, rank (S threshold 19000 for Waybackyard, 30000 for Rock Bottom) and highscore stored per save file

### Interface
- **TV** (`obj_hud`): turn-on animation, expressions (idle, combo ≥10, heat ≥50, escape, mach 3, crazy run, hurt, happy), static transitions, slides away when the player is near the right edge
- **Combo counter:** +1 per kill, 0.15/frame timer extended by collectibles, throws and cages (−25 on damage), meter with lights and arrow, "COMBO" titles every 5, "COMBO LOST" with bonus points
- HUD is hidden in the hub and the tutorial, as in the original
- **Pause menu** (frozen frame with a wave shader, scrolling pattern, floating hearts, secret hearts and treasure indicator, pause music)
- **Main menu** with three VHS save slots, completion percentage and per-slot room palette
- **Options:** master/music/SFX/ambience volume, fullscreen, screen effects

### Saves
Ini-style files modeled on the original:
- `saves/saveDate_1..3.ini` — palette, tutorial progress, secrets, treasure, broken toppin cages, rank and highscore per level
- `saves/saveData.ini` — volumes, fullscreen, screen effects

Saving happens on palette change, toppin cage break, finishing or leaving a level, entering secrets and returning to the main menu.

### Visuals
- Tile layers, scrolling backgrounds and depth sorting
- **Room tint** from GameMaker's "Tint" effect layer (a bluish `#99CAFF` in Waybackyard, darker in `wayback_lap`)
- **Escape shader** (`shd_escape`): wave distortion, red/blue channel split and two additive ghost copies of the frame during pizza time
- **Player palettes** (`spr_playerPal`, default index 1 as in the original). Palette swaps are done on the CPU with a texture cache, so they do not depend on a shader
- Break particles (debris, sparks for metal, dust), enemy death effects, score popups

### Debug tools
`F3` draws hitboxes (solid, destructible and metal blocks, slopes and platforms as lines, player mask, enemies, doors and hallways with target rooms, barrels, ladders, spawner radius) and a status line. `F4` hides textures. `V` toggles noclip.

## Known limitations

- **Not ported:** ski, snowball, skateboard, surfing, gunk, parry and taunt states, diveboard, the FEATS screen, Pizzaface, lap portals, escape pointers, the full rank screen, the full original options menu, the online server menu, and pizza time in Rock Bottom (the original `city_*` rooms contain no monolith).
- The player character is Mildred (buzzsaw on top of a wall climb). Hauling is simplified (no swing-ading spin and some finishers).
- The FMOD sound system is replaced with pre-converted `.ogg` files. **The mapping of files to events is based on file names and durations**, so some sounds and tracks may differ from the original; edit `tools/convert_audio.py` and re-run it to change them.
- Gate backgrounds are static (the original scrolls them).
- Shader and render-target behaviour differs between `macroquad` versions. If the pause screenshot or the escape effect is flipped, press `F5`; if a shader fails to build, the game falls back to rendering without it and prints the reason to the console.

## Project layout

```
tools/convert.py        GameMaker rooms/sprites/tilesets -> JSON + PNG (assets/)
tools/convert_audio.py  sound archive -> .ogg, event -> file table
src/main.rs             60 Hz loop, rooms and transitions, camera, rendering, escape, title cards
src/assets.rs           asset loading, sprite/tile drawing, palette swap, room tint, gate masks
src/world.rs            room runtime: blocks, doors, hallways, portals, pickups, decor, Rock Bottom objects
src/player.rs           player physics and states (ported from GML)
src/player/moves.rs     grab, hauling, ground pound, superjump, buzzsaw, ladders, slip, metal blocks
src/enemy.rs            enemy AI, enemy/player interaction, darts
src/hud.rs              TV and combo interface
src/menu.rs             pause menu, main menu, options
src/save.rs             ini saves, options and ranks
src/audio.rs            sound loading, music, ambience and volume handling
```

## Assets and rights

This is an unofficial, non-commercial fan port. All art, music and sound effects belong to their original authors. The files in `assets/` are converted from the original project; use and redistribute them only within the rights you have for the original game. [Add your license for the Rust source code here, e.g. MIT.]

## Credits

- Original game: *Fidelity Hotel* by [original authors]
- Rust port: phant1k, Claude Code
- Built with [macroquad](https://macroquad.rs), `serde` and `serde_json`
- Bundled font: DejaVu Sans Bold (Bitstream Vera license)
