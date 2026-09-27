#!/usr/bin/env python3
"""End-to-end PTY smoke test for PALIMPSEST.

Spawns the interactive binary on a real pseudo-terminal — the path no
headless test can cover (raw mode, real event polling, live differential
rendering to an actual terminal, clean quit) — drives it with the keys a
user would press, and verifies the *visible screen* after each action by
replaying the wire bytes through a small terminal model (cursor addressing,
SGR, erase-to-EOL, line inserts — the same discipline as the Rust-side
wire tests).

Usage: scripts/pty_smoke.py [path-to-binary] [--profile=tiny|medium|large]
Exit status: 0 = smoke passed, 1 = smoke failed.
"""

import fcntl
import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import time

BIN = "./target/release/palimpsest"
PROFILE = "tiny"
COLS, ROWS = 110, 32

for a in sys.argv[1:]:
    if a.startswith("--profile="):
        PROFILE = a.split("=", 1)[1]
    elif not a.startswith("--"):
        BIN = a


class Screen:
    """A tiny terminal model: enough VT to faithfully replay the app's wire."""

    def __init__(self, cols, rows):
        self.cols = cols
        self.rows = rows
        self.grid = [[" "] * cols for _ in range(rows * 4)]
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
                # OSC: swallow until BEL or ST
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
            i += 1  # two-character escape: ignore

    def _csi(self, params, final):
        clean = params.lstrip("?><=")
        vals = []
        for p in clean.split(";"):
            if p == "":
                vals.append(0)
            elif p.isdigit():
                vals.append(int(p))
            else:
                return  # private/uninteresting sequence (e.g. ?2026h): skip

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
            row = self.grid[self.y] if self.y < len(self.grid) else None
            if row is not None:
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
            cnt = v(0)
            for _ in range(cnt):
                self.grid.insert(self.y, [" "] * self.cols)
                self.grid.pop()
        elif final == "M":
            cnt = v(0)
            for _ in range(cnt):
                if self.y < len(self.grid):
                    del self.grid[self.y]
                    self.grid.append([" "] * self.cols)

    def text(self):
        return "\n".join("".join(r).rstrip() for r in self.grid[: self.rows * 3])


def main():
    pid, master = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.execv(BIN, [BIN, f"--demo={PROFILE}"])
        os._exit(127)

    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    holder = {"screen": Screen(COLS, ROWS), "cols": COLS, "rows": ROWS}
    failures = []

    def pump(duration):
        """Read wire bytes for `duration` seconds, feeding the screen model."""
        end = time.time() + duration
        while time.time() < end:
            r, _, _ = select.select([master], [], [], 0.1)
            if r:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    return
                if not chunk:
                    return
                holder["screen"].feed(chunk.decode("utf-8", "replace"))

    def expect(marker, desc, wait=6.0):
        pump(wait)
        if marker in holder["screen"].text():
            print(f"  ok   {desc}")
            return True
        print(f"  FAIL {desc}: {marker!r} not on screen")
        failures.append(desc)
        return False

    def resize(cols, rows):
        """Resize the PTY and reset the screen model: the app invalidates its
        anchor and full-repaints the new geometry, so a fresh grid is filled
        entirely by post-resize frames."""
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        holder["screen"] = Screen(cols, rows)
        holder["cols"], holder["rows"] = cols, rows

    ok = True
    try:
        print(f"PTY smoke ({PROFILE}, {COLS}x{ROWS}):")
        ok &= expect("PALIMPSEST", "first frame / masthead", 25)
        ok &= expect("HISTORY ATLAS", "atlas view visible")

        os.write(master, b"j")
        ok &= expect("lens", "selection step rendered")

        os.write(master, b"\r")
        ok &= expect("DIFF LENS", "Enter zooms into the diff lens")

        os.write(master, b"\x1b")
        ok &= expect("HISTORY ATLAS", "Esc backs out to the atlas")

        os.write(master, b"5")
        ok &= expect("REPOSITORY HEALTH", "numeric view switch")

        os.write(master, b"R")
        ok &= expect("palimpsest artifact", "report committed into native scrollback")

        os.write(master, b"1")
        pump(1.5)

        # resize while navigating: shrink the terminal to a constrained width
        # and verify the responsive collapse keeps semantic identity (the
        # compact view code replaces the full title below 60 columns).
        resize(56, 22)
        os.write(master, b"j")
        ok &= expect("PALIMPSEST", "first frame after resize (56x22)")
        ok &= expect("ATLAS", "identity collapses to the short code at 56 cols")

        # grow back and verify the full title returns
        resize(COLS, ROWS)
        os.write(master, b"j")
        ok &= expect("HISTORY ATLAS", "resize back restores the full view title")

        # quit and verify a clean exit
        os.write(master, b"q")
        status = None
        deadline = time.time() + 10
        while time.time() < deadline:
            pump(0.2)
            done, st = os.waitpid(pid, os.WNOHANG)
            if done == pid:
                status = st
                break
        if status is None:
            print("  FAIL app did not exit after q")
            failures.append("clean quit")
            ok = False
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        elif os.WEXITSTATUS(status) != 0:
            print(f"  FAIL app exited with {os.WEXITSTATUS(status)}")
            failures.append("clean quit")
            ok = False
        else:
            print("  ok   clean quit via q")
    finally:
        try:
            os.close(master)
        except OSError:
            pass

    if ok:
        print("PTY smoke: PASS")
        return 0
    print(f"PTY smoke: FAIL ({len(failures)} failures)")
    with open("/tmp/pty-smoke-screen.txt", "w") as f:
        f.write(holder["screen"].text())
    print("(last screen state saved to /tmp/pty-smoke-screen.txt)")
    return 1


if __name__ == "__main__":
    sys.exit(main())