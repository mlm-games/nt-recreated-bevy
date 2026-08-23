#!/usr/bin/env python3
"""Import original Nuclear Throne assets (local-only, never committed).

Copies ``.ogg`` (Vorbis) and texture atlases **without conversion** -
Bevy is built with the ``vorbis`` feature so ``.ogg`` is loaded directly.
Placeholders (generated ``.wav``) remain as fallback when ``assets/og``
is empty, as noted in the README.

Usage:
    python3 tools/gen_assets.py
    python3 tools/gen_assets.py /path/to/game/assets
    python3 tools/gen_assets.py --source /path/with\\ spaces/game/assets --dry-run
    NT_ASSETS=/path/to/game/assets python3 tools/gen_assets.py

Source resolution (first existing wins):
  1. CLI positional arg / --source
  2. $NT_ASSETS env var
  3. $NT_ASSETS_PATH env var (compat)
  4. Default Downloads path used by the author
  5. ./game/assets  (relative to repo root)

Destination:
  Always ``<repo>/assets``.  Original files are copied to:
    - ``assets/audio/*.ogg``  (all .ogg from source root)
    - ``assets/images/*.png`` (all .png from source/tex/)
  A mirror is also kept under ``assets/og/`` (fully gitignored) so the
  fallback check ``assets/og is empty`` keeps working.  No transcoding
  is performed.

Idempotent: re-running overwrites only changed files (size/mtime check).
Pass ``--clean`` to remove previously imported originals.

Pure stdlib, no deps.
"""

from __future__ import annotations

import argparse
import os
import shutil
import struct
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SRC = Path("/home/ymsr/Downloads/nuclear_throne/game-src/game/assets")

DEST_AUDIO = REPO_ROOT / "assets" / "audio"
DEST_IMAGES = REPO_ROOT / "assets" / "images"
DEST_OG = REPO_ROOT / "assets" / "og"


def resolve_source(cli_source: str | None) -> Path | None:
    candidates: list[Path] = []
    if cli_source:
        candidates.append(Path(cli_source))
    for env in ("NT_ASSETS", "NT_ASSETS_PATH"):
        v = os.environ.get(env)
        if v:
            candidates.append(Path(v))
    candidates.append(DEFAULT_SRC)
    candidates.append(REPO_ROOT / "game" / "assets")
    # Also try sibling of repo (common when repo is next to extracted game)
    candidates.append(REPO_ROOT.parent / "game" / "assets")
    for p in candidates:
        if p.exists() and p.is_dir():
            return p
    return None


def should_copy(src: Path, dst: Path) -> bool:
    if not dst.exists():
        return True
    # quick check: size or mtime differs
    try:
        s = src.stat()
        d = dst.stat()
        if s.st_size != d.st_size:
            return True
        # allow 1s tolerance for filesystem rounding
        if abs(s.st_mtime - d.st_mtime) > 1.0:
            return True
        return False
    except OSError:
        return True


def copy_preserve(src: Path, dst: Path, dry_run: bool) -> bool:
    if not should_copy(src, dst):
        return False
    if dry_run:
        return True
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    return True


WAD_CANDIDATES = ["data.win", "game.unx", "game.ios", "game.droid"]


def locate_wad(src: Path) -> Path | None:
    for name in WAD_CANDIDATES:
        p = src / name
        if p.exists():
            return p
        # also check parent (when src is .../assets, WAD is alongside assets/)
        p2 = src.parent / name
        if p2.exists():
            return p2
        p3 = src.parent.parent / name
        if p3.exists():
            return p3
    # also check if src itself is a WAD file
    if src.is_file() and src.suffix in (".win", ".unx", ".ios", ".droid"):
        return src
    return None


def parse_wad_chunks(data: bytes) -> dict[str, int]:
    if data[0:4] != b"FORM":
        raise ValueError("Invalid WAD: missing FORM header")
    chunks: dict[str, int] = {}
    off = 8
    n = len(data)
    while off + 8 <= n:
        name = data[off : off + 4].decode("ascii", errors="replace")
        if len(name) != 4 or not name.isprintable():
            break
        size = struct.unpack_from("<I", data, off + 4)[0]
        chunks[name] = off
        off += 8 + size
        if off > n:
            break
    return chunks


def read_strg(data: bytes, pos: int) -> str:
    end = data.find(b"\x00", pos)
    if end == -1:
        return ""
    return data[pos:end].decode("utf-8", errors="replace")


def extract_wad_sounds(wad_path: Path, dest_audio: Path, dest_og_audio: Path, dry_run: bool) -> tuple[int, int]:
    try:
        data = wad_path.read_bytes()
    except OSError as e:
        print(f"warning: cannot read WAD {wad_path}: {e}", file=sys.stderr)
        return 0, 0
    try:
        chunks = parse_wad_chunks(data)
    except Exception as e:
        print(f"warning: WAD parse failed: {e}", file=sys.stderr)
        return 0, 0
    if "SOND" not in chunks or "AUDO" not in chunks or "STRG" not in chunks:
        return 0, 0

    def u32(off: int) -> int:
        return struct.unpack_from("<I", data, off)[0]

    sond_off = chunks["SOND"]
    audo_off = chunks["AUDO"]
    try:
        sond_cnt = u32(sond_off + 8)
        audo_cnt = u32(audo_off + 8)
    except struct.error:
        return 0, 0

    # Build AUDO buffers
    audo_buffers: list[tuple[int, bytes, bool]] = []
    for i in range(audo_cnt):
        try:
            pos = u32(audo_off + 12 + i * 4)
            blen = u32(pos)
            is_riff = data[pos + 4 : pos + 8] == b"RIFF"
            buf = data[pos + 4 : pos + 4 + blen]
            # Some buffers may be truncated if blen includes header? JS uses pos+4 as start and blen as length
            # Use slice as in JS: data[pos+4 : pos+4+blen] where blen is at pos
            # But JS does: data[pos : pos+blen] after pos+=4, which is same as pos+4
            # We'll use that.
            audo_buffers.append((blen, buf, is_riff))
        except Exception:
            continue

    copied = 0
    skipped = 0
    audo_idx = 0
    for i in range(sond_cnt):
        try:
            pos = u32(sond_off + 12 + i * 4)
            name_off = u32(pos)
            name = read_strg(data, name_off)
            if not name:
                continue
            flags = u32(pos + 4)
            is_external = flags == 100  # AUDIO_FLAG_IS_REGULAR
            if is_external:
                continue  # already handled via external .ogg copy
            if audo_idx >= len(audo_buffers):
                break
            blen, buf, is_riff = audo_buffers[audo_idx]
            audo_idx += 1
            ext = ".wav" if is_riff else ".ogg"
            # Keep original extension, no conversion
            dst = dest_audio / (name + ext)
            dst_og = dest_og_audio / (name + ext)
            # Skip if file exists and same size
            if dst.exists() and dst.stat().st_size == len(buf):
                skipped += 1
                continue
            if dry_run:
                copied += 1
                if copied <= 5:
                    print(f"would extract audio/{name+ext} ({len(buf)//1024} KB, {'WAV' if is_riff else 'OGG'})")
                elif copied == 6:
                    print("  ...")
                continue
            dest_audio.mkdir(parents=True, exist_ok=True)
            dest_og_audio.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(buf)
            # also mirror
            try:
                dst_og.write_bytes(buf)
            except OSError:
                pass
            copied += 1
            if copied <= 5:
                print(f"extracted audio/{name+ext} ({len(buf)//1024} KB)")
        except Exception as e:
            # per-entry failure shouldn't abort whole extraction
            print(f"warning: sound {i} failed: {e}", file=sys.stderr)
            continue
    if copied > 5:
        print(f"  ... and {copied-5} more embedded sounds")
    return copied, skipped


def extract_wad_sprites(wad_path: Path, src_tex_dir: Path | None, dest_sprites: Path, dest_og_sprites: Path, dry_run: bool) -> tuple[int, int]:
    # Extract individual sprite first-frames for auto-wire (no hard dep).
    # Keeps .png directly, no conversion. Uses Pillow to crop from atlas.
    try:
        from PIL import Image  # type: ignore
    except ImportError:
        print("note: Pillow not installed, skipping per-sprite extraction (atlases already copied)")
        return 0, 0
    try:
        data = wad_path.read_bytes()
    except OSError:
        return 0, 0
    try:
        chunks = parse_wad_chunks(data)
        if "SPRT" not in chunks or "TXTR" not in chunks or "TGIN" not in chunks:
            return 0, 0

        # Texture group info (TGIN)
        tgin_off = chunks["TGIN"]
        tgin_cnt = struct.unpack_from("<I", data, tgin_off + 12)[0]
        tex_groups: list[tuple[str, str | None, str]] = []
        for i in range(tgin_cnt):
            pos = struct.unpack_from("<I", data, tgin_off + 16 + i * 4)[0]
            name = read_strg(data, struct.unpack_from("<I", data, pos)[0])
            dir_name = read_strg(data, struct.unpack_from("<I", data, pos + 4)[0])
            ext = read_strg(data, struct.unpack_from("<I", data, pos + 8)[0])
            if not dir_name or dir_name == "DynTex":
                dir_name = None
            tex_groups.append((name, dir_name, ext))

        # TXTR - texture page sizes and external check
        txtr_off = chunks["TXTR"]
        txtr_cnt = struct.unpack_from("<I", data, txtr_off + 8)[0]
        tex_pages: list[tuple[int, int, bool, str | None]] = []  # w,h,is_external, path
        for i in range(txtr_cnt):
            pos = struct.unpack_from("<I", data, txtr_off + 12 + i * 4)[0]
            w = struct.unpack_from("<I", data, pos + 12)[0]
            h = struct.unpack_from("<I", data, pos + 16)[0]
            is_ext = struct.unpack_from("<I", data, pos + 24)[0] == 0
            # need group info to get file path
            if i < len(tex_groups):
                gname, gdir, gext = tex_groups[i]
                if gdir and gext == ".png" and src_tex_dir:
                    # find actual file: tex/<gname>_<idx>.png where idx is indexInGroup
                    # indexInGroup is at pos+20
                    idx_in_group = struct.unpack_from("<I", data, pos + 20)[0]
                    # Try tex dir
                    cand = src_tex_dir / f"{gname}_{idx_in_group}{gext}"
                    if cand.exists():
                        tex_pages.append((w, h, True, str(cand)))
                        continue
                    # fallback to tex dir without idx
                    cand2 = src_tex_dir / f"{gname}{gext}"
                    if cand2.exists():
                        tex_pages.append((w, h, True, str(cand2)))
                        continue
            tex_pages.append((w, h, is_ext, None))

        # 100% sprites when --all-sprites, else curated auto-wire set
        wanted_all = "--all-sprites" in sys.argv or os.environ.get("NT_ALL_SPRITES") == "1"
        if wanted_all:
            # Extract all 2113 sprites
            spr_off_tmp = chunks["SPRT"]
            spr_cnt_tmp = struct.unpack_from("<I", data, spr_off_tmp + 8)[0]
            wanted = set()
            for i in range(spr_cnt_tmp):
                pos = struct.unpack_from("<I", data, spr_off_tmp + 12 + i * 4)[0]
                n = read_strg(data, struct.unpack_from("<I", data, pos)[0])
                if n.startswith("spr"):
                    wanted.add(n)
        else:
            wanted = {
                # mutants
                "sprMutant1Idle", "sprMutant2Idle", "sprMutant3Idle", "sprMutant4Idle",
                "sprMutant1Walk", "sprMutant2Walk", "sprMutant3Walk", "sprMutant4Walk",
                # enemies
                "sprMaggotIdle", "sprBanditIdle", "sprScorpionIdle", "sprGoldScorpionIdle",
                "sprAssassinIdle", "sprJungleAssassinIdle",
                "sprFreak1Idle", "sprExploFreakIdle",
                "sprBanditBossIdle", "sprThroneIdle",
                # floors / walls (all areas you support)
                "sprFloor0", "sprFloor1", "sprFloor100", "sprFloor102",
                "sprWall0Out", "sprWall0Top", "sprWall0Bot",
                "sprWall100Out", "sprWall102Out",
                "sprWall100Top", "sprWall100Bot",
                # props / pickups / portal
                "sprCrate", "sprBarrel", "sprCactus", "sprPortal",
                "sprRad", "sprHP", "sprChest",
                "sprBulletPickup", "sprShellPickup", "sprBoltPickup", "sprExploPickup",
                "sprRevolver", "sprShotgun", "sprMachinegun", "sprCrossbow",
            }

        # Sprites exported as full horizontal animation strips (all frames).
        anim_manifest: dict[str, dict] = {}
        ANIM_SPRITES = {
            "sprMutant1Idle": 10.0, "sprMutant1Walk": 12.0,
            "sprMutant2Idle": 10.0, "sprMutant2Walk": 12.0,
            "sprMutant3Idle": 10.0, "sprMutant3Walk": 12.0,
            "sprMutant4Idle": 10.0, "sprMutant4Walk": 12.0,
            "sprBanditIdle": 8.0, "sprBanditWalk": 10.0,
            "sprJungleAssassinIdle": 8.0, "sprJungleAssassinWalk": 10.0,
            "sprMaggotIdle": 6.0,
            "sprScorpionIdle": 10.0,
            "sprFreak1Idle": 8.0,
            "sprExploFreakIdle": 8.0,
            "sprBanditBossIdle": 8.0, "sprBanditBossWalk": 10.0,
            "sprPortal": 10.0,
        }

        spr_off = chunks["SPRT"]
        spr_cnt = struct.unpack_from("<I", data, spr_off + 8)[0]

        def find_sprite_pos(name: str) -> int | None:
            for i in range(spr_cnt):
                pos = struct.unpack_from("<I", data, spr_off + 12 + i * 4)[0]
                n = read_strg(data, struct.unpack_from("<I", data, pos)[0])
                if n == name:
                    return pos
            return None

        copied = 0
        # Limit to 50 per run unless --all-sprites to avoid 2113-at-once overhead
        to_extract = sorted(wanted)
        if not wanted_all:
            to_extract = to_extract[:50]
        for name in to_extract:
            pos = find_sprite_pos(name)
            if pos is None:
                # try alternative names
                alts = {
                    "sprScorpionIdle": ["sprScorpionIdle", "sprGoldScorpionIdle"],
                    "sprAssassinIdle": ["sprAssassinIdle", "sprJungleAssassinIdle"],
                    "sprThroneIdle": ["sprThroneIdle", "sprThroneStatue"],
                }
                for alt in alts.get(name, []):
                    pos = find_sprite_pos(alt)
                    if pos is not None:
                        name = alt
                        break
                if pos is None:
                    continue
            # Try to parse sprite to get first frame's tPageItem
            # Heuristic: after header, find imageNumber
            try:
                # Read width/height at pos+4, pos+8
                w = struct.unpack_from("<I", data, pos + 4)[0]
                h = struct.unpack_from("<I", data, pos + 8)[0]
                if w == 0 or h == 0 or w > 512 or h > 512:
                    continue
                # Find imageNumber by scanning for small int followed by valid offsets
                img_num = None
                img_off = None
                # Scan from pos+40 to pos+120
                for scan in range(pos + 40, min(pos + 200, len(data) - 4)):
                    v = struct.unpack_from("<I", data, scan)[0]
                    if 1 <= v <= 32:
                        # Check if next v*4 bytes are plausible offsets
                        ok = True
                        for k in range(v):
                            if scan + 4 + k * 4 + 4 > len(data):
                                ok = False
                                break
                            off2 = struct.unpack_from("<I", data, scan + 4 + k * 4)[0]
                            if off2 == 0 or off2 >= len(data) - 22:
                                ok = False
                                break
                            # Check tPageItem has plausible sizes
                            sw = struct.unpack_from("<H", data, off2 + 4)[0]
                            sh = struct.unpack_from("<H", data, off2 + 6)[0]
                            if sw == 0 or sh == 0 or sw > 2048 or sh > 2048:
                                ok = False
                                break
                        if ok:
                            img_num = v
                            img_off = scan
                            break
                if img_num is None or img_off is None:
                    continue
                # First frame's tPageItem offset
                tp_off = struct.unpack_from("<I", data, img_off + 4)[0]
                sx = struct.unpack_from("<H", data, tp_off)[0]
                sy = struct.unpack_from("<H", data, tp_off + 2)[0]
                sw = struct.unpack_from("<H", data, tp_off + 4)[0]
                sh = struct.unpack_from("<H", data, tp_off + 6)[0]
                tx = struct.unpack_from("<H", data, tp_off + 8)[0]
                ty = struct.unpack_from("<H", data, tp_off + 10)[0]
                # tw, th = target size, not needed for crop
                tpid = struct.unpack_from("<h", data, tp_off + 20)[0]
                if tpid < 0 or tpid >= len(tex_pages):
                    continue
                _, _, is_ext, tex_path = tex_pages[tpid]
                if not tex_path or not Path(tex_path).exists():
                    # fallback to atlas in dest
                    # Try to find atlas in src_tex_dir
                    if src_tex_dir:
                        # Find any atlas that matches size
                        for cand in src_tex_dir.glob("*.png"):
                            try:
                                with Image.open(cand) as im:
                                    if im.width == 2048 and im.height == 2048:
                                        tex_path = str(cand)
                                        break
                            except Exception:
                                continue
                    if not tex_path:
                        continue
                dst = dest_sprites / f"{name}.png"
                dst_og = dest_og_sprites / f"{name}.png"
                if dst.exists() and not dry_run:
                    continue
                if dry_run:
                    copied += 1
                    continue
                dest_sprites.mkdir(parents=True, exist_ok=True)
                dest_og_sprites.mkdir(parents=True, exist_ok=True)
                with Image.open(tex_path) as atlas:
                    is_strip = img_num > 1
                    if is_strip:
                        # Horizontal strip: every frame's own tPageItem.
                        out = Image.new("RGBA", (w * img_num, h), (0, 0, 0, 0))

                        for k in range(img_num):
                            tp_k = struct.unpack_from("<I", data, img_off + 4 + k * 4)[0]
                            kx = struct.unpack_from("<H", data, tp_k)[0]
                            ky = struct.unpack_from("<H", data, tp_k + 2)[0]
                            kw = struct.unpack_from("<H", data, tp_k + 4)[0]
                            kh = struct.unpack_from("<H", data, tp_k + 6)[0]
                            ktx = struct.unpack_from("<H", data, tp_k + 8)[0]
                            kty = struct.unpack_from("<H", data, tp_k + 10)[0]

                            crop = atlas.crop((kx, ky, kx + kw, ky + kh))
                            out.paste(crop, (k * w + ktx, kty))

                        # Standard GameMaker sprite fields. Keep a guarded fallback for
                        # unusual WAD versions.
                        try:
                            xorigin = struct.unpack_from("<i", data, pos + 48)[0]
                            yorigin = struct.unpack_from("<i", data, pos + 52)[0]

                            if abs(xorigin) > w * 8 or abs(yorigin) > h * 8:
                                raise ValueError("implausible sprite origin")
                        except (struct.error, ValueError):
                            xorigin = w // 2
                            yorigin = h // 2

                        anim_manifest[name] = {
                            "frames": img_num,
                            "w": w,
                            "h": h,
                            "fps": ANIM_SPRITES.get(name, 0.0),
                            "xorigin": xorigin,
                            "yorigin": yorigin,
                        }
                    else:
                        out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
                        crop = atlas.crop((sx, sy, sx + sw, sy + sh))
                        out.paste(crop, (tx, ty))

                        try:
                            xorigin = struct.unpack_from("<i", data, pos + 48)[0]
                            yorigin = struct.unpack_from("<i", data, pos + 52)[0]

                            if abs(xorigin) > w * 8 or abs(yorigin) > h * 8:
                                raise ValueError("implausible sprite origin")
                        except (struct.error, ValueError):
                            xorigin = w // 2
                            yorigin = h // 2

                        anim_manifest[name] = {
                            "frames": 1,
                            "w": w,
                            "h": h,
                            "fps": 0.0,
                            "xorigin": xorigin,
                            "yorigin": yorigin,
                        }
                    out.save(dst)
                    out.save(dst_og)
                copied += 1
                if copied <= 3:
                    kind = "frame strip" if img_num > 1 else "sprite"
                    print(f"extracted sprites/{name}.png ({w}x{h}, {kind})")
            except Exception as e:
                print(f"warning: sprite {name} failed: {e}", file=sys.stderr)
                continue
        if copied:
            print(f"sprites: extracted {copied} sprite(s) for auto-wire")
        if anim_manifest and not dry_run:
            import json
            manifest_path = dest_sprites / "anims.json"
            merged = {}
            if manifest_path.exists():
                try:
                    merged = json.loads(manifest_path.read_text())
                except Exception:
                    merged = {}
            merged.update(anim_manifest)
            manifest_path.write_text(json.dumps(merged, indent=1))
            print(f"anims: {len(merged)} entries in {manifest_path.name}")
        return copied, 0
    except Exception as e:
        print(f"warning: sprite extraction failed: {e}", file=sys.stderr)
        return 0, 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "source",
        nargs="?",
        help="Path to original game/assets folder (contains .ogg + tex/*.png)",
    )
    parser.add_argument(
        "--source",
        dest="source_opt",
        help="Same as positional source (takes precedence)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be copied without writing",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Remove previously imported original assets (assets/audio/*.ogg, assets/images/*.png from tex, assets/og/)",
    )
    parser.add_argument(
        "--no-mirror",
        action="store_true",
        help="Do not also mirror into assets/og/",
    )
    args = parser.parse_args()

    cli_src = args.source_opt or args.source

    if args.clean:
        removed = 0
        for pattern in [
            DEST_AUDIO.glob("*.ogg"),
            DEST_IMAGES.glob("*.png"),
            DEST_IMAGES.glob("*.PNG"),
        ]:
            for p in pattern:
                # Only remove files that look like originals (from tex or ogg)
                # - keep .gitkeep and .wav placeholders
                if args.dry_run:
                    print(f"would remove {p.relative_to(REPO_ROOT)}")
                else:
                    try:
                        p.unlink()
                        print(f"removed {p.relative_to(REPO_ROOT)}")
                    except FileNotFoundError:
                        pass
                removed += 1
        if DEST_OG.exists():
            if args.dry_run:
                print(f"would remove tree {DEST_OG.relative_to(REPO_ROOT)}/")
            else:
                shutil.rmtree(DEST_OG)
                print(f"removed tree {DEST_OG.relative_to(REPO_ROOT)}/")
                removed += 1
        if removed == 0:
            print("nothing to clean")
        return 0

    src = resolve_source(cli_src)
    if src is None:
        print("error: could not locate original game/assets folder.", file=sys.stderr)
        print("Hint: pass it explicitly:", file=sys.stderr)
        print('  python3 tools/gen_assets.py "/path/to/game/assets"', file=sys.stderr)
        print("  or set NT_ASSETS env var", file=sys.stderr)
        if cli_src:
            print(f"tried: {cli_src}", file=sys.stderr)
        print(f"tried default: {DEFAULT_SRC}", file=sys.stderr)
        return 1

    print(f"source: {src}")
    print(f"dest:   {REPO_ROOT / 'assets'}")
    if args.dry_run:
        print("(dry-run)")

    oggs = sorted(src.glob("*.ogg"))
    # tex folder may be src/tex or src/assets/tex depending on layout
    tex_candidates = [src / "tex", src / "assets" / "tex"]
    tex_dir = next((p for p in tex_candidates if p.is_dir()), None)
    # also handle case where src already is .../assets, then tex is src/tex
    if tex_dir is None and (src / "tex").is_dir():
        tex_dir = src / "tex"

    pngs: list[Path] = []
    if tex_dir and tex_dir.is_dir():
        pngs = sorted(tex_dir.glob("*.png")) + sorted(tex_dir.glob("*.PNG"))
    else:
        # fallback: any png directly under src
        pngs = sorted(src.glob("*.png"))

    if not oggs and not pngs:
        print(f"warning: no .ogg or .png found under {src}", file=sys.stderr)
        return 1

    copied = 0
    skipped = 0

    DEST_AUDIO.mkdir(parents=True, exist_ok=True)
    DEST_IMAGES.mkdir(parents=True, exist_ok=True)

    # Try WAD-embedded sounds (keeps .ogg/.wav as-is, no conversion)
    wad = locate_wad(src)
    if wad:
        print(f"WAD found: {wad.name} ({wad.stat().st_size // (1024*1024)} MB)")
        dest_og_audio = DEST_OG / "audio"
        extra_copied, extra_skipped = extract_wad_sounds(wad, DEST_AUDIO, dest_og_audio, args.dry_run)
        if extra_copied or extra_skipped:
            print(f"WAD sounds: {extra_copied} extracted, {extra_skipped} up-to-date")
            copied += extra_copied
            skipped += extra_skipped
        # Sprite extraction stub (atlases already copied)
        if src:
            tex_dir_for_wad = src / "tex" if (src / "tex").is_dir() else None
            s_copied, s_skipped = extract_wad_sprites(wad, tex_dir_for_wad, DEST_IMAGES, DEST_OG / "images", args.dry_run)
            if s_copied:
                copied += s_copied

    for ogg in oggs:
        dst = DEST_AUDIO / ogg.name
        # keep .ogg extension, do NOT convert
        did = copy_preserve(ogg, dst, args.dry_run)
        if did:
            copied += 1
            print(f"{'would copy' if args.dry_run else 'copied'} audio/{ogg.name} ({ogg.stat().st_size // 1024} KB)")
        else:
            skipped += 1
        if not args.no_mirror:
            mirror = DEST_OG / "audio" / ogg.name
            if copy_preserve(ogg, mirror, args.dry_run):
                if args.dry_run:
                    print(f"  -> would mirror og/audio/{ogg.name}")
                # don't double count
                pass

    for png in pngs:
        dst = DEST_IMAGES / png.name
        did = copy_preserve(png, dst, args.dry_run)
        if did:
            copied += 1
            print(f"{'would copy' if args.dry_run else 'copied'} images/{png.name} ({png.stat().st_size // 1024} KB)")
        else:
            skipped += 1
        if not args.no_mirror:
            mirror = DEST_OG / "images" / png.name
            copy_preserve(png, mirror, args.dry_run)

    # Also mirror icon if present (useful for window icon)
    icon = src / "icon.png"
    if icon.exists():
        dst = DEST_IMAGES / "icon.png"
        if copy_preserve(icon, dst, args.dry_run):
            copied += 1
            print(f"{'would copy' if args.dry_run else 'copied'} images/icon.png")

    print(f"done: {copied} copied, {skipped} up-to-date ({len(oggs)} ogg, {len(pngs)} png)")
    if args.dry_run:
        print("dry-run: no files written")
    else:
        # Ensure gitkeep remains for empty check semantics
        for keep in [DEST_AUDIO / ".gitkeep", DEST_IMAGES / ".gitkeep"]:
            if not keep.exists():
                keep.touch(exist_ok=True)
        print("Note: original assets are gitignored (see .gitignore). Placeholders remain.")
        print("Bevy loads .ogg directly via `vorbis` feature - no conversion performed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
