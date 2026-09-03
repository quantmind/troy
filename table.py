#!/usr/bin/env python3
"""Run the bpt-dec bench suite and print per-operation timings."""

import re
import subprocess
import sys

UNITS = {"ps": 1e-3, "ns": 1.0, "µs": 1e3, "us": 1e3, "ms": 1e6, "s": 1e9}
ROWS = ["cmp", "add", "mul", "div", "round_dp", "round_to_step", "floor", "ceil", "to_f64", "from_f64", "parse", "format"]
COLUMNS = ["f64", "Dec", "rust_decimal", "fastnum"]


def run() -> str:
    command = ["cargo", "bench", "-p", "bpt-dec", "--bench", "arithmetic"]
    command += sys.argv[1:]
    print(" ".join(command), file=sys.stderr)
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        sys.exit(result.returncode)
    return result.stdout


def parse(output: str) -> dict[tuple[str, str], float]:
    # criterion wraps a long id onto its own line, so track the last id seen
    timings: dict[tuple[str, str], float] = {}
    identifier = None
    for line in output.splitlines():
        text = line.strip()
        head = re.match(r"^(\S+)/(\S+)/(\d+)\s*(time:.*)?$", text)
        if head:
            identifier = (head.group(1), head.group(2), int(head.group(3)))
            rest = head.group(4)
        elif identifier and text.startswith("time:"):
            rest = text
        else:
            continue
        if not rest:
            continue
        # [low <unit> median <unit> high <unit>] — take the median
        window = re.search(r"\[\S+ \S+ ([\d.]+) (\S+) \S+ \S+\]", rest)
        if not window:
            continue
        group, implementation, count = identifier
        nanos = float(window.group(1)) * UNITS[window.group(2)]
        timings[(group, implementation)] = nanos / count
        identifier = None
    return timings


def render(timings: dict[tuple[str, str], float]) -> None:
    width = max(len(row) for row in ROWS + ["op"]) + 2
    header = "op".ljust(width) + "".join(name.rjust(14) for name in COLUMNS)
    print(f"\nns per operation\n\n{header}\n{'-' * len(header)}")
    for row in ROWS:
        if not any((row, column) in timings for column in COLUMNS):
            continue
        best = min(
            (timings[(row, c)] for c in ("Dec", "rust_decimal", "fastnum") if (row, c) in timings),
            default=None,
        )
        cells = ""
        for column in COLUMNS:
            value = timings.get((row, column))
            if value is None:
                cells += "-".rjust(14)
            else:
                cells += f"{value:>13.2f}" + ("*" if value == best else " ")
        print(row.ljust(width) + cells)
    print("\n* fastest decimal")


if __name__ == "__main__":
    render(parse(run()))
