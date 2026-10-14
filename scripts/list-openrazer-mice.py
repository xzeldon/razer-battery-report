#!/usr/bin/env python3
"""List the openrazer mice that report battery, with the values our device catalog needs.

Reads a local openrazer checkout and prints one row per USB product id: a table for people,
or JSON with --json for tools. JSON field names match the catalog entry fields. A value that
cannot be parsed is printed as empty, reported on stderr, and makes the exit status 1, so a
change in openrazer's source fails loudly instead of producing a wrong catalog.

    python scripts/list-openrazer-mice.py <openrazer-checkout> [--json] [--pid 0x00BF]
"""

import argparse
import json
import re
import sys
from pathlib import Path

MOUSE_SOURCE = "driver/razermouse_driver.c"
MOUSE_HEADER = "driver/razermouse_driver.h"
DAEMON_HARDWARE = "daemon/openrazer_daemon/hardware"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("openrazer", type=Path, help="path to an openrazer checkout")
    parser.add_argument("--json", action="store_true", help="print JSON instead of a table")
    parser.add_argument("--pid", type=lambda s: int(s, 16), help="only this product id, e.g. 0x00BF")
    args = parser.parse_args()

    if not (args.openrazer / MOUSE_SOURCE).is_file():
        sys.exit(f"error: {args.openrazer} is not an openrazer checkout (no {MOUSE_SOURCE})")

    mice = list_mice(args.openrazer)
    if args.pid is not None:
        mice = [m for m in mice if m["pid"] == args.pid]
        if not mice:
            sys.exit(f"error: 0x{args.pid:04X} is not a battery mouse in openrazer")

    problems = [
        f"0x{m['pid']:04X} {m['define']}: cannot parse {field}"
        for m in mice
        for field in ("tid", "interface", "wait_us", "charging", "serial")
        if m[field] is None
    ]

    if args.json:
        print(json.dumps(mice, indent=2))
    else:
        print_table(mice)
    for problem in problems:
        print(f"warning: {problem}", file=sys.stderr)
    sys.exit(1 if problems else 0)


def list_mice(root):
    """Return one dict per battery-capable mouse product id, sorted by pid."""
    source = (root / MOUSE_SOURCE).read_text(errors="replace")
    header = (root / MOUSE_HEADER).read_text(errors="replace")
    pids = {m[1]: int(m[2], 16) for m in re.finditer(r"#define\s+(USB_DEVICE_ID_RAZER_\w+)\s+0x([0-9A-Fa-f]+)", header)}
    waits = {m[1]: int(m[2]) for m in re.finditer(r"#define\s+(RAZER_\w+_WAIT_US)\s+(\d+)", header)}
    names = daemon_names(root)

    attrs = {}
    for labels, block in case_groups(function_body(source, r"static int razer_mouse_probe\(")):
        created = set(re.findall(r"CREATE_DEVICE_FILE\([^,]+,\s*&dev_attr_(\w+)\)", block))
        for label in labels:
            attrs.setdefault(label, set()).update(created)

    level = tid_by_case(function_body(source, r"static ssize_t razer_attr_read_charge_level\("))
    status = tid_by_case(function_body(source, r"static ssize_t razer_attr_read_charge_status\("))
    serial = tid_by_case(function_body(source, r"static ssize_t razer_attr_read_device_serial\("))
    report = report_params(function_body(source, r"static int razer_get_report\("), waits)

    mice = []
    for define, created in attrs.items():
        if "charge_level" not in created or define not in pids:
            continue
        pid = pids[define]
        tid = level.get(define)
        interface, wait_us = report.get(define, report.get("default", (None, None)))
        mice.append({
            "pid": pid,
            "define": define,
            "name": names.get(pid) or name_from_define(define),
            "tid": tid if isinstance(tid, int) else None,
            "interface": interface,
            "wait_us": wait_us,
            "charging": charging(created, status.get(define, status.get("default")), tid),
            "serial": "query" if isinstance(serial.get(define), int) else "unsupported",
            "serial_tid": serial[define] if isinstance(serial.get(define), int) and serial[define] != tid else None,
        })
    return sorted(mice, key=lambda m: m["pid"])


def charging(created, status_tid, level_tid):
    """'query' when openrazer asks the mouse, 'unsupported' when it never does, None if unclear."""
    if "charge_status" not in created or status_tid in ("no-query", "unsupported"):
        return "unsupported"
    return "query" if status_tid == level_tid else None


def function_body(source, signature):
    """Return the brace-balanced body of the first function whose signature matches."""
    match = re.search(signature, source)
    if not match:
        sys.exit(f"error: cannot find {signature} in openrazer; its source layout changed")
    start = source.index("{", match.end())
    depth = 0
    for i in range(start, len(source)):
        depth += {"{": 1, "}": -1}.get(source[i], 0)
        if depth == 0:
            return source[start:i + 1]
    sys.exit(f"error: unbalanced braces after {signature}")


def case_groups(body):
    """Yield (case labels, code) for each case group; a `fallthrough;` group includes the next."""
    pattern = re.compile(
        r"((?:\s*(?:case\s+\w+|default)\s*:\s*(?://[^\n]*)?)+)(.*?)(?=\n\s*(?:case\s+\w+|default)\s*:|\Z)", re.S
    )
    groups = [
        (re.findall(r"case\s+(\w+)\s*:", m[1]) + (["default"] if "default" in m[1] else []), m[2])
        for m in pattern.finditer(body)
    ]
    for i, (labels, block) in enumerate(groups):
        j = i
        while re.search(r"\bfallthrough\s*;", groups[j][1]) and j + 1 < len(groups):
            j += 1
            block += groups[j][1]
        yield labels, block


def tid_by_case(body):
    """Map each case label to its transaction id, or to 'no-query' / 'unsupported'."""
    out = {}
    for labels, block in case_groups(body):
        tids = re.findall(r"transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)", block)
        if tids:
            value = int(tids[-1], 16)
        elif "-EINVAL" in block:
            value = "unsupported"
        elif "razer_send_payload" not in block and "sysfs_emit" in block:
            value = "no-query"
        else:
            value = None
        for label in labels:
            out[label] = value
    return out


def report_params(body, waits):
    """Map each case label of razer_get_report to (USB interface, response wait in µs)."""
    out = {}
    for labels, block in case_groups(body):
        index = re.findall(r"index\s*=\s*(0x[0-9A-Fa-f]+|\d+)", block)
        wait = re.findall(r"(RAZER_\w+_WAIT_US)", block)
        value = (int(index[-1], 0) if index else 0, waits.get(wait[-1]) if wait else None)
        for label in labels:
            out[label] = value
    return out


def daemon_names(root):
    """Map pid to the display name from the daemon's class docstrings."""
    names = {}
    for path in sorted((root / DAEMON_HARDWARE).glob("*.py")):
        text = path.read_text(errors="replace")
        for m in re.finditer(r'class \w+\([^)]*\):\s*"""\s*Class for the (.+?)\s*"""(.*?)USB_PID\s*=\s*(0x[0-9A-Fa-f]+)', text, re.S):
            if "class " not in m[2]:
                names.setdefault(int(m[3], 16), m[1].strip())
    return names


def name_from_define(define):
    words = define.removeprefix("USB_DEVICE_ID_RAZER_").split("_")
    return "Razer " + " ".join(w.capitalize() for w in words)


def print_table(mice):
    columns = ("pid", "name", "tid", "interface", "wait_us", "charging", "serial")
    rows = [[cell(m, c) for c in columns] for m in mice]
    widths = [max(len(c), *(len(r[i]) for r in rows)) for i, c in enumerate(columns)]
    for row in [list(columns), *rows]:
        print("  ".join(v.ljust(w) for v, w in zip(row, widths)).rstrip())


def cell(mouse, column):
    value = mouse[column]
    if value is None:
        return "?"
    if column == "pid":
        return f"0x{value:04X}"
    if column == "tid":
        return f"0x{value:02X}"
    if column == "serial" and mouse["serial_tid"] is not None:
        return f"query (tid 0x{mouse['serial_tid']:02X})"
    return str(value)


if __name__ == "__main__":
    main()
