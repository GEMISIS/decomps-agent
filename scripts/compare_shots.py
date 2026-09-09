#!/usr/bin/env python3
"""Compose labelled side-by-side comparison images.
usage: compare_shots.py out.png "Title" original.png rebuilt.png ["Title2" original2.png rebuilt2.png ...]
Each triple becomes one row: ORIGINAL | REBUILT, scaled 2x, with a diff percentage in the row label."""
import sys
from PIL import Image, ImageDraw, ImageChops
out = sys.argv[1]; args = sys.argv[2:]; rows = [args[i:i+3] for i in range(0, len(args), 3)]
SCALE = 2; PAD = 12; LABEL_H = 22
imgs = []
for title, a, b in rows:
    A = Image.open(a).convert("RGB"); B = Image.open(b).convert("RGB")
    if B.size != A.size: B = B.resize(A.size)
    diff = ImageChops.difference(A, B).convert("L"); bbox = diff.getbbox()
    differing = sum(1 for p in diff.getdata() if p > 16) / (A.size[0] * A.size[1]) * 100
    A2 = A.resize((A.width*SCALE, A.height*SCALE), Image.NEAREST); B2 = B.resize((B.width*SCALE, B.height*SCALE), Image.NEAREST)
    row = Image.new("RGB", (A2.width*2 + PAD*3, A2.height + LABEL_H + PAD), (30, 30, 30)); d = ImageDraw.Draw(row)
    d.text((PAD, 4), f"{title} — ORIGINAL", fill=(230, 230, 230)); d.text((A2.width + PAD*2, 4), f"REBUILT   (pixels differing: {differing:.1f}%)", fill=(230, 230, 230))
    row.paste(A2, (PAD, LABEL_H)); row.paste(B2, (A2.width + PAD*2, LABEL_H)); imgs.append(row)
W = max(i.width for i in imgs); H = sum(i.height for i in imgs)
sheet = Image.new("RGB", (W, H), (30, 30, 30)); y = 0
for i in imgs: sheet.paste(i, (0, y)); y += i.height
sheet.save(out); print(f"wrote {out} ({W}x{H})")
