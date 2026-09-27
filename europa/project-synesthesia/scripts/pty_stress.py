#!/usr/bin/env python3
"""Bounded PTY input/resize stress probe for Synesthesia's LibGibson consumer path."""

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


MAX_CAPTURE = 2 * 1024 * 1024


def resize(master_fd, cols, rows):
    fcntl.ioctl(master_fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def drain(master_fd, seconds, capture):
    deadline = time.monotonic() + seconds
    total = 0
    while time.monotonic() < deadline:
        ready, _, _ = select.select([master_fd], [], [], min(0.03, deadline - time.monotonic()))
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
    return re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data)


def app_receipts(data):
    values = re.findall(rb"application input events received:\s*(\d+)", strip_ansi(data))
    return int(values[-1]) if values else None


def app_key_receipts(data):
    values = re.findall(rb"application key events received:\s*(\d+)", strip_ansi(data))
    return int(values[-1]) if values else None


def app_resize_receipts(data):
    values = re.findall(rb"application resize events received:\s*(\d+)", strip_ansi(data))
    return int(values[-1]) if values else None


def profile_metrics(data):
    clean = strip_ansi(data).decode("utf-8", "replace")
    patterns = {
        "frames": r"frames:\s*(\d+)\s+target:\s*(\d+) FPS\s+late.*?:\s*(\d+)\s+scheduler-skipped:\s*(\d+)",
        "frame_ms": r"frame: mean\s*([\d.]+) ms\s*p95\s*([\d.]+) ms\s*max\s*([\d.]+) ms",
        "dsp_surface_renderer_ms": r"DSP:\s*([\d.]+) ms/frame\s*custom Surface:\s*([\d.]+) ms/frame \(max\s*([\d.]+)\)\s*LibGibson render:\s*([\d.]+) ms/frame",
        "exact_changed_and_affected_cells": r"exact changed cells:\s*([\d.]+)/frame\s*affected footprint:\s*([\d.]+)/frame",
        "bytes_and_history": r"emitted frame bytes:\s*([\d.]+)/frame\s*max\s*(\d+)\s*retained spectra:\s*(\d+)/(\d+)",
        "rss_kib": r"RSS: start\s*(\d+) -> warm\(64\)\s*(\d+) -> end\s*(\d+) KiB\s*peak\s*(\d+) KiB",
        "application_deadline_misses": r"application deadline misses:\s*(\d+)",
        "input_backend_errors": r"input backend errors:\s*(\d+)",
        "audio_backend_errors": r"audio backend stream errors:\s*(\d+)",
        "audio_callback_timing": r"audio callback timing: callbacks\s*(\d+) nonzero\s*(\d+) peak\s*([\d.]+) mean\s*([\d.]+) us max\s*([\d.]+) us over-budget\s*(\d+) backend errors\s*(\d+)",
    }
    result = {}
    for name, pattern in patterns.items():
        match = re.search(pattern, clean)
        if match:
            values = [float(value) if "." in value else int(value) for value in match.groups()]
            result[name] = values[0] if len(values) == 1 else values
    return result


def run(args):
    argv = [args.binary, "--demo", "--profile", "--fps=30"]
    if not args.audio:
        argv.append("--silent")
    pid, master = pty.fork()
    if pid == 0:
        env = os.environ.copy()
        env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor"})
        os.execve(args.binary, argv, env)
        os._exit(127)

    captured = bytearray()
    resize(master, 120, 40)
    output_bytes = drain(master, 0.35, captured)
    audio_active_seen = b"audio active" in captured

    # Editing, focus, both arrow paths, repeat, and modal numeric entry.
    controls = b"\x1b[C" * 64 + b"\t\x1b[Z" + b"f4400\r" + b"e0.02\r" + b"wms[]\rp"
    os.write(master, controls)
    output_bytes += drain(master, 0.30, captured)
    control_keys_expected = 64 + 2 + 6 + 6 + 7

    # Stop draining terminal output while the renderer remains live. The app's
    # independent reader should still queue the resize and coincident key.
    if not args.no_backpressure:
        resize(master, 56, 24)
        os.write(master, b"r")
        time.sleep(args.output_pause_ms / 1_000.0)
    else:
        resize(master, 56, 24)
        os.write(master, b"r")
    resize(master, 120, 40)
    output_bytes += drain(master, 0.35, captured)

    # A separate key checks recovery after the pause and resize sequence.
    os.write(master, b"z")
    output_bytes += drain(master, 0.25, captured)

    live_deadline = time.monotonic() + args.live_seconds
    while time.monotonic() < live_deadline:
        output_bytes += drain(master, min(0.25, live_deadline - time.monotonic()), captured)

    os.write(master, b"q")
    output_bytes += drain(master, 0.25, captured)
    deadline = time.monotonic() + 2.0
    exit_code = None
    while time.monotonic() < deadline:
        try:
            waited, wait_status = os.waitpid(pid, os.WNOHANG)
        except ChildProcessError:
            waited = pid
            wait_status = 0
        if waited:
            exit_code = os.waitstatus_to_exitcode(wait_status)
            break
        time.sleep(0.02)
    if exit_code is None:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
        exit_code = "forced-timeout"
    os.close(master)

    result = {
        "argv": argv,
        "terminal_sizes": [[120, 40], [56, 24]],
        "output_drain_pause_ms_after_resize": 0
        if args.no_backpressure
        else args.output_pause_ms,
        "captured_output_bytes": output_bytes,
        "keyboard_events_in_control_burst": control_keys_expected,
        "expected_total_keyboard_events": control_keys_expected + 3,
        "shutdown_application_input_receipts": app_receipts(captured),
        "shutdown_application_key_receipts": app_key_receipts(captured),
        "shutdown_application_resize_receipts": app_resize_receipts(captured),
        "live_profile": profile_metrics(captured),
        "all_expected_keyboard_events_received": app_key_receipts(captured)
        == control_keys_expected + 3,
        "exit_code": exit_code,
        "audio_requested": args.audio,
        # The rolling capture can evict the startup status during a long run.
        "audio_active_status_seen": audio_active_seen or b"audio active" in captured,
        "live_seconds_after_input_stress": args.live_seconds,
        "app_input_receipt_counter": "AppModel keyboard/event counters printed at shutdown",
        "claim_boundary": "PTY receipt evidence covers application-dispatched events; the run does not localize syscall-to-Crossterm latency.",
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    final_keys = app_key_receipts(captured)
    expected_key_receipts = control_keys_expected + 3
    return 0 if exit_code == 0 and final_keys == expected_key_receipts else 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=os.path.abspath("target/release/synesthesia"))
    parser.add_argument("--output-pause-ms", type=int, default=90)
    parser.add_argument("--no-backpressure", action="store_true")
    parser.add_argument("--audio", action="store_true")
    parser.add_argument("--live-seconds", type=float, default=1.0)
    args = parser.parse_args()
    if args.output_pause_ms < 0 or args.output_pause_ms > 1000:
        parser.error("--output-pause-ms must be in 0..=1000")
    if args.live_seconds < 0 or args.live_seconds > 3600:
        parser.error("--live-seconds must be in 0..=3600")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
