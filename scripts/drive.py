#!/usr/bin/env python3
"""Drive the TUI under a pty and print what it rendered.

The app needs a real terminal, so piping stdin doesn't work. This forks
a pty, sends timed keystrokes, and captures the frames.

    cargo build
    python3 scripts/drive.py --cwd /tmp/scratch 1.0='hi there<enter>' 6.0='<esc>'

Each argument is DELAY=KEYS, where DELAY is seconds since launch. Keys
take <enter>, <esc>, <ctrl-c>, <ctrl-j>, <page-up>, <page-down>, <home>,
<end>, <scroll-up>, <scroll-down>, <tab>, <click:ROW,COL> (0-indexed,
a left click at that terminal row and column), and literal text.

Output is the raw byte stream. --plain turns every escape sequence into
a space, which is what makes the rendered text greppable: ratatui moves
the cursor between words, so "hi there" is never contiguous on the wire.

Frames accumulate, so a word repeated in the output is a redraw, not a
duplicate event. Read the log file for what was actually committed.
"""

import argparse
import fcntl
import os
import pty
import re
import select
import struct
import sys
import termios
import time

KEYS = {
    "<enter>": b"\r",
    "<esc>": b"\x1b",
    "<ctrl-c>": b"\x03",
    "<ctrl-j>": b"\x0a",
    "<tab>": b"\t",
    "<backspace>": b"\x7f",
    "<page-up>": b"\x1b[5~",
    "<page-down>": b"\x1b[6~",
    "<home>": b"\x1b[H",
    "<end>": b"\x1b[F",
    "<scroll-up>": b"\x1b[<64;1;1M",
    "<scroll-down>": b"\x1b[<65;1;1M",
}
ESCAPE = re.compile(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b[()][A-Z0-9]|\x1b.")
CLICK = re.compile(r"<click:(\d+),(\d+)>")


def encode(keys):
    # SGR mouse reporting is 1-indexed on the wire, hence the +1s.
    keys = CLICK.sub(
        lambda m: f"\x1b[<0;{int(m.group(2)) + 1};{int(m.group(1)) + 1}M", keys
    )
    for name, code in KEYS.items():
        keys = keys.replace(name, code.decode("latin-1"))
    return keys.encode("latin-1")


def drive(binary, cwd, rows, cols, script, total):
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(cwd)
        os.environ["TERM"] = "xterm-256color"
        os.execv(binary, [binary])

    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    out = bytearray()
    pending = sorted(script)
    start = time.time()
    while time.time() - start < total:
        while pending and time.time() - start >= pending[0][0]:
            os.write(fd, pending.pop(0)[1])
        ready, _, _ = select.select([fd], [], [], 0.05)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        out += chunk

    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass
    return bytes(out).decode("utf-8", "replace")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("keys", nargs="*", metavar="DELAY=KEYS")
    parser.add_argument("--bin", default="target/debug/percept")
    parser.add_argument("--cwd", default=".", help="the log lives here")
    parser.add_argument("--total", type=float, help="seconds to run, default last delay + 1")
    parser.add_argument("--rows", type=int, default=24)
    parser.add_argument("--cols", type=int, default=80)
    parser.add_argument("--plain", action="store_true", help="strip escapes")
    args = parser.parse_args()

    script = []
    for entry in args.keys:
        delay, _, keys = entry.partition("=")
        script.append((float(delay), encode(keys)))

    total = args.total or (max((d for d, _ in script), default=0.0) + 1.0)
    output = drive(
        os.path.abspath(args.bin), args.cwd, args.rows, args.cols, script, total
    )
    if args.plain:
        output = re.sub(r" +", " ", ESCAPE.sub(" ", output))
    sys.stdout.write(output)


if __name__ == "__main__":
    main()
