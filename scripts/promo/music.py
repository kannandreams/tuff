"""Synthesises an original 100 BPM backing track for the promo video.

    music.py <out.wav> [bars]
"""
import sys, wave
from itertools import accumulate

import numpy as np

SR = 48000
BPM = 100
BEAT = 60 / BPM
BAR = 4 * BEAT
BARS = int(sys.argv[2]) if len(sys.argv) > 2 else 17
TAIL = 3.0
N = int((BARS * BAR + TAIL) * SR)
rng = np.random.default_rng(7)

def hz(m): return 440.0 * 2 ** ((m - 69) / 12)
def env(n, a, r, sustain=True):
    t = np.arange(n) / SR
    e = np.minimum(1, t / max(a, 1e-4))
    if r: e *= np.exp(-t / r) if not sustain else np.minimum(1, (n / SR - t) / r)
    return e
def onepole(x, cutoff):  # one-pole low-pass filter
    a = np.exp(-2 * np.pi * cutoff / SR)
    return np.fromiter(accumulate(x * (1 - a), lambda p, v: p * a + v), float, len(x))
def place(buf, sig, t):
    i = int(t * SR); j = min(len(buf), i + len(sig)); buf[i:j] += sig[: j - i]

# vi-IV-I-V in C, voiced warm. One chord per bar.
CHORDS = [[57, 60, 64, 67], [53, 57, 60, 64], [48, 55, 60, 64], [55, 59, 62, 67]]
ROOTS = [33, 29, 36, 31]
DRUMS_FROM, DRUMS_TO = 2, BARS - 3   # drums enter on bar 2, stop for the end card

pad = np.zeros(N); bass = np.zeros(N); arp = np.zeros(N); kick = np.zeros(N); hat = np.zeros(N); fx = np.zeros(N)

for b in range(BARS + 1):
    t0 = b * BAR
    chord = CHORDS[b % 4] if b < BARS else [48, 55, 60, 64, 72]
    length = BAR if b < BARS else TAIL
    n = int((length + 0.6) * SR); t = np.arange(n) / SR
    # pad: detuned saw-ish stack
    s = np.zeros(n)
    for m in chord:
        for d in (-0.08, 0.08):
            f = hz(m + d)
            for k in range(1, 6): s += np.sin(2 * np.pi * f * k * t + k) / k ** 1.3
    s *= env(n, 0.35, 0.6) * 0.035
    place(pad, s, t0)
    if b >= BARS: continue
    # bass: eighth-note pulse
    for e in range(8):
        n2 = int(BEAT / 2 * SR); t2 = np.arange(n2) / SR
        f = hz(ROOTS[b % 4] + (12 if e % 4 == 3 else 0))
        sig = (np.sin(2 * np.pi * f * t2) + 0.25 * np.sin(4 * np.pi * f * t2)) * np.exp(-t2 / 0.18) * np.minimum(1, t2 / 0.005)
        place(bass, sig * (0.18 if b >= DRUMS_FROM else 0.09), t0 + e * BEAT / 2)
    # arp: sixteenth plucks over chord tones
    pattern = [0, 2, 1, 3, 2, 1, 3, 2]
    for e in range(16):
        if b < 1 and e % 2: continue
        m = chord[pattern[e % 8]] + 12
        n3 = int(0.4 * SR); t3 = np.arange(n3) / SR
        sig = (np.sin(2 * np.pi * hz(m) * t3) + 0.3 * np.sin(2 * np.pi * hz(m) * 2 * t3)) * np.exp(-t3 / 0.09) * np.minimum(1, t3 / 0.002)
        place(arp, sig * 0.05 * (0.8 + 0.2 * (e % 4 == 0)), t0 + e * BEAT / 4)
    if DRUMS_FROM <= b < DRUMS_TO:
        for q in range(4):
            n4 = int(0.35 * SR); t4 = np.arange(n4) / SR
            f = 45 + 90 * np.exp(-t4 / 0.03)
            ph = 2 * np.pi * np.cumsum(f) / SR
            place(kick, np.sin(ph) * np.exp(-t4 / 0.12) * 0.55, t0 + q * BEAT)
            n5 = int(0.06 * SR)
            h = rng.standard_normal(n5); h = h - onepole(h, 6000)
            place(hat, h * np.exp(-np.arange(n5) / SR / 0.015) * 0.05, t0 + q * BEAT + BEAT / 2)

# a noise swell into the drop
n6 = int(BAR * SR); sw = rng.standard_normal(n6); sw = onepole(sw, 3000) - onepole(sw, 400)
place(fx, sw * np.linspace(0, 1, n6) ** 3 * 0.12, (DRUMS_FROM - 1) * BAR)

# sidechain: duck pad and bass on each kick
duck = np.ones(N)
for b in range(DRUMS_FROM, DRUMS_TO):
    for q in range(4):
        i = int((b * BAR + q * BEAT) * SR); n7 = int(BEAT * SR); tt = np.arange(n7) / SR
        duck[i:i + n7] = np.minimum(duck[i:i + n7], 1 - 0.55 * np.exp(-tt / 0.12))

pad = onepole(pad, 2600)
mix = (pad + bass) * duck + arp + kick + hat + fx
# stereo: arp and hats slightly wide
left = mix + 0.15 * np.roll(arp, int(0.011 * SR)); right = mix + 0.15 * np.roll(arp, int(0.017 * SR))
st = np.stack([left, right], 1)
fade = np.ones(N); fl = int(2.5 * SR); fade[-fl:] = np.linspace(1, 0, fl) ** 2; fade[:int(0.02 * SR)] = np.linspace(0, 1, int(0.02 * SR))
st *= fade[:, None]
st = np.tanh(st * 1.4); st /= np.abs(st).max() / 0.89
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR)
    w.writeframes((st * 32767).astype("<i2").tobytes())
print(f"{N / SR:.2f}s")
