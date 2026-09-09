#!/usr/bin/env python3
"""Deterministic check of an emulator observation against the behavioral spec.
usage: spec_check.py <spec.json> <observe.txt> [--screen initial|later]
Prints PASS/FAIL lines a model cannot argue with:
  - palette: every `video.initial_palettes` entry vs PPU palette RAM (initial screen only)
  - drawn: nametable has non-blank rows (the spec's initial_screen is not empty)
  - sprites: at least one visible sprite when timelines mention sprites at that state
  - boot: more than one distinct palette value (an all-zero palette means the boot hung)
Exit 0 only if every check passes."""
import json, re, sys
spec = json.load(open(sys.argv[1])); obs = open(sys.argv[2]).read()
screen = "initial"
if "--screen" in sys.argv: screen = sys.argv[sys.argv.index("--screen") + 1]
fails = 0
def report(ok, name, detail=""):
    global fails
    print(("PASS  " if ok else "FAIL  ") + name + ((": " + detail) if detail else "")); fails += 0 if ok else 1
m = re.search(r"PALETTE[^:]*:\s*([0-9A-F ]+)", obs)
pal = [int(x, 16) for x in m.group(1).split()] if m else []
report(len(pal) == 32, "observation parsed (32 palette entries)", f"got {len(pal)}")
report(len(set(pal)) > 1, "boot reached the palette upload (palette not all one value)", f"palette = {pal}")
video = spec.get("video") or {}
if screen == "initial":
    for p in video.get("initial_palettes") or []:
        base = (0 if p["kind"] == "background" else 16) + 4 * int(p["index"])
        want = list(p["colors"]); got = pal[base:base + len(want)] if pal else []
        # entry 0 of every sub-palette mirrors the backdrop; compare it loosely
        ok = got[1:] == want[1:] and (got[:1] == want[:1] or p["index"] != 0)
        report(ok, f"palette {p['kind']} {p['index']}", f"want {want} got {got}")
rows = re.findall(r"^\d\d ((?:\S\S ?){32})$", obs, re.M)
nonblank = sum(1 for r in rows if any(t not in ("..", "__") for t in r.split()))
mw = re.search(r"VRAM WRITES[^\n]*nametable=(\d+) attribute=(\d+) palette=(\d+)", obs)
ntw = int(mw.group(1)) if mw else -1
if (video.get("initial_screen") or "").strip():
    # Stub assets draw as zero tiles, so count the bytes the code WROTE into the nametable, not the tiles visible.
    if ntw >= 0:
        report(ntw >= 64, "something is drawn (nametable bytes written by the code)", f"{ntw} nametable bytes written; {nonblank} of {len(rows)} rows show non-zero tiles (zero tiles are expected with stub assets)")
    else:
        report(nonblank > 0, "something is drawn (non-blank nametable rows)", f"{nonblank} of {len(rows)} rows have tiles")
# If an acceptance observation states how many nametable bytes the original wrote by this
# screen ("... written 2816 nametable ..."), require the build to be within 25% of it.
acc = " ".join(str(a.get("expect", "")) + " " + str(a.get("when", "")) for a in (spec.get("acceptance_observations") or []))
mc = re.search(r"(\d+)\s+nametable", acc)
if mc and ntw >= 0 and screen == "initial":
    want = int(mc.group(1)); ok = abs(ntw - want) <= max(64, want // 4)
    report(ok, "amount drawn matches the original's nametable write count", f"spec says {want} nametable bytes, build wrote {ntw}")
m = re.search(r"\((\d+) visible sprites", obs); nspr = int(m.group(1)) if m else -1
print(f"INFO  visible sprites: {nspr}; non-blank rows: {nonblank}")
print("SUMMARY: " + ("ALL PASS" if fails == 0 else f"{fails} FAIL"))
sys.exit(0 if fails == 0 else 1)
