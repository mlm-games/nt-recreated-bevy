# Nuclear Throne (Dogfooding for bevy-repose)

**No assets from the original game are included.** All visuals are placeholder
colored sprites and all sounds are procedurally generated WAVs if `assets/og`
is empty. Original `.ogg` (Vorbis) are used directly without conversion
(`vorbis` feature) via `tools/gen_assets.py` if you have the game locally.

## Quick Start

```bash
cargo run
```

Dev build with hot-reload and FPS overlay:

```bash
cargo run --features dev
```

placeholder SFX (pure Python stdlib, no deps, fallback when `assets/og` is empty):

```bash
python3 tools/gen_audio.py
```

import original assets locally (keeps `.ogg` as `.ogg`, never committed):

```bash
# copies .ogg (Vorbis) + texture atlases
python3 tools/gen_assets.py
python3 tools/gen_assets.py /path/to/NuclearThrone/game/assets
NT_ASSETS=/path/to/game/assets python3 tools/gen_assets.py --dry-run
```

## Controls

| Input | Action |
|-------|--------|
| WASD / Arrows | Move |
| Mouse | Aim |
| LMB / Space | Shoot / swing melee |
| 1 / 2 | Switch weapon |
| E / Left Shift | Active ability |
| Esc | Pause |
| 1 / 2 / 3 | Pick mutation |

## Structure

The game-feel ecosystem (audio, transitions, juice, VFX, save, i18n, pooling)
lives in the **[game-utils](https://github.com/mlm-games/game-utils)** workspace.
This repo holds only the app layer:

```
src/
├── main.rs              # Entry point
├── app.rs               # AppPlugin, states, UI action bridge, HUD shared state
├── save.rs              # SaveData type (high score, best floor, runs, kills)
├── screens/             # Splash, loading, title
├── menus/               # Title (character select), pause, settings, credits, HUD
├── game/                # The Nuclear Throne-style game
│   ├── content.rs       # Characters, weapons, enemies, mutations (data)
│   ├── components.rs    # Resources + components + constants
│   ├── world.rs         # Arena generation, props, collision helpers
│   ├── player.rs        # Movement, aim, abilities, firing (ranged + melee)
│   ├── enemies.rs       # Per-floor spawns, boss spawning, AI
│   ├── combat.rs        # Projectiles, explosions, contact damage, deaths, drops
│   ├── pickups.rs       # Rads, medkits, ammo, weapons, chests, toast
│   ├── progression.rs   # Level-ups, mutations, portals, save flushing
│   ├── audio.rs         # GameAudio handle set (generated WAVs)
│   └── hud.rs           # Pushes live state into the Repose HUD
├── theme/               # Theme resource
├── dev_tools.rs         # FPS overlay, state logging (dev feature)
└── asset_tracking.rs    # Preload tracking
```

```
tools/
├── gen_audio.py         # Generates placeholder SFX (WAV) into assets/audio/
└── gen_assets.py        # Imports original .ogg/.png locally (gitignored, no conversion)
```

## Legal

This is an unofficial, non-commercial fan recreation for learning purposes.
"Nuclear Throne" and its characters are trademarks of Vlambeer. No copyrighted
assets from the original game are included in this repository. It is just meant to showcase that the UI is capable. 
Do also have a ported version (from godot) of floppy-warriors [here](https://github.com/mlm-games/floppy-warriors).

## License

GPL-3.0
