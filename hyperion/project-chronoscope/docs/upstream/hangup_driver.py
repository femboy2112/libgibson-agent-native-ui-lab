#!/usr/bin/env python3
"""Run a TUI binary in a PTY with SIGHUP ignored (like nohup), close the PTY master, report CPU."""
import os, pty, signal, subprocess, sys, time

exe = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    os.execv(exe, [exe])
time.sleep(1.0)
os.close(fd)          # the terminal window disappears
time.sleep(2.0)
print(subprocess.run(["ps", "-o", "pid,stat,pcpu,etime", "-p", str(pid)], capture_output=True, text=True).stdout.strip())
os.kill(pid, signal.SIGKILL)
