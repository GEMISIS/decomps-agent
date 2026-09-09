#!/usr/bin/env python3
"""Compare two smoke tables: print one line per check whose FAIL got WORSE by count (before -> after).
   screen: tile rows differ + attribute rows differ (+100 if the palette differs); sprites: records differ;
   motion: frames differ; sound: |rebuilt writes - original writes|; runaway: cycles/frame.
   Used by remedy_loop.sh's guard: a kept round must not make any failing check worse, even if its
   pass/fail state and first-divergence frame are unchanged."""
import re, sys
def sev(line):
    m = re.match(r'FAIL\s+(.*?)(?: \(frame \d+\))?:', line)
    if not m: return None, None
    name = m.group(1).strip(); body = line
    if name.startswith('motion'):
        m2 = re.search(r'differ on (\d+) of', body); return name, int(m2.group(1)) if m2 else None
    if name.startswith('sound'):
        m2 = re.search(r'original (\d+) writes, rebuilt (\d+)', body); return name, abs(int(m2.group(1)) - int(m2.group(2))) if m2 else None
    if 'runaway' in name:
        m2 = re.search(r'(\d+) cycles/frame', body); return 'runaway routine', int(m2.group(1)) if m2 else None
    if 'tile rows differ' in body:
        rows = int(re.search(r'(\d+) of 30 tile rows differ', body).group(1))
        attr = re.search(r'(\d+) of 8 attribute rows differ', body); attr = int(attr.group(1)) if attr else 0
        return name, rows + attr + (100 if 'palette DIFFERS' in body else 0)
    m2 = re.search(r'(\d+) records differ', body)
    if m2: return name, int(m2.group(1))
    return name, None
def table(path):
    out = {}
    for l in open(path, errors='replace'):
        if l.startswith('FAIL'):
            n, s = sev(l.rstrip())
            if n and s is not None: out[n] = s
    return out
b, a = table(sys.argv[1]), table(sys.argv[2])
for n, sb in b.items():
    sa = a.get(n)
    if sa is not None and sa > sb: print(f"{n}(severity {sb} -> {sa})")
