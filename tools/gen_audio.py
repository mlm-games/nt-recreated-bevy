#!/usr/bin/env python3
"""Generate placeholder SFX WAVs for the NT recreation.

Pure stdlib (wave/math/struct). These are synthesized blips/noises, not
copies of any copyrighted material. Run from repo root:

    python3 tools/gen_audio.py
"""
import math
import struct
import wave
from pathlib import Path

SR = 22050
OUT = Path(__file__).resolve().parent.parent / "assets" / "audio"
OUT.mkdir(parents=True, exist_ok=True)


def write_wav(name, samples):
    samples = [max(-1.0, min(1.0, s)) for s in samples]
    data = b"".join(struct.pack("<h", int(s * 32767)) for s in samples)
    with wave.open(str(OUT / name), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data)


def noise():
    import random

    return random.uniform(-1.0, 1.0)


def blip(dur, freq, decay=0.0, wave_fn=math.sin):
    n = int(SR * dur)
    out = []
    for i in range(n):
        t = i / SR
        env = math.exp(-decay * t) if decay > 0 else (1 - i / n)
        out.append(wave_fn(2 * math.pi * freq * t) * env)
    return out


def sweep(dur, f0, f1, wave_fn=math.sin, decay=0.0):
    n = int(SR * dur)
    out = []
    phase = 0.0
    for i in range(n):
        t = i / SR
        freq = f0 + (f1 - f0) * (i / n)
        phase += 2 * math.pi * freq / SR
        env = math.exp(-decay * t) if decay > 0 else (1 - i / n)
        out.append(wave_fn(phase) * env)
    return out


def mix(*tracks):
    n = max(len(t) for t in tracks)
    out = [0.0] * n
    for t in tracks:
        for i, s in enumerate(t):
            out[i] += s
    return out


def apply_env(samples, attack=0.002, release=0.02):
    n = len(samples)
    a = int(SR * attack)
    r = int(SR * release)
    out = list(samples)
    for i in range(n):
        env = 1.0
        if i < a:
            env = i / a
        elif i > n - r:
            env = max(0.0, (n - i) / r)
        out[i] *= env
    return out


def noise_burst(dur, gain=1.0):
    n = int(SR * dur)
    return [noise() * gain * (1 - i / n) for i in range(n)]


def gen_shoot():
    tone = blip(0.09, 750, decay=40)
    nse = noise_burst(0.05, 0.5)
    s = mix(tone, nse)
    write_wav("shoot.wav", s)


def gen_machine():
    tone = blip(0.05, 1200, decay=55)
    nse = noise_burst(0.03, 0.4)
    write_wav("machine.wav", mix(tone, nse))


def gen_shotgun():
    tone = blip(0.12, 180, decay=30)
    nse = noise_burst(0.1, 0.8)
    write_wav("shotgun.wav", mix(tone, nse))


def gen_bolt():
    s = sweep(0.16, 300, 1400, decay=8)
    write_wav("bolt.wav", apply_env(s))


def gen_melee():
    s = sweep(0.12, 1400, 280, decay=10)
    nse = noise_burst(0.06, 0.3)
    write_wav("melee.wav", apply_env(mix(s, nse)))


def gen_explode():
    nse = noise_burst(0.4, 1.0)
    low = blip(0.3, 90, decay=9)
    write_wav("explode.wav", apply_env(mix(nse, low), release=0.3))


def gen_boom():
    nse = noise_burst(0.65, 1.0)
    low = blip(0.5, 55, decay=6)
    write_wav("boom.wav", apply_env(mix(nse, low), release=0.5))


def gen_hit():
    write_wav("hit.wav", apply_env(blip(0.05, 520, decay=60), release=0.01))


def gen_hurt():
    s = sweep(0.2, 240, 110, wave_fn=math.sin, decay=6)
    write_wav("hurt.wav", apply_env(s, release=0.1))


def gen_pickup():
    write_wav("pickup.wav", apply_env(blip(0.09, 880, decay=25), release=0.02))


def gen_levelup():
    notes = [523.25, 659.25, 783.99]
    out = []
    for f in notes:
        out += [0.0] * int(SR * 0.02) + list(blip(0.16, f, decay=10))
    write_wav("levelup.wav", apply_env(out, attack=0.01, release=0.15))


def gen_portal():
    s = sweep(0.5, 180, 900, decay=2)
    write_wav("portal.wav", apply_env(s, release=0.25))


def gen_death():
    s = sweep(0.7, 400, 60, decay=3)
    nse = noise_burst(0.4, 0.5)
    write_wav("death.wav", apply_env(mix(s, nse), release=0.4))


def gen_chest():
    a = blip(0.12, 660, decay=20)
    b = blip(0.18, 990, decay=15)
    out = a + [0.0] * int(SR * 0.05) + b
    write_wav("chest.wav", apply_env(out, release=0.15))


if __name__ == "__main__":
    for f in [gen_shoot, gen_machine, gen_shotgun, gen_bolt, gen_melee, gen_explode, gen_boom, gen_hit, gen_hurt, gen_pickup, gen_levelup, gen_portal, gen_death, gen_chest]:
        f()
        print("generated", f.__name__)
    print("done ->", OUT)
