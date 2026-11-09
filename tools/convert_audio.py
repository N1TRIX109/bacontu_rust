#!/usr/bin/env python3
"""
Отбирает нужные звуки из архива sfxs и конвертирует в .ogg (macroquad не читает mp3).
    python3 tools/convert_audio.py <папка sfxs> assets/audio
Таблица ниже = соответствие "событие порта -> файл". Хотите другой звук - меняйте имя файла и перезапускайте.
"""
import subprocess, sys
from pathlib import Path

SRC, OUT = Path(sys.argv[1]), Path(sys.argv[2])
OUT.mkdir(parents=True, exist_ok=True)

# ключ: (файл, качество vorbis 0-10, моно?[, обрезать до N секунд])
TABLE = {
    # --- музыка и атмосфера ---
    "music_hub":            ("Dining Hall and Lucid Dreams FULL.mp3", 4, False),  # event:/Music/W1/hub (предположение)
    "music_wayback":        ("Through Troubled Waters_2 (3).ogg",     4, False),  # soundtest: wayback
    "music_wayback_secret": ("waybackSecret.ogg",                     4, False),  # mangrovesecret
    "amb_wayback":          ("rain-and-birds-singing-in-the-forest-422415.mp3", 2, False),  # ambience/mangrove
    # --- игрок ---
    "jump":       ("jump.ogg", 4, True),
    "step":       ("sfx_step.wav", 4, True),
    "mach2loop":  ("mach 2 BT.wav", 4, True),
    "mach3loop":  ("mach 3 BT.wav", 4, True),
    "maxspeed":   ("maxspeed BT.wav", 4, True),
    "machstart":  ("mach 1 BT.wav", 4, True),
    "bump":       ("wallbump.mp3", 4, True),
    "mach2bump1": ("mach2bump2.wav", 4, True),
    "mach2bump2": ("mach2bump3.wav", 4, True),
    "mach2bump3": ("mach2bump4.wav", 4, True),
    "mach3bump":  ("shakinbump.ogg", 4, True),
    "machturn":   ("reise turn.ogg", 4, True),
    "machslide":  ("reise skid.ogg", 4, True),
    "noclip_on":  ("switch.wav", 4, True),
    # --- мир ---
    "door":       ("openSlide.ogg", 4, True),
    "breakblock": ("sfx_breakmetal.wav", 4, True, 1.6),
    "killenemy":  ("sfx_killenemy.wav", 4, True),
    "dive":       ("sfx/dive.wav", 4, True),
    "divelanding": ("gp land.ogg", 4, True, 1.4),
    # --- pizza time, враги, топпины ---
    "music_escape": ("RTR LEGACY (2).ogg", 4, False),            # предположение: музыка побега
    "escape_start": ("monolith DIE.wav", 4, True, 4.0),
    "hurt":         ("baconscreamow.ogg", 4, True, 0.9),   # короткий, играет тихо (0.16)
    "punch":        ("punch.mp3", 4, True),
    "toppin":       ("bigescapecollect.ogg", 4, True, 2.2),
    "cagebreak":    ("destroyable.wav", 4, True, 1.5),
    "tick":         ("time ticking.ogg", 4, True),
    "grab":         ("EEnE Grab 1.wav", 4, True, 1.2),
    "superjump":    ("sfx_superjumprelease.wav", 4, True, 2.0),
    "superjumpprep": ("sfx_superjumpprep.wav", 4, True, 2.0),
    "uppercut":     ("uppercut.ogg", 4, True),
    # --- пауза и главное меню ---
    "music_pause":  ("FHPause.ogg", 4, False),
    "music_menu":   ("WelcomeBackAMBIANCE.mp3", 3, False),   # предположение
    "menu_move":    ("MENU (1).wav", 4, True),
    "menu_select":  ("MENU (2).wav", 4, True),
    "menu_back":    ("MENU (3).wav", 4, True),
    "tvstatic":     ("TVStatic.wav", 4, True, 1.2),
    # --- Rock Bottom ---
    "music_city":        ("FHROCKBOTTOMNEWwithVocals.ogg", 3, False),
    "music_city_secret": ("rockbottomSecret.ogg", 3, False),
    "jingle_city":       ("rockbotombro.ogg", 4, True),       # джингл титульной карты (предположение)
    "collect":    ("escapcikkect.ogg", 4, True, 0.7),   # обычный коллект - короткий
    "bigcollect": ("bigcollect.ogg", 4, True),
    "secret":     ("secret enter.wav", 4, True),
}

for key, entry in TABLE.items():
    name, q, mono = entry[:3]
    trim = entry[3] if len(entry) > 3 else None
    src = SRC / name
    if not src.exists():
        print("НЕТ ФАЙЛА:", name); continue
    cmd = ["ffmpeg", "-y", "-loglevel", "error", "-i", str(src), "-vn"]
    if mono: cmd += ["-ac", "1"]
    if trim: cmd += ["-t", str(trim), "-af", f"afade=t=out:st={max(trim - 0.25, 0)}:d=0.25"]
    cmd += ["-c:a", "libvorbis", "-q:a", str(q), str(OUT / f"{key}.ogg")]
    subprocess.run(cmd, check=True)
    print("ok", key, "<-", name)
