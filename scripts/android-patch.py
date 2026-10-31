#!/usr/bin/env python3
"""Донастройва генерирания Android проект (след `npx cap add android`):
разрешения за камера (сканиране на QR код) и мрежа, http връзка към компютъра и икони на приложението."""
import os
import re
import sys

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(ROOT, "android", "app", "src", "main")
MANIFEST = os.path.join(APP, "AndroidManifest.xml")
RES = os.path.join(APP, "res")
SRC = os.path.join(ROOT, "resources", "icon-source.png")
BG = "#0c0f22"

if not os.path.exists(MANIFEST):
    sys.exit("Няма android/ проект. Първо изпълнете: npx cap add android")

# ---- манифест
with open(MANIFEST, encoding="utf-8") as f:
    m = f.read()

perms = [
    '<uses-permission android:name="android.permission.CAMERA" />',
    '<uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />',
    '<uses-permission android:name="android.permission.WAKE_LOCK" />',
    '<uses-feature android:name="android.hardware.camera" android:required="false" />',
    '<uses-feature android:name="android.hardware.camera.autofocus" android:required="false" />',
]
missing = [p for p in perms if p.split('"')[1] not in m]
if missing:
    block = "".join("    " + p + "\n" for p in missing)
    m = m.replace("</manifest>", block + "</manifest>", 1)

if "usesCleartextTraffic" not in m:
    m = re.sub(r"<application", '<application android:usesCleartextTraffic="true"', m, count=1)

with open(MANIFEST, "w", encoding="utf-8") as f:
    f.write(m)

# ---- икони
src = Image.open(SRC).convert("RGBA")
legacy = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
foreground = {"mdpi": 108, "hdpi": 162, "xhdpi": 216, "xxhdpi": 324, "xxxhdpi": 432}

for dens, size in legacy.items():
    d = os.path.join(RES, "mipmap-" + dens)
    os.makedirs(d, exist_ok=True)
    icon = src.resize((size, size), Image.LANCZOS)
    icon.save(os.path.join(d, "ic_launcher.png"))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, size - 1, size - 1), fill=255)
    round_icon = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    round_icon.paste(icon, (0, 0), mask)
    round_icon.save(os.path.join(d, "ic_launcher_round.png"))

for dens, size in foreground.items():
    d = os.path.join(RES, "mipmap-" + dens)
    os.makedirs(d, exist_ok=True)
    inner = int(size * 0.62)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.paste(src.resize((inner, inner), Image.LANCZOS), ((size - inner) // 2, (size - inner) // 2))
    canvas.save(os.path.join(d, "ic_launcher_foreground.png"))

vals = os.path.join(RES, "values")
os.makedirs(vals, exist_ok=True)
with open(os.path.join(vals, "ic_launcher_background.xml"), "w", encoding="utf-8") as f:
    f.write('<?xml version="1.0" encoding="utf-8"?>\n<resources>\n    <color name="ic_launcher_background">%s</color>\n</resources>\n' % BG)

print("Android проектът е донастроен.")
