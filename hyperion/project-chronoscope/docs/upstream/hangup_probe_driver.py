#!/usr/bin/env python3
"""usage: hangup_probe_driver.py BINARY MODE   (MODE = poll | render | write; see hangup_probe.rs)"""
import pty, os, time, signal, subprocess, sys
exe, mode = sys.argv[1], sys.argv[2]
LOG = os.path.join(os.environ.get("TMPDIR", "/tmp"), "hangup_probe.log")
try: os.remove(LOG)
except FileNotFoundError: pass
pid, fd = pty.fork()
if pid == 0:
    os.environ.update(TERM="xterm-256color", PROBE_LOG=LOG)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)   # a process that survives its terminal (nohup-style)
    os.execv(exe, [exe, mode])
time.sleep(1.0)
os.close(fd)
time.sleep(2.0)
r = subprocess.run(["ps","-o","pid,stat,pcpu","-p",str(pid)],capture_output=True,text=True).stdout.strip().split("\n")[-1]
lines = open(LOG).read().strip().split("\n") if os.path.exists(LOG) else []
print(f"{mode:7} -> {r} | log lines {len(lines)} | tail: {lines[-3:]}")
try: os.kill(pid, signal.SIGKILL)
except ProcessLookupError: pass
