#!/usr/bin/python3
"""Stop a macOS local service only after checking its project and command.

Run from the saved service project directory. --check validates without signals.
Only the listener and its same-user, same-project descendants are targeted.
"""

import argparse
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import time


def run(args):
    return subprocess.run(args, capture_output=True, text=True, timeout=2)


def listeners(port):
    result = run([
        "/usr/sbin/lsof", "-nP", "-iTCP:" + str(port),
        "-sTCP:LISTEN", "-Fp",
    ])
    if result.returncode not in (0, 1) or result.stderr.strip():
        raise RuntimeError("Cannot inspect listening processes: " + result.stderr.strip())
    return {int(line[1:]) for line in result.stdout.splitlines() if line.startswith("p")}


def identity(pid):
    result = run([
        "/bin/ps", "-p", str(pid), "-o", "uid=,lstart=,command=",
    ])
    parts = result.stdout.strip().split(None, 6)
    if len(parts) != 7:
        return None
    return int(parts[0]), " ".join(parts[1:6]), parts[6]


def project_of(pid):
    result = run(["/usr/sbin/lsof", "-a", "-p", str(pid), "-d", "cwd", "-Fn"])
    paths = [line[1:] for line in result.stdout.splitlines() if line.startswith("n")]
    return Path(paths[0]).resolve() if len(paths) == 1 else None


def matches(command, program, script):
    words = shlex.split(command)
    if len(words) < 2 or Path(words[1]).name != script:
        return False
    executable = Path(words[0]).name
    if program == "python":
        return re.fullmatch(r"[Pp]ython(?:\d+(?:\.\d+)*)?", executable) is not None
    return executable == program


def descendants(roots):
    result = run(["/bin/ps", "-axo", "pid=,ppid="])
    if result.returncode:
        raise RuntimeError("Cannot inspect child processes")
    tree = [tuple(map(int, line.split())) for line in result.stdout.splitlines()]
    found = set(roots)
    while True:
        children = {pid for pid, parent in tree if parent in found} - found
        if not children:
            return found
        found.update(children)


def stop(port, program, script, check=False):
    project = Path.cwd().resolve()
    roots = listeners(port)
    if not roots:
        print("Already stopped (no listener on port " + str(port) + ")")
        return
    targets = {}
    for pid in sorted(roots):
        current = identity(pid)
        if (current is None or current[0] != os.getuid()
                or project_of(pid) != project
                or not matches(current[2], program, script)):
            raise RuntimeError("Port %s belongs to a different process; refusing to stop PID %s"
                               % (port, pid))
        targets[pid] = current
    for pid in descendants(roots) - roots:
        current = identity(pid)
        if current and current[0] == os.getuid() and project_of(pid) == project:
            targets[pid] = current
    print("Verified project %s; listener(s) %s; target(s) %s"
          % (project, sorted(roots), sorted(targets)), flush=True)
    if check:
        return

    def send(pid, sig):
        # Re-check identity immediately before each signal, including escalation.
        if identity(pid) == targets[pid] and project_of(pid) == project:
            try:
                os.kill(pid, sig)
            except ProcessLookupError:
                pass

    for pid in targets:
        send(pid, signal.SIGTERM)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        alive = [pid for pid in targets if identity(pid) == targets[pid]]
        if not alive:
            break
        time.sleep(0.1)
    else:
        for pid in alive:
            send(pid, signal.SIGKILL)
    if listeners(port):
        # A new or unrelated listener is never added to the signal targets.
        raise RuntimeError("Port %s is still occupied; restart cancelled" % port)
    print("Stopped", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port", type=int, choices=range(1, 65536), metavar="PORT")
    parser.add_argument("program", choices=["node", "python"])
    parser.add_argument("script")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stop(args.port, args.program, args.script, args.check)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.TimeoutExpired, ValueError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
