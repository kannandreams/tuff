"""Renders the Tuff Console promo video frame by frame and pipes it to ffmpeg.

Run through scripts/promo/build.sh, which prepares the screenshots and fonts
this script reads from $PROMO_DIR (shots/*.png, fonts/*.ttf).

    render.py <out.mp4>          render the whole video
    render.py - <t> [<t> ...]    write preview-<t>.png stills instead
"""
import math, os, subprocess, sys
from multiprocessing import get_context
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = os.environ.get("PROMO_DIR", "target/promo")
VERSION = os.environ["TUFF_VERSION"]
SHOTS = os.path.join(HERE, "shots")
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "promo-silent.mp4")
W, H, SS, FPS = 1920, 1080, 2, 30
BAR = 2.4
DURATION = 17 * BAR + 3.0
XFADE = 0.4

NAVY = (15, 23, 42); NAVY2 = (22, 33, 60); ORANGE = (232, 93, 42)
INK = (241, 245, 249); MUTED = (148, 163, 184)
def geist(w, px): return ImageFont.truetype(os.path.join(HERE, "fonts", f"Geist-{w}.ttf"), int(px * SS))
def mono(w, px): return ImageFont.truetype(os.path.join(HERE, "fonts", f"JetBrainsMono-{w}.ttf"), int(px * SS))

# ---- window card geometry (world = output pixels at zoom 1) ----
SRC_W, SRC_H, CHROME, PAD, RADIUS = 3200, 2000, 72, 220, 34
CARD_W = 1480; S = CARD_W / SRC_W                   # world px per source px
CARD_X = (W - CARD_W) / 2; CARD_Y = (H - (SRC_H + CHROME) * S) / 2
SHOT_Y = CARD_Y + CHROME * S
def wrect(x0, y0, x1, y1):                          # screenshot display coords (2000x1250) -> world
    k = 1.6 * S
    return (CARD_X + x0 * k, SHOT_Y + y0 * k, CARD_X + x1 * k, SHOT_Y + y1 * k)

def make_card(name):
    shot = Image.open(os.path.join(SHOTS, name + ".png")).convert("RGBA")
    win = Image.new("RGBA", (SRC_W, SRC_H + CHROME), (236, 235, 231, 255))
    d = ImageDraw.Draw(win)
    for i, c in enumerate([(255, 95, 87), (254, 188, 46), (40, 200, 64)]):
        cx = 40 + i * 38; d.ellipse((cx - 12, 36 - 12, cx + 12, 36 + 12), fill=c)
    f = ImageFont.truetype(os.path.join(HERE, "fonts", "Geist-500.ttf"), 26)
    label = "Tuff Console"; tw = d.textlength(label, font=f)
    d.text(((SRC_W - tw) / 2, 22), label, font=f, fill=(110, 110, 105))
    d.line((0, CHROME - 1, SRC_W, CHROME - 1), fill=(214, 212, 206), width=2)
    win.paste(shot, (0, CHROME))
    mask = Image.new("L", win.size, 0); ImageDraw.Draw(mask).rounded_rectangle((0, 0, *win.size), RADIUS / S * 0.5, fill=255)
    layer = Image.new("RGBA", (win.width + 2 * PAD, win.height + 2 * PAD), (0, 0, 0, 0))
    sh = Image.new("L", layer.size, 0)
    ImageDraw.Draw(sh).rounded_rectangle((PAD, PAD + 40, PAD + win.width, PAD + win.height + 40), 60, fill=150)
    sh = sh.filter(ImageFilter.GaussianBlur(70))
    layer.paste(Image.new("RGBA", layer.size, (0, 0, 0, 255)), (0, 0), sh)
    layer.paste(win, (PAD, PAD), mask)
    return layer

def make_bg():
    bg = Image.new("RGB", (W * SS // 4, H * SS // 4))
    px = bg.load(); bw, bh = bg.size
    for y in range(bh):
        for x in range(bw):
            u, v = x / bw, y / bh
            t = min(1, math.hypot(u - 0.15, v - 0.1) / 1.2)
            r, g, b = [NAVY2[i] * (1 - t) + NAVY[i] * t for i in range(3)]
            glow = max(0, 1 - math.hypot(u - 0.9, v - 1.0) / 0.55) ** 2 * 0.22
            px[x, y] = (int(r + (ORANGE[0] - r) * glow), int(g + (ORANGE[1] - g) * glow), int(b + (ORANGE[2] - b) * glow))
    return bg.resize((W * SS, H * SS), Image.BICUBIC).filter(ImageFilter.GaussianBlur(6)).convert("RGBA")

def pill(text, size=38):
    f = geist(600, size); tw = f.getlength(text); ph = int(86 * SS); pw = int(tw + 110 * SS)
    im = Image.new("RGBA", (pw, ph), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
    d.rounded_rectangle((0, 0, pw - 1, ph - 1), ph // 2, fill=(*NAVY, 238), outline=(51, 65, 85, 255), width=SS)
    d.ellipse((34 * SS, ph / 2 - 7 * SS, 48 * SS, ph / 2 + 7 * SS), fill=ORANGE)
    d.text((66 * SS, ph / 2), text, font=f, fill=INK, anchor="lm")
    return im

def title_panel():
    lines = [(mono("Bold", 24), f"tuffcli {VERSION}", ORANGE), (geist(700, 84), "Tuff Console", INK),
             (geist(400, 34), "The agent setup of every repository, in one place.", (203, 213, 225))]
    pw = int(max(f.getlength(t) for f, t, _ in lines) + 120 * SS); ph = int(270 * SS)
    im = Image.new("RGBA", (pw, ph), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
    d.rounded_rectangle((0, 0, pw - 1, ph - 1), 30 * SS, fill=(*NAVY, 244), outline=(51, 65, 85, 255), width=SS)
    d.text((60 * SS, 50 * SS), lines[0][1], font=lines[0][0], fill=lines[0][2])
    d.text((60 * SS, 86 * SS), lines[1][1], font=lines[1][0], fill=lines[1][2])
    d.text((60 * SS, 196 * SS), lines[2][1], font=lines[2][0], fill=lines[2][2])
    return im

# ---- camera ----
def ease(x): x = min(1, max(0, x)); return 4 * x ** 3 if x < 0.5 else 1 - (-2 * x + 2) ** 3 / 2
WIDE = (W / 2, H / 2, 1.0)
def cam(r, drift=1.0):
    x0, y0, x1, y1 = wrect(*r); w, h = x1 - x0, y1 - y0
    z = min(2.3, W / (w * 1.12), 860 / (h * 1.12)) * drift
    return clamp(((x0 + x1) / 2, (y0 + y1) / 2 + 60 / z, z))
def clamp(c):
    cx, cy, z = c; vw, vh = W / z, H / z
    l, r, t, b = CARD_X, CARD_X + CARD_W, CARD_Y, CARD_Y + (SRC_H + CHROME) * S
    if vw < r - l: cx = min(max(cx, l + vw / 2), r - vw / 2)
    if vh < b - t: cy = min(max(cy, t + vh / 2), b - vh / 2)
    return (cx, cy, z)
def wide(z): return (W / 2, H / 2, z)
def at(keys, t):
    if t <= keys[0][0]: return keys[0][1]
    for (t0, a), (t1, b) in zip(keys, keys[1:]):
        if t <= t1:
            k = ease((t - t0) / (t1 - t0))
            z = math.exp(math.log(a[2]) * (1 - k) + math.log(b[2]) * k)
            return (a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, z)
    return keys[-1][1]

R_STATS = (1120, 180, 1970, 295); R_ATTN = (285, 315, 1240, 850); R_MIXED = (285, 190, 1110, 400)
R_HHEAD = (285, 215, 1240, 500); R_HTABLE = (285, 200, 1965, 770); R_RULES = (285, 190, 1965, 440)
R_GUARD = (285, 465, 1240, 830); R_A1 = (285, 315, 1130, 630); R_A2 = (285, 550, 1130, 865)

SCENES = [  # (start, end, kind, card, camera keys, captions)
    (0.0, 9.6, "shot", "dashboard",
     [(0, wide(1.0)), (4.6, wide(1.035)), (5.9, cam(R_STATS)), (7.1, cam(R_STATS, 1.03)), (8.3, cam(R_ATTN)), (10, cam(R_ATTN, 1.03))],
     [(5.0, 7.15, "Drift, outdated versions, and policy gaps across every project"), (7.45, 9.6, "Needs attention lists each project with the reason")]),
    (9.6, 14.4, "shot", "capabilities",
     [(0, wide(1.0)), (0.5, wide(1.01)), (1.7, cam(R_MIXED)), (5.2, cam(R_MIXED, 1.04))],
     [(0.35, 4.8, "Mixed versions of a skill or MCP server across projects")]),
    (14.4, 19.2, "shot", "harnesses",
     [(0, cam(R_HHEAD)), (0.9, cam(R_HHEAD, 1.02)), (2.3, cam(R_HTABLE)), (5.2, wide(0.99))],
     [(0.35, 4.8, "Which harnesses each project installs capabilities for")]),
    (19.2, 24.0, "shot", "policies",
     [(0, wide(1.0)), (0.4, wide(1.01)), (1.5, cam(R_RULES)), (2.8, cam(R_RULES, 1.02)), (3.8, cam(R_GUARD)), (5.2, cam(R_GUARD, 1.03))],
     [(0.35, 4.8, "Every policy rule a harness does not enforce")]),
    (24.0, 28.8, "shot", "audit",
     [(0, cam(R_A1)), (0.5, cam(R_A1)), (4.5, cam(R_A2)), (5.2, cam(R_A2, 1.01))],
     [(0.35, 4.8, "An audit log with the commit behind every change")]),
    (28.8, 33.6, "ci", None, None, []),
    (33.6, DURATION, "end", None, None, []),
]

A = {}
def load():
    A["bg"] = make_bg()
    A["cards"] = {s[3]: make_card(s[3]) for s in SCENES if s[2] == "shot"}
    A["caps"] = {c[2]: pill(c[2]) for s in SCENES for c in s[5]}
    A["title"] = title_panel()
    A["ci"] = ci_layers(); A["end"] = end_layers()

def ci_layers():
    base = A["bg"].copy() if "bg" in A else make_bg(); d = ImageDraw.Draw(base)
    d.text((150 * SS, 330 * SS), "Publish from CI", font=geist(700, 72), fill=INK)
    body = ["On GitHub Actions, tuff console publish", "uses the job's OIDC token.", "There is no secret to store."]
    for i, line in enumerate(body): d.text((152 * SS, (450 + i * 54) * SS), line, font=geist(400, 36), fill=(203, 213, 225))
    d.text((152 * SS, 650 * SS), "Other CI systems use an API key", font=geist(500, 28), fill=MUTED)
    d.text((152 * SS, 690 * SS), "bound to one repository.", font=geist(500, 28), fill=MUTED)
    x0, y0, x1, y1 = 1010 * SS, 250 * SS, 1790 * SS, 830 * SS
    sh = Image.new("L", base.size, 0); ImageDraw.Draw(sh).rounded_rectangle((x0, y0 + 30 * SS, x1, y1 + 30 * SS), 24 * SS, fill=170)
    base.paste((0, 0, 0, 255), (0, 0), sh.filter(ImageFilter.GaussianBlur(40 * SS)))
    d = ImageDraw.Draw(base)
    d.rounded_rectangle((x0, y0, x1, y1), 22 * SS, fill=(17, 19, 21), outline=(48, 54, 61), width=SS)
    for i, c in enumerate([(255, 95, 87), (254, 188, 46), (40, 200, 64)]):
        cx = x0 + (34 + i * 26) * SS; d.ellipse((cx - 7 * SS, y0 + 27 * SS, cx + 7 * SS, y0 + 41 * SS), fill=c)
    d.text((x0 + 120 * SS, y0 + 34 * SS), ".github/workflows/tuff-console.yml", font=mono("Regular", 20), fill=(139, 148, 158), anchor="lm")
    code = [("permissions:", 0, "k"), ("id-token: write", 1, "k"), ("", 0, ""), ("steps:", 0, "k"),
            ("- run: tuff check", 1, "r"), ("- run: tuff console publish --all", 1, "r"), ("env:", 2, "k"),
            ("TUFF_CONSOLE_URL: https://tuff.acme.dev", 3, "k")]
    lines = []
    for i, (txt, ind, kind) in enumerate(code):
        im = Image.new("RGBA", base.size, (0, 0, 0, 0)); dd = ImageDraw.Draw(im)
        x = x0 + (40 + ind * 30) * SS; y = y0 + (100 + i * 54) * SS; f = mono("Regular", 27)
        if ":" in txt and not txt.startswith("- run"):
            k, v = txt.split(":", 1)
            dd.text((x, y), k + ":", font=f, fill=(255, 166, 87)); dd.text((x + f.getlength(k + ":"), y), v, font=f, fill=(165, 214, 255))
        elif txt.startswith("- run"):
            dd.text((x, y), "- run:", font=f, fill=(255, 166, 87))
            hl = "publish" in txt
            dd.text((x + f.getlength("- run:"), y), txt[6:], font=mono("Bold" if hl else "Regular", 27), fill=(126, 231, 135) if hl else (230, 237, 243))
        lines.append(im)
    return base, lines

def end_layers():
    word = mono("Bold", 230); tag = geist(500, 44); url = mono("Medium", 38)
    def centered(text, font, color, y):
        im = Image.new("RGBA", (W * SS, H * SS), (0, 0, 0, 0)); ImageDraw.Draw(im).text((W * SS / 2, y * SS), text, font=font, fill=color, anchor="mm"); return im
    u = centered("tuffcli.dev", url, INK, 790)
    d = ImageDraw.Draw(u); uw = url.getlength("tuffcli.dev")
    d.rounded_rectangle((W * SS / 2 - uw / 2 - 40 * SS, 790 * SS - 42 * SS, W * SS / 2 + uw / 2 + 40 * SS, 790 * SS + 42 * SS), 42 * SS, outline=ORANGE, width=3 * SS)
    return (centered("tuff", word, ORANGE, 430), centered("Manage and govern agent capabilities.", tag, (226, 232, 240), 610),
            centered(f"Tuff Console is in tuffcli {VERSION}", geist(400, 30), MUTED, 672), u)

def fade_in(t, t0, d=0.45): return ease((t - t0) / d)

def render_shot(sc, t):
    _, _, _, card, keys, caps = sc
    cx, cy, z = at(keys, t)
    layer = A["cards"][card]; k = 1 / (SS * z * S)
    ox = CARD_X - PAD * S; oy = CARD_Y - PAD * S
    tf = layer.transform((W * SS, H * SS), Image.AFFINE, (k, 0, (cx - W / 2 / z - ox) / S, 0, k, (cy - H / 2 / z - oy) / S), Image.BICUBIC)
    frame = A["bg"].copy(); frame.alpha_composite(tf)
    for t0, t1, text in caps:
        a = min(fade_in(t, t0), 1 - fade_in(t, t1 - 0.35, 0.35))
        if a <= 0: continue
        im = A["caps"][text]; y = int((H - 70) * SS - im.height + (1 - a) * 18 * SS)
        if a < 1: im = im.copy(); im.putalpha(im.getchannel("A").point(lambda v: int(v * a)))
        frame.alpha_composite(im, ((W * SS - im.width) // 2, y))
    if card == "dashboard":
        a = 1 - fade_in(t, 4.15, 0.5)
        if a > 0:
            im = A["title"]
            if a < 1: im = im.copy(); im.putalpha(im.getchannel("A").point(lambda v: int(v * a)))
            frame.alpha_composite(im, (110 * SS, int((H - 90) * SS - im.height + (1 - a) * 20 * SS)))
    return frame.convert("RGB").resize((W, H), Image.LANCZOS)

def with_alpha(im, a):
    if a >= 1: return im
    im = im.copy(); im.putalpha(im.getchannel("A").point(lambda v: int(v * a))); return im

def render_ci(t):
    base, lines = A["ci"]; frame = base.copy()
    for i, im in enumerate(lines):
        a = fade_in(t, 0.5 + i * 0.22, 0.35)
        if a > 0: frame.alpha_composite(with_alpha(im, a), (0, int((1 - a) * 10 * SS)))
    zz = 1 + 0.04 * ease(t / 5.0); bw, bh = W * SS / zz, H * SS / zz
    return frame.convert("RGB").resize((W, H), Image.LANCZOS, box=((W * SS - bw) / 2, (H * SS - bh) / 2, (W * SS + bw) / 2, (H * SS + bh) / 2))

def render_end(t):
    word, tag, sub, url = A["end"]; frame = A["bg"].copy()
    a = fade_in(t, 0.25, 0.8); s = 0.92 + 0.08 * a
    if a > 0:
        w2 = with_alpha(word, a).resize((int(W * SS * s), int(H * SS * s)), Image.BICUBIC)
        frame.alpha_composite(w2, ((W * SS - w2.width) // 2, int((H * SS - w2.height) / 2 + (430 - 540) * SS * (1 - s))))
    for im, t0 in ((tag, 0.75), (sub, 1.05), (url, 1.35)):
        a = fade_in(t, t0, 0.5)
        if a > 0: frame.alpha_composite(with_alpha(im, a), (0, int((1 - a) * 14 * SS)))
    zz = 1 + 0.025 * ease(t / 8); bw, bh = W * SS / zz, H * SS / zz
    return frame.convert("RGB").resize((W, H), Image.LANCZOS, box=((W * SS - bw) / 2, (H * SS - bh) / 2, (W * SS + bw) / 2, (H * SS + bh) / 2))

def render_scene(sc, t):
    if sc[2] == "shot": return render_shot(sc, t)
    if sc[2] == "ci": return render_ci(t)
    return render_end(t)

def frame(i):
    t = i / FPS
    for j, sc in enumerate(SCENES):
        if sc[0] <= t < sc[1] or j == len(SCENES) - 1:
            cur = render_scene(sc, t - sc[0])
            if j > 0 and t - sc[0] < XFADE / 2:          # second half of a crossfade from the previous scene
                prev = SCENES[j - 1]; k = 0.5 + (t - sc[0]) / XFADE
                return Image.blend(render_scene(prev, t - prev[0]), cur, k).tobytes()
            if j < len(SCENES) - 1 and sc[1] - t < XFADE / 2:
                nxt = SCENES[j + 1]; k = 0.5 - (sc[1] - t) / XFADE
                return Image.blend(cur, render_scene(nxt, t - nxt[0]), k).tobytes()
            return cur.tobytes()

if __name__ == "__main__":
    only = [float(x) for x in sys.argv[2:]]
    ctx = get_context("fork"); load()
    if only:
        for t in only: Image.frombytes("RGB", (W, H), frame(int(t * FPS))).save(os.path.join(HERE, f"preview-{t:05.2f}.png"))
        sys.exit()
    n = int(DURATION * FPS)
    ff = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                           "-c:v", "libx264", "-preset", "slow", "-crf", "16", "-pix_fmt", "yuv420p", "-movflags", "+faststart", OUT], stdin=subprocess.PIPE)
    with ctx.Pool(12) as pool:
        for k, buf in enumerate(pool.imap(frame, range(n), chunksize=2)):
            ff.stdin.write(buf)
            if k % 150 == 0: print(f"{k}/{n}", flush=True)
    ff.stdin.close(); ff.wait(); print("done", OUT)
