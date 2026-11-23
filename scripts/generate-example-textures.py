"""Procedurally generate original 32x32 block textures (no external sources)."""
import random, sys, os
from PIL import Image
S = 32
def clamp(v): return max(0, min(255, int(v)))
def base(seed, color, jitter):
    r = random.Random(seed); img = Image.new("RGBA", (S, S))
    px = img.load()
    for y in range(S):
        for x in range(S):
            j = r.uniform(-jitter, jitter)
            px[x, y] = tuple(clamp(c + j) for c in color) + (255,)
    return img, r, px
def speckle(px, r, color, n, size=1):
    for _ in range(n):
        x, y = r.randrange(S), r.randrange(S)
        for dx in range(size):
            for dy in range(size):
                px[(x+dx) % S, (y+dy) % S] = tuple(clamp(c + r.uniform(-12, 12)) for c in color) + (255,)
def blobs(px, r, color, n):
    for _ in range(n):
        cx, cy, rad = r.randrange(S), r.randrange(S), r.uniform(1.5, 3.2)
        for y in range(S):
            for x in range(S):
                dx = min(abs(x-cx), S-abs(x-cx)); dy = min(abs(y-cy), S-abs(y-cy))
                if dx*dx + dy*dy <= rad*rad:
                    px[x, y] = tuple(clamp(c + r.uniform(-15, 15)) for c in color) + (255,)
def stone(seed):
    img, r, px = base(seed, (122, 122, 126), 14); speckle(px, r, (95, 95, 100), 70, 2); speckle(px, r, (150, 150, 152), 40); return img, r, px
T = {}
def t(name):
    def d(f): T[name] = f; return f
    return d
@t("basic_world/stone")
def _(): return stone(1)[0]
@t("minetest/stone")
def _(): return stone(2)[0]
@t("basic_world/iron_ore")
def _(): img, r, px = stone(3); blobs(px, r, (196, 150, 118), 6); return img
@t("basic_world/crystal")
def _():
    img, r, px = base(4, (120, 80, 170), 10)
    for y in range(S):
        for x in range(S):
            if (x + y) % 8 in (0, 1): px[x, y] = tuple(clamp(c) for c in (175, 140, 220)) + (255,)
            if (x - y) % 11 == 0: px[x, y] = (80, 50, 120, 255)
    return img
@t("basic_world/rare_ore")
def _(): img, r, px = stone(5); blobs(px, r, (60, 200, 170), 5); speckle(px, r, (200, 255, 240), 10); return img
@t("minetest/grass_top")
def _(): img, r, px = base(6, (88, 150, 64), 18); speckle(px, r, (60, 120, 45), 90); speckle(px, r, (120, 180, 80), 50); return img
@t("minetest/dirt")
def _(): img, r, px = base(7, (128, 92, 62), 16); speckle(px, r, (95, 68, 45), 80, 2); return img
@t("minetest/sand")
def _(): img, r, px = base(8, (220, 205, 150), 10); speckle(px, r, (195, 180, 125), 60); return img
@t("minetest/gravel")
def _(): img, r, px = base(9, (130, 125, 120), 10); blobs(px, r, (100, 96, 92), 18); blobs(px, r, (160, 155, 150), 10); return img
@t("minetest/snow")
def _(): img, r, px = base(10, (240, 244, 250), 6); speckle(px, r, (215, 222, 235), 40); return img
@t("minetest/wood_log")
def _():
    img, r, px = base(11, (110, 82, 50), 8)
    for x in range(S):
        if x % 5 == 0 or r.random() < 0.08:
            for y in range(S): px[x, y] = tuple(clamp(c + r.uniform(-8, 8)) for c in (80, 58, 34)) + (255,)
    return img
@t("minetest/leaves")
def _():
    img, r, px = base(12, (60, 125, 50), 22); speckle(px, r, (35, 85, 30), 120); speckle(px, r, (95, 160, 70), 60)
    return img
@t("minetest/water")
def _():
    img, r, px = base(13, (50, 100, 200), 8)
    for y in range(S):
        for x in range(S):
            if (y + (x // 4) % 3) % 7 == 0: px[x, y] = (110, 160, 235, 255)
    img.putalpha(190); return img
out = sys.argv[1]
for name, f in T.items():
    p = os.path.join(out, name.split("/")[0], "textures", name.split("/")[1] + ".png")
    f().save(p, optimize=True); print(p)
