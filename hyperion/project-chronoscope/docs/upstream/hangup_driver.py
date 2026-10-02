#!/usr/bin/env python3
"""Run a TUI binary in a PTY, close the PTY master, report what the process is doing afterwards.

    hangup_driver.py BINARY            # SIGHUP ignored (like nohup / a harness that survives HUP)
    hangup_driver.py BINARY --default  # SIGHUP left at its default (the control)
"""
import os, pty, signal, subprocess, sys, time

exe = sys.argv[1]
default_hup = "--default" in sys.argv[2:]
pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    if not default_hup:
        signal.signal(signal.SIGHUP, signal.SIG_IGN)
    os.execv(exe, [exe])
time.sleep(1.0)
os.close(fd)          # the terminal window disappears
time.sleep(2.0)
out = subprocess.run(["ps", "-o", "pid,stat,pcpu,etime", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
print(out)
try:
    done, status = os.waitpid(pid, os.WNOHANG)
    if done:
        print("exited:", "signal %d" % os.WTERMSIG(status) if os.WIFSIGNALED(status) else "code %d" % os.WEXITSTATUS(status))
except ChildProcessError:
    pass
try:
    os.kill(pid, signal.SIGKILL)
except ProcessLookupError:
    pass
