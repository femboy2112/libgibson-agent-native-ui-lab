#!/usr/bin/env python3
"""Bounded PTY stress probe for Project Cathedral's LibGibson consumer path.

This drives the *interactive* `cathedral` binary inside a real pseudo-terminal and
exercises the pressure cases the experiment report claims: resize during the
catastrophic cascade, rapid operator commands, output backpressure, scrollback
commits, live interaction during animation, and terminal restoration on exit.

Honesty boundary: Cathedral (unlike Synesthesia) does not print per-event input
receipts at shutdown, so this probe cannot prove one-to-one key delivery. It
verifies observable *side effects* of a subset of actions (each operator action
emits a `◇ ACTION` line into native scrollback) plus liveness, resize survival
and terminal restoration. Absence of that receipt channel is itself reported.
"""

import argparse
import fcntl
import json
import os
import pty
import re
import select
import struct
import sys
import termios
import time


MAX_CAPTURE = 48 * 1024 * 1024

ALT_SCREEN_ENTER = b"\x1b[?1049h"
ALT_SCREEN_LEAVE = b"\x1b[?1049l"
CURSOR_HIDE = b"\x1b[?25l"
CURSOR_SHOW = b"\x1b[?25h"


def resize(master_fd, cols, rows):
    fcntl.ioctl(master_fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def drain(master_fd, seconds, capture):
    """Read available output for `seconds`, rolling the capture to bound memory."""
    deadline = time.monotonic() + seconds
    total = 0
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        ready, _, _ = select.select([master_fd], [], [], min(0.03, remaining))
        if not ready:
            continue
        try:
            chunk = os.read(master_fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        total += len(chunk)
        capture.extend(chunk)
        if len(capture) > MAX_CAPTURE:
            del capture[: len(capture) - MAX_CAPTURE]
    return total


def strip_ansi(data):
    # CSI + OSC (the latter covers hyperlink/title sequences LibGibson may emit on restore)
    data = re.sub(rb"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)", b"", data)
    return re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data)


def text_of(capture):
    return strip_ansi(bytes(capture)).decode("utf-8", "replace")


def seen(capture, needle):
    return needle in bytes(capture) or needle in text_of(capture)


def run(args):
    argv = [args.binary, "--wtf", "--no-music", "--fps=%d" % args.fps]
    if args.music:
        argv = [args.binary, "--wtf", "--fps=%d" % args.fps]

    pid, master = pty.fork()
    if pid == 0:
        env = os.environ.copy()
        env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "LANG": "C.UTF-8"})
        os.execve(args.binary, argv, env)
        os._exit(127)

    captured = bytearray()
    events = []

    # 1. Startup: enter alternate screen and begin animating.
    resize(master, 120, 40)
    output_bytes = drain(master, 0.6, captured)
    enter_alt = ALT_SCREEN_ENTER in bytes(captured)

    # 2. Rapid operator commands while the animation is live. Each emits an
    #    observable `◇ ACTION` scrollback line if dispatched. In observe-only mode
    #    we send only non-mutating view keys, so the scripted incident still resolves
    #    and commits its report to scrollback.
    if args.observe_only:
        operator_keys = [
            (b"\t", None),          # scale cycle
            (b"\x1b[C", None),      # focus right
            (b"\x1b[D", None),      # focus left
            (b"d", None),           # jump to most distressed
            (b"2", None),           # cluster scale
            (b"3", None),           # causal-chain scale
            (b"4", None),           # service scale
            (b"1", None),           # whole-system scale
            (b"h", None),           # toggle help
            (b"h", None),           # toggle help back
            (b" ", None),           # pause
            (b" ", None),           # resume
        ]
    else:
        operator_keys = [
            (b"a", "operator acknowledge"),
            (b"n", "suspect shared ledger pressure"),
            (b"f", "operator inject"),
            (b"i", "operator isolate"),
            (b"x", "operator reroute"),
            (b"s", "operator shed-load 50%"),
            (b"r", "operator rollback"),
            (b"\t", None),          # scale cycle
            (b"\x1b[C", None),      # focus right
            (b"\x1b[D", None),      # focus left
            (b"d", None),           # jump to most distressed
            (b"2", None),           # cluster scale
            (b"3", None),           # causal-chain scale
            (b"4", None),           # service scale
            (b"1", None),           # whole-system scale
            (b"h", None),           # toggle help
            (b"h", None),           # toggle help back
            (b" ", None),           # pause
            (b" ", None),           # resume
        ]
    for key, _label in operator_keys:
        os.write(master, key)
        output_bytes += drain(master, 0.05, captured)
    output_bytes += drain(master, 0.4, captured)
    events.append("operator_command_burst")

    # 3. Resize during the cascade, then back up, while still animating.
    resize(master, 56, 24)
    os.write(master, b"a")
    output_bytes += drain(master, 0.35, captured)
    events.append("resize_small_during_cascade")
    resize(master, 200, 50)
    os.write(master, b"3")
    output_bytes += drain(master, 0.35, captured)
    events.append("resize_large")
    resize(master, 120, 40)
    os.write(master, b"1")
    output_bytes += drain(master, 0.35, captured)
    events.append("resize_restore")

    # 4. Output backpressure: stop draining for a beat while the renderer stays
    #    live, then resume. This is the scenario that historically stalled the
    #    crossterm event channel (libgibson#15); we only claim what we observe.
    if not args.no_backpressure:
        os.write(master, b"n")
        time.sleep(args.output_pause_ms / 1000.0)
        drain(master, 0.05, bytearray())  # deliberate discard of queued bytes
        events.append("output_backpressure_%dms" % args.output_pause_ms)

    # 5. Keep interacting during sustained animation, stopping as soon as the
    #    compact incident report is observed (so a rolling capture cannot evict it).
    live_deadline = time.monotonic() + args.live_seconds
    ticks = 0
    while time.monotonic() < live_deadline:
        if not args.observe_only and ticks % 8 == 0:
            os.write(master, b"n")
        output_bytes += drain(master, min(0.2, live_deadline - time.monotonic()), captured)
        ticks += 1
        if b"INCIDENT REPORT" in bytes(captured):
            break
    events.append("sustained_animation")

    # 6. Quit and confirm restoration + exit.
    os.write(master, b"q")
    output_bytes += drain(master, 0.5, captured)
    deadline = time.monotonic() + 3.0
    exit_code = None
    while time.monotonic() < deadline:
        try:
            waited, wait_status = os.waitpid(pid, os.WNOHANG)
        except ChildProcessError:
            waited, wait_status = pid, 0
        if waited:
            exit_code = os.waitstatus_to_exitcode(wait_status)
            break
        time.sleep(0.02)
    if exit_code is None:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
        exit_code = "forced-timeout"
    os.close(master)

    data = bytes(captured)
    txt = text_of(captured)
    leave_alt = ALT_SCREEN_LEAVE in data

    # Observable evidence from the captured stream.
    action_lines = re.findall(r"\u25c7\s+(?:[A-Z-]+\s+)?t=\s*\d+\s+target=[^\n]*", txt)
    observed_actions = {}
    for _key, label in operator_keys:
        if label is not None:
            observed_actions[label] = label in txt

    incident_seen = any(s in txt for s in ("INCIDENT", "CASCADE", "LOCAL FAULT", "RESTORED"))
    report_seen = "INCIDENT REPORT" in txt
    scrollback_seen = "\u25c7" in txt or "\u25c6" in txt
    # The footer is drawn at every size; sample a stable token from it.
    affordance_seen = "[F]" in txt and "inject-fault" in txt
    exit_seen = "[Q]uit" in txt or "Quit" in txt

    result = {
        "argv": argv,
        "terminal_sizes": [[120, 40], [56, 24], [200, 50], [120, 40]],
        "output_drain_pause_ms_after_resize": 0
        if args.no_backpressure
        else args.output_pause_ms,
        "captured_output_bytes": output_bytes,
        "events_exercised": events,
        "alt_screen_enter_seen": enter_alt,
        "alt_screen_leave_seen": leave_alt,
        "cursor_hide_seen": CURSOR_HIDE in data,
        "cursor_show_seen": CURSOR_SHOW in data,
        "incident_state_rendered": incident_seen,
        "compact_incident_report_committed": report_seen,
        "scrollback_action_lines_seen": len(action_lines),
        "operator_actions_observed": observed_actions,
        "operator_affordance_in_footer": affordance_seen,
        "exit_affordance_in_footer": exit_seen,
        "music_enabled": args.music,
        "observe_only": args.observe_only,
        "exit_code": exit_code,
        "live_seconds_after_input_stress": args.live_seconds,
        "claim_boundary": (
            "PTY receipt evidence covers observable action side-effects, liveness, "
            "resize survival and terminal restoration. The app emits no per-key "
            "input receipt, so exact one-to-one event delivery is NOT verified; "
            "a single passing probe does not establish that libgibson#15 is fixed."
        ),
    }
    print(json.dumps(result, indent=2, sort_keys=True))

    delivered = sum(1 for v in observed_actions.values() if v)
    common_ok = (
        isinstance(exit_code, int)
        and exit_code == 0
        and enter_alt
        and leave_alt
        and incident_seen
        and affordance_seen
        and exit_seen
    )
    if args.observe_only:
        ok = common_ok and report_seen
    else:
        ok = common_ok and delivered >= 5
    return 0 if ok else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=os.path.abspath("target/release/cathedral"))
    parser.add_argument("--output-pause-ms", type=int, default=120)
    parser.add_argument("--no-backpressure", action="store_true")
    parser.add_argument("--music", action="store_true", help="run with live music enabled")
    parser.add_argument(
        "--observe-only",
        action="store_true",
        help="send only non-mutating view keys and wait for the incident report to commit",
    )
    parser.add_argument("--fps", type=int, default=30, help="interactive render cadence")
    parser.add_argument("--live-seconds", type=float, default=3.0)
    args = parser.parse_args()
    if args.output_pause_ms < 0 or args.output_pause_ms > 5000:
        parser.error("--output-pause-ms must be in 0..=5000")
    if args.live_seconds < 0 or args.live_seconds > 3600:
        parser.error("--live-seconds must be in 0..=3600")
    if not os.path.exists(args.binary):
        parser.error("binary not found: %s" % args.binary)
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
