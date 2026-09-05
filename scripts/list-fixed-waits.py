#!/usr/bin/env python3
"""Every fixed wait of 200 ms or more in test code, longest first.

Resolves `from_millis`/`from_secs` behind any type prefix and named `const`s
declared in the same file: a grep on call syntax misses both, and twice produced
a wrong count for ADR 0064 step 5. Writes the inventory that ADR cites --
`python3 scripts/list-fixed-waits.py > docs/adr/0064-fixed-waits.txt`.
"""
import re, subprocess, sys

files = subprocess.run(["git","ls-files","crates/*.rs"],capture_output=True,text=True).stdout.split()
DUR = re.compile(r'from_(millis|secs)\(([0-9_]+)\)')
SLEEP = re.compile(r'sleep\(([^)]*(?:\([^)]*\))?[^)]*)\)')
rows = []
for f in files:
    src = open(f).read().split("\n")
    if "/tests/" in f:
        start = 0
    else:
        start = None
        for i in range(len(src)-1):
            if src[i].strip() == "#[cfg(test)]" and src[i+1].startswith("mod tests"):
                start = i+1; break
        if start is None:
            continue
    # Constants defined anywhere in the file, so `sleep(NAME)` resolves.
    consts = {}
    for line in src:
        m = re.search(r'const (\w+): Duration = [\w:]*Duration::' + DUR.pattern, line)
        if m:
            unit, n = m.group(2), int(m.group(3).replace("_",""))
            consts[m.group(1)] = n * (1000 if unit == "secs" else 1)
    for i in range(start, len(src)):
        line = src[i]
        if "sleep(" not in line or line.lstrip().startswith(("//","///","//!")):
            continue
        m = DUR.search(line)
        if m:
            ms = int(m.group(2).replace("_","")) * (1000 if m.group(1)=="secs" else 1)
        else:
            name = SLEEP.search(line)
            key = name.group(1).strip() if name else ""
            if key not in consts:
                continue
            ms = consts[key]
        if ms >= 200:
            rows.append((ms, f, i+1, line.strip()))
rows.sort(key=lambda r: -r[0])
print(f"{len(rows)} waits, {sum(r[0] for r in rows)/1000:.1f} s in total")
for ms, f, ln, line in rows:
    print(f"{ms:6d} ms  {f}:{ln}")
