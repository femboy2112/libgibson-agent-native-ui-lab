#!/usr/bin/env python3
"""Render a terminal byte stream (what a Context actually emitted) to a PNG.

usage: ansi_to_png.py STREAM.ansi COLSxROWS OUT.png [--scale N]

The stream is interpreted by pyte (a real VT emulator) and drawn with DejaVu Sans Mono.
This is a preview aid, not a LibGibson feature: fonts, subpixel layout and colour handling
here are NOT claims about any real terminal.
"""
import sys, pyte
from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_B = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
FONT_SANS = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"

NAMED = {
    "black": (0, 0, 0), "red": (205, 49, 49), "green": (13, 188, 121), "brown": (229, 229, 16),
    "blue": (36, 114, 200), "magenta": (188, 63, 188), "cyan": (17, 168, 205), "white": (229, 229, 229),
    "brightblack": (102, 102, 102), "brightred": (241, 76, 76), "brightgreen": (35, 209, 139),
    "brightbrown": (245, 245, 67), "brightblue": (59, 142, 234), "brightmagenta": (214, 112, 214),
    "brightcyan": (41, 184, 219), "brightwhite": (255, 255, 255),
}

def rgb(c, default):
    if c == "default":
        return default
    if c in NAMED:
        return NAMED[c]
    if len(c) == 6:
        try:
            return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))
        except ValueError:
            pass
    return default

def main():
    stream_path, geom, out = sys.argv[1:4]
    scale = 1
    if "--scale" in sys.argv:
        scale = int(sys.argv[sys.argv.index("--scale") + 1])
    cols, rows = (int(x) for x in geom.split("x"))
    screen = pyte.Screen(cols, rows)
    st = pyte.ByteStream(screen)
    st.feed(open(stream_path, "rb").read())
    cw, ch = 10, 20
    font = ImageFont.truetype(FONT, 17)
    fontb = ImageFont.truetype(FONT_B, 17)
    fonts = ImageFont.truetype(FONT_SANS, 17)
    from fontTools.ttLib import TTFont
    mono_cmap = TTFont(FONT).getBestCmap()
    img = Image.new("RGB", (cols * cw, rows * ch), (0, 0, 0))
    d = ImageDraw.Draw(img)
    DEF_FG, DEF_BG = (200, 200, 200), (0, 0, 0)
    for y in range(rows):
        line = screen.buffer[y]
        for x in range(cols):
            c = line[x]
            fg, bg = rgb(c.fg, DEF_FG), rgb(c.bg, DEF_BG)
            if c.reverse:
                fg, bg = bg, fg
            d.rectangle([x * cw, y * ch, x * cw + cw - 1, y * ch + ch - 1], fill=bg)
            if c.data and c.data != " ":
                f = fontb if c.bold else font
                if ord(c.data[0]) not in mono_cmap:
                    f = fonts
                d.text((x * cw, y * ch), c.data, font=f, fill=fg)
    if scale != 1:
        img = img.resize((img.width * scale, img.height * scale), Image.NEAREST)
    img.save(out)
    print(out, img.size)

main()
