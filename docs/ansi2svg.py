#!/usr/bin/env python3
"""Render ANSI-coloured terminal output as a standalone SVG.

Reads ANSI text on stdin, writes SVG on stdout. Used to regenerate demo.svg:

    lsports 3000 --color always | python3 docs/ansi2svg.py > docs/demo.svg

Capture interactive output (prompts) through a pty, or the tool will correctly
detect a pipe and drop its colour.
"""
import re
import sys
from html import escape

BG = "#0d1117"
FG = "#c9d1d9"
DIM = "#8b949e"
BOLD_FG = "#e6edf3"
COLORS = {"36": "#79c0ff", "33": "#d29922", "32": "#3fb950"}

FONT = ("ui-monospace,SFMono-Regular,'SF Mono',Menlo,Consolas,"
        "'Liberation Mono',monospace")
SIZE = 14
# Real glyph advance for a 14px monospace face. Undershooting this clips the
# card; textLength below pins the grid for renderers that honour it.
CHAR_W = 8.8
LINE_H = 22
PAD_X = 18
PAD_Y = 20
TOP_BAR = 34

TOKEN = re.compile(r"\x1b\[([0-9;]*)m")


def spans(line):
    """Yield (text, bold, dim, colour) runs for one line."""
    out, pos = [], 0
    bold = dim = False
    color = None
    for m in TOKEN.finditer(line):
        if m.start() > pos:
            out.append((line[pos:m.start()], bold, dim, color))
        for code in (m.group(1) or "0").split(";"):
            if code in ("", "0"):
                bold = dim = False
                color = None
            elif code == "1":
                bold = True
            elif code == "2":
                dim = True
            elif code in COLORS:
                color = COLORS[code]
        pos = m.end()
    if pos < len(line):
        out.append((line[pos:], bold, dim, color))
    return out


def main():
    raw = sys.stdin.read().replace("\r\n", "\n").rstrip("\n")
    lines = [l.rstrip() for l in raw.split("\n")]

    visible = [len(TOKEN.sub("", l)) for l in lines] or [0]
    width = int(PAD_X * 2 + max(visible) * CHAR_W) + 16
    height = int(TOP_BAR + PAD_Y + len(lines) * LINE_H + 10)

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" '
        f'height="{height}" viewBox="0 0 {width} {height}" '
        f'font-family="{FONT}" font-size="{SIZE}">',
        f'<rect width="{width}" height="{height}" rx="10" fill="{BG}"/>',
        f'<rect width="{width}" height="{TOP_BAR}" rx="10" fill="#161b22"/>',
        f'<rect y="{TOP_BAR - 10}" width="{width}" height="10" fill="#161b22"/>',
    ]
    for i, c in enumerate(("#ff5f57", "#febc2e", "#28c840")):
        parts.append(f'<circle cx="{18 + i * 18}" cy="17" r="6" fill="{c}"/>')

    # textLength pins each line to an exact character grid, so alignment and the
    # card width hold regardless of which monospace font the viewer resolves.
    for row, line in enumerate(lines):
        chars = len(TOKEN.sub("", line))
        if chars == 0:
            continue
        y = TOP_BAR + PAD_Y + row * LINE_H
        parts.append(
            f'<text x="{PAD_X}" y="{y}" xml:space="preserve" '
            f'textLength="{chars * CHAR_W:.1f}" lengthAdjust="spacing">'
        )
        for text, bold, dim, color in spans(line):
            if not text:
                continue
            fill = color or (BOLD_FG if bold else DIM if dim else FG)
            weight = ' font-weight="600"' if bold else ""
            parts.append(f'<tspan fill="{fill}"{weight}>{escape(text)}</tspan>')
        parts.append("</text>")

    parts.append("</svg>")
    sys.stdout.write("\n".join(parts) + "\n")


if __name__ == "__main__":
    main()
