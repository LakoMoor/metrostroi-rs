"""Builds the UI font atlas: game/assets/fonts/ui_font.png + ui_font.json (advances), copied into crates/viewer/assets."""
import json, os, shutil
from PIL import Image, ImageDraw, ImageFont

FONT = next(p for p in (r"C:\Windows\Fonts\segoeui.ttf", r"C:\Windows\Fonts\arial.ttf") if os.path.exists(p))
SIZE, COLS, CW, CH = 40, 16, 44, 56
chars = [chr(c) for c in range(32, 127)] + [chr(0x401)] + [chr(c) for c in range(0x410, 0x450)] + [chr(0x451)] + list("—…№«»°·×↑↓←→")
f = ImageFont.truetype(FONT, SIZE)
rows = (len(chars) + COLS - 1) // COLS
im = Image.new("RGBA", (COLS * CW, rows * CH + 8), (255, 255, 255, 0))
im.paste((255, 255, 255, 255), (0, rows * CH, 8, rows * CH + 8))  # solid block for rectangles
adv = {}
for i, ch in enumerate(chars):
    x, y = (i % COLS) * CW, (i // COLS) * CH
    mask = Image.new("L", (CW, CH), 0)
    ImageDraw.Draw(mask).text((4, 4), ch, font=f, fill=255)
    im.paste((255, 255, 255, 255), (x, y), mask)
    adv[ord(ch)] = [i, round(f.getlength(ch))]
out = os.path.join("game", "assets", "fonts")
os.makedirs(out, exist_ok=True)
im.save(os.path.join(out, "ui_font.png"))
json.dump({"cell": [CW, CH], "cols": COLS, "size": SIZE, "pad": 4, "glyphs": adv, "white": [4, rows * CH + 4], "atlas": list(im.size)}, open(os.path.join(out, "ui_font.json"), "w"))
for n in ("ui_font.png", "ui_font.json"):
    shutil.copy(os.path.join(out, n), os.path.join("sim", "crates", "viewer", n))
print(FONT, len(chars), "glyphs", im.size)
