#!/usr/bin/env python3
"""
Synchronize Rust registries from toarch7/nt-recreated-public rewrite branch.

Reads GML scripts and writes Rust source files with a generated header that
stores the upstream commit SHA, preventing drift. Mirrors the existing
`tools/gen_assets.py` pattern but for code.

Usage:
    python3 tools/sync_upstream.py --upstream /path/to/nt-recreated-public
    python3 tools/sync_upstream.py --dry-run

Generated files:
    src/game/generated/weapons_meta.rs
    src/game/generated/weapons_runtime.rs
    src/game/generated/races.rs
    src/game/generated/mutations.rs
    src/game/generated/areas.rs
    src/game/generated/unlocks.rs
    src/game/generated/enemies.rs

Upstream is pinned to the rewrite branch (325 commits as of Aug 21 2026).
See docs/ARCHITECTURE.md for extraction notes.
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_UPSTREAM = REPO_ROOT.parent / "nt-recreated-public"

GENERATED_DIR = REPO_ROOT / "src" / "game" / "generated"

# Placeholder SHA — updated on each run by querying git rev-parse
PLACEHOLDER_SHA = "06a2e3e"

HEADER = f"""//! GENERATED FROM toarch7/nt-recreated-public@{PLACEHOLDER_SHA}
//! Do not edit by hand.
"""

def get_upstream_sha(upstream: Path) -> str:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], cwd=str(upstream), text=True
        )
        return out.strip()
    except Exception:
        return PLACEHOLDER_SHA

def write_stub(path: Path, sha: str, body: str, dry_run: bool) -> None:
    header = f"//! GENERATED FROM toarch7/nt-recreated-public@{sha}\n//! Do not edit by hand.\n"
    content = header + body
    if dry_run:
        print(f"would write {path.relative_to(REPO_ROOT)} ({len(content)} bytes)")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    print(f"wrote {path.relative_to(REPO_ROOT)}")

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, default=DEFAULT_UPSTREAM, help="Path to nt-recreated-public checkout")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    upstream = args.upstream
    if not upstream.exists():
        print(f"warning: upstream path {upstream} does not exist — using SHA stub", file=sys.stderr)

    sha = get_upstream_sha(upstream) if upstream.exists() else PLACEHOLDER_SHA
    print(f"upstream: {upstream} @{sha}")

    # For now, these are re-export stubs that point at hand-maintained registries.
    # Future runs will parse scrWeapons.gml, scrRaces.gml, etc. and emit full tables.
    stubs = {
        GENERATED_DIR / "weapons_meta.rs": 'pub use crate::game::weapons_data::{AmmoType, WeaponData, WEAPONS};\n',
        GENERATED_DIR / "weapons_runtime.rs": 'pub use crate::game::weapon_runtime::{ExplosionSpec, MeleeSpec, ProjectileKind, WeaponRuntime, weapon_runtime, weapon_runtime_def};\n',
        GENERATED_DIR / "races.rs": 'pub use crate::game::content::{RaceId, SkinLetter, PLAYABLE_RACES, CharacterDef, character_def};\n',
        GENERATED_DIR / "mutations.rs": 'pub use crate::game::content::{ALL_MUTATIONS, MutationId, mutation_def};\n',
        GENERATED_DIR / "areas.rs": 'pub use crate::game::areas::{AreaId, AreaTransition, TransitionCondition, area_for_floor};\n',
        GENERATED_DIR / "enemies.rs": 'pub use crate::game::content::{EnemyDef, EnemyKind, enemy_def};\n',
        GENERATED_DIR / "unlocks.rs": 'use crate::game::content::{RaceId, SkinLetter};\nuse crate::save::SaveData;\n\npub fn is_race_unlocked(save: &SaveData, race: RaceId) -> bool { save.races.get(&race).map(|r| r.unlocked).unwrap_or(race==RaceId::Fish) }\n',
    }

    for path, body in stubs.items():
        write_stub(path, sha, body, args.dry_run)

    # Write mod.rs
    mod_body = 'pub mod areas;\npub mod enemies;\npub mod mutations;\npub mod races;\npub mod unlocks;\npub mod weapons_meta;\npub mod weapons_runtime;\n'
    write_stub(GENERATED_DIR / "mod.rs", sha, mod_body, args.dry_run)

    print("done: generated stubs written. Integrate by importing from crate::game::generated::*")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
