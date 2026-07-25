#!/usr/bin/env python3
"""Parse cargo-llvm-cov JSON and print uncovered lines + functions per file."""
import json
import sys
from collections import defaultdict

SRC_PREFIX = "apps/api/src/"
COVERAGE_FILE = "coverage.json"


def demangle(name: str) -> str:
    """Best-effort demangling of Rust symbol names."""
    import subprocess
    try:
        result = subprocess.run(
            ["rustfilt", name], capture_output=True, text=True, timeout=5
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    # Fallback: strip common Rust prefix patterns for readability
    s = name
    for prefix in ["_ZN", "_R"]:
        if s.startswith(prefix):
            # Just return as-is if we can't demangle
            break
    return s


def shorten_path(path: str) -> str:
    """Strip workspace prefix to get relative path."""
    if SRC_PREFIX in path:
        return path[path.index(SRC_PREFIX):]
    # For test files, strip up to 'tests/'
    idx = path.find("tests/")
    if idx >= 0:
        return path[idx:]
    return path.split("/")[-1]


def main():
    coverage_path = COVERAGE_FILE
    if len(sys.argv) > 1:
        coverage_path = sys.argv[1]

    with open(coverage_path) as f:
        data = json.load(f)

    d = data["data"][0]
    totals = d["totals"]

    # ── Overall summary ──
    print("=" * 80)
    print("OVERALL COVERAGE")
    print("=" * 80)
    for kind in ["lines", "functions"]:
        t = totals[kind]
        print(f"  {kind.capitalize():>12}: {t['covered']:>4}/{t['count']:<4}  ({t['percent']:.1f}%)")
    print()

    # ── Build per-file data from files[].summary ──
    print("=" * 80)
    print("PER-FILE COVERAGE")
    print("=" * 80)
    header = f"{'File':<55} {'Lines':>12} {'Funcs':>12}"
    print(header)
    print("-" * 80)

    file_summaries = {}
    for fe in d["files"]:
        fname = shorten_path(fe["filename"])
        s = fe["summary"]
        lc = s["lines"]
        fc = s["functions"]
        file_summaries[fname] = {
            "lines_covered": lc["covered"],
            "lines_total": lc["count"],
            "funcs_covered": fc["covered"],
            "funcs_total": fc["count"],
        }

    for fname in sorted(file_summaries.keys()):
        fs = file_summaries[fname]
        if fs["lines_total"] == 0 and fs["funcs_total"] == 0:
            continue
        lines_str = f"{fs['lines_covered']:>4}/{fs['lines_total']:<4} {fs['lines_covered']/fs['lines_total']*100 if fs['lines_total'] else 100:>5.1f}%"
        funcs_str = f"{fs['funcs_covered']:>4}/{fs['funcs_total']:<4} {fs['funcs_covered']/fs['funcs_total']*100 if fs['funcs_total'] else 100:>5.1f}%"
        print(f"  {fname:<53} {lines_str:>12} {funcs_str:>12}")

    print()

    # ── Uncovered lines per file (from segments) ──
    print("=" * 80)
    print("UNCOVERED LINES BY FILE")
    print("=" * 80)

    has_uncov_lines = False
    for fe in d["files"]:
        fname = shorten_path(fe["filename"])
        # Only show src/ files
        if "src/" not in fname:
            continue

        # Group segments by line, track if any segment was hit
        line_hits = {}  # line -> bool (was any segment on this line hit?)
        for seg in fe.get("segments", []):
            line = seg[0]
            executed = seg[3]
            if line not in line_hits:
                line_hits[line] = False
            if executed:
                line_hits[line] = True

        uncov_lines = sorted(l for l, hit in line_hits.items() if not hit)
        if not uncov_lines:
            continue

        has_uncov_lines = True
        print(f"\n  {fname}  ({len(uncov_lines)} uncovered lines)")
        # Print lines in compact groups (e.g. 57-62, 80, 95-99)
        groups = []
        start = prev = None
        for ln in uncov_lines:
            if start is None:
                start = prev = ln
            elif ln == prev + 1:
                prev = ln
            else:
                groups.append((start, prev))
                start = prev = ln
        if start is not None:
            groups.append((start, prev))

        parts = []
        for s, e in groups:
            if s == e:
                parts.append(str(s))
            else:
                parts.append(f"{s}-{e}")
        # Print in lines of ~80 chars
        line_str = ", ".join(parts)
        while len(line_str) > 76:
            cut = line_str[:76].rfind(", ")
            if cut < 0:
                cut = 76
            print(f"    {line_str[:cut]}")
            line_str = line_str[cut + 2:]
        if line_str:
            print(f"    {line_str}")

    if not has_uncov_lines:
        print("  (none)")

    print()

    # ── Uncovered functions by file ──
    print("=" * 80)
    print("UNCOVERED FUNCTIONS BY FILE")
    print("=" * 80)

    uncov_funcs_by_file = defaultdict(list)
    for func in d["functions"]:
        if func["count"] == 0:
            for fpath in func["filenames"]:
                short = shorten_path(fpath)
                uncov_funcs_by_file[short].append(func["name"])

    has_uncov_funcs = False
    for fname in sorted(uncov_funcs_by_file.keys()):
        if "src/" not in fname:
            continue
        funcs = uncov_funcs_by_file[fname]
        has_uncov_funcs = True
        print(f"\n  {fname}  ({len(funcs)} uncovered)")
        for name in funcs:
            print(f"    - {demangle(name)}")

    if not has_uncov_funcs:
        print("  (none)")

    print()


if __name__ == "__main__":
    main()
