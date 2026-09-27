#!/usr/bin/env python3
"""Flatten a PALIMPSEST `--dump` byte stream to the visible screen.

`--dump` emits the exact wire bytes a terminal would receive (cursor
addressing, SGR, erase-to-EOL). This tool replays them through a small
terminal model and prints the resulting visible grid — the honest capture
for review and diffing, and the same discipline the Rust wire tests use.

Usage:
    palimpsest --dump --width=120 --height=40 [flags...] \
        | python3 scripts/flatten_dump.py 120 40 > capture.txt

Arguments: <cols> <rows> (must match the --dump geometry).
"""

import sys


class Screen:
    def __init__(self, cols, rows):
        self.cols = cols
        self.rows = rows
        self.grid = [[" "] * cols for _ in range(rows * 2)]
        self.x = self.y = 0

    def _put(self, ch):
        if self.x < self.cols and self.y < len(self.grid):
            self.grid[self.y][self.x] = ch
        self.x += 1

    def feed(self, text):
        i, n = 0, len(text)
        while i < n:
            c = text[i]
            i += 1
            if c != "\x1b":
                if c == "\r":
                    self.x = 0
                elif c == "\n":
                    self.y = min(len(self.grid) - 1, self.y + 1)
                    self.x = 0
                elif c == "\x07":
                    pass
                elif c >= " ":
                    self._put(c)
                continue
            if i >= n:
                break
            if text[i] == "[":
                i += 1
                params = ""
                while i < n:
                    ch = text[i]
                    i += 1
                    if ch.isalpha() or ch == "~":
                        self._csi(params, ch)
                        break
                    params += ch
                continue
            if text[i] == "]":
                i += 1
                while i < n:
                    o = text[i]
                    i += 1
                    if o == "\x07":
                        break
                    if o == "\x1b" and i < n and text[i] == "\\":
                        i += 1
                        break
                continue
            i += 1

    def _csi(self, params, final):
        clean = params.lstrip("?><=")
        vals = []
        for p in clean.split(";"):
            if p == "":
                vals.append(0)
            elif p.isdigit():
                vals.append(int(p))
            else:
                return

        def v(k, d=1):
            return vals[k] if len(vals) > k and vals[k] else d

        if final == "A":
            self.y = max(0, self.y - v(0))
        elif final in "Be":
            self.y = min(len(self.grid) - 1, self.y + v(0))
        elif final in "Ca":
            self.x = min(self.cols - 1, self.x + v(0))
        elif final == "D":
            self.x = max(0, self.x - v(0))
        elif final in "G`":
            self.x = max(0, v(0, 1) - 1)
        elif final == "d":
            self.y = max(0, v(0, 1) - 1)
        elif final in "Hf":
            self.y = max(0, v(0, 1) - 1)
            self.x = max(0, v(1, 1) - 1)
        elif final == "K":
            m = vals[0] if vals else 0
            if self.y < len(self.grid):
                row = self.grid[self.y]
                if m == 0:
                    for xx in range(self.x, self.cols):
                        row[xx] = " "
                elif m == 2:
                    for xx in range(self.cols):
                        row[xx] = " "
        elif final == "J":
            m = vals[0] if vals else 0
            if m == 2:
                for yy in range(len(self.grid)):
                    self.grid[yy] = [" "] * self.cols
                self.x = self.y = 0
            elif m == 0:
                if self.y < len(self.grid):
                    for xx in range(self.x, self.cols):
                        self.grid[self.y][xx] = " "
                for yy in range(self.y + 1, len(self.grid)):
                    self.grid[yy] = [" "] * self.cols
        elif final == "L":
            for _ in range(v(0)):
                self.grid.insert(self.y, [" "] * self.cols)
                self.grid.pop()

    def text(self):
        return "\n".join("".join(r).rstrip() for r in self.grid[: self.rows])


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    cols, rows = int(sys.argv[1]), int(sys.argv[2])
    screen = Screen(cols, rows)
    screen.feed(sys.stdin.read())
    print(screen.text())
    return 0


if __name__ == "__main__":
    sys.exit(main())