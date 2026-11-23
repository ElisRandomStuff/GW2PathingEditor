#!/usr/bin/env python3
# Usage: python3 scripts/make_icon.py assets
# Generates the quick-access icons (64x64 RGBA PNG): a map pin with a dark outline.
import math, struct, sys, zlib

def teardrop(px, py, cx, cy, r, tx, ty):
    if (px - cx) ** 2 + (py - cy) ** 2 <= r * r:
        return True
    d = ty - cy
    th = math.acos(r / d)
    p1 = (cx - r * math.sin(th), cy + r * math.cos(th))
    p2 = (cx + r * math.sin(th), cy + r * math.cos(th))
    def side(a, b, p):
        return (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    t = (tx, ty)
    s1, s2, s3 = side(p1, p2, (px, py)), side(p2, t, (px, py)), side(t, p1, (px, py))
    return (s1 >= 0 and s2 >= 0 and s3 >= 0) or (s1 <= 0 and s2 <= 0 and s3 <= 0)

def render(fill, size=64, ss=4):
    cx, cy, r, tx, ty = 32.0, 24.0, 17.0, 32.0, 57.0
    out = 3.0
    dark = (24, 24, 24)
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(ss):
                for sx in range(ss):
                    px, py = x + (sx + 0.5) / ss, y + (sy + 0.5) / ss
                    if teardrop(px, py, cx, cy, r, tx, ty):
                        hole = (px - cx) ** 2 + (py - cy) ** 2 <= 6.5 ** 2
                        c = dark if hole else fill
                        a = 255
                    elif teardrop(px, py, cx, cy, r + out, tx, ty + out * 1.6):
                        c, a = dark, 230
                    else:
                        continue
                    acc[0] += c[0] * a; acc[1] += c[1] * a; acc[2] += c[2] * a; acc[3] += a
            n = ss * ss
            a = acc[3] / n
            if a > 0:
                row += bytes([round(acc[0] / acc[3]), round(acc[1] / acc[3]), round(acc[2] / acc[3]), round(a)])
            else:
                row += bytes(4)
        rows.append(bytes(row))
    raw = b"".join(rows)
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")

out_dir = sys.argv[1]
open(f"{out_dir}/icon.png", "wb").write(render((214, 150, 60)))
open(f"{out_dir}/icon_hover.png", "wb").write(render((255, 196, 92)))
