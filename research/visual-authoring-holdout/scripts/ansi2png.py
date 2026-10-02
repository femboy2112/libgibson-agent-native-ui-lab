#!/usr/bin/env python3
"""Uniform ANSI-frame -> PNG normalizer for the holdout blind packet.

One pipeline for EVERY candidate: pyte (reconstruct the exact cell grid from the
app's `--capture WxH[:depth]` stdout frame) -> Pillow (block glyphs as exact
rects, text/Braille via DejaVu Sans Mono). Fixed cell geometry => no crop, no
post-processing, no color correction, no compositional change. Reused from
LibGibson's docs/assets/render_hero.py render core.

Usage: ansi2png.py --in frame.ansi --width 120 --height 40 --out frame.png
"""
import argparse
import pyte
from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
DEF_FG = (205, 210, 215); DEF_BG = (0, 0, 0)


def to_rgb(val, default):
    if val == "default" or val is None:
        return default
    if isinstance(val, str) and len(val) == 6:
        try:
            return (int(val[0:2], 16), int(val[2:4], 16), int(val[4:6], 16))
        except ValueError:
            pass
    named = {"black": (0, 0, 0), "red": (205, 0, 0), "green": (0, 205, 0),
             "brown": (205, 205, 0), "yellow": (255, 255, 0), "blue": (0, 0, 238),
             "magenta": (205, 0, 205), "cyan": (0, 205, 205), "white": (229, 229, 229)}
    return named.get(val, default)


def render(screen, cols, rows, cw, ch, font_size, out):
    img = Image.new("RGB", (cols * cw, rows * ch), DEF_BG)
    d = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONT, font_size)
    for y in range(rows):
        line = screen.buffer[y]
        for x in range(cols):
            c = line[x]
            ch_ = c.data or " "
            fg = to_rgb(c.fg, DEF_FG); bg = to_rgb(c.bg, DEF_BG)
            if c.reverse:
                fg, bg = bg, fg
            px, py = x * cw, y * ch
            d.rectangle([px, py, px + cw - 1, py + ch - 1], fill=bg)
            if ch_ in (" ", "\x00"):
                continue
            if ch_ == "█":
                d.rectangle([px, py, px + cw - 1, py + ch - 1], fill=fg); continue
            if ch_ == "▀":
                d.rectangle([px, py, px + cw - 1, py + ch // 2 - 1], fill=fg); continue
            if ch_ == "▄":
                d.rectangle([px, py + ch // 2, px + cw - 1, py + ch - 1], fill=fg); continue
            if ch_ == "▌":
                d.rectangle([px, py, px + cw // 2 - 1, py + ch - 1], fill=fg); continue
            if ch_ == "▐":
                d.rectangle([px + cw // 2, py, px + cw - 1, py + ch - 1], fill=fg); continue
            if ch_ in "░▒▓":
                a = {"░": 0.25, "▒": 0.5, "▓": 0.75}[ch_]
                col = tuple(int(bg[i] + (fg[i] - bg[i]) * a) for i in range(3))
                d.rectangle([px, py, px + cw - 1, py + ch - 1], fill=col); continue
            lower8 = "▁▂▃▄▅▆▇█"
            if ch_ in lower8:
                frac = (lower8.index(ch_) + 1) / 8.0
                top = py + int(ch * (1 - frac))
                d.rectangle([px, top, px + cw - 1, py + ch - 1], fill=fg); continue
            try:
                bb = d.textbbox((0, 0), ch_, font=font)
                gw = bb[2] - bb[0]; gh = bb[3] - bb[1]
                ox = px + (cw - gw) // 2 - bb[0]; oy = py + (ch - gh) // 2 - bb[1]
                d.text((ox, oy), ch_, font=font, fill=fg)
            except Exception:
                pass
    img.save(out)
    return img.size


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--in", dest="inp", required=True)
    ap.add_argument("--width", type=int, default=120)
    ap.add_argument("--height", type=int, default=40)
    ap.add_argument("--cw", type=int, default=12)
    ap.add_argument("--ch", type=int, default=24)
    ap.add_argument("--font-size", type=int, default=22)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    data = open(a.inp, "rb").read()
    scr = pyte.Screen(a.width, a.height)
    st = pyte.Stream(scr)
    st.feed(data.decode("utf-8", "replace"))
    size = render(scr, a.width, a.height, a.cw, a.ch, a.font_size, a.out)
    nonblank = sum(1 for y in range(a.height) for x in range(a.width)
                   if (scr.buffer[y][x].data or " ") != " ")
    print(f"{a.inp}: nonblank={nonblank} image={size} -> {a.out}")


if __name__ == "__main__":
    main()
