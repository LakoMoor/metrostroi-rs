"""Builds game/content/trains/81-717_mvm/tooltips_ru.json from Metrostroi's Russian language files."""
import json
import re

LANG = r"E:\metrostroi2pz\src\scripts\lua\metrostroi_data\languages" + "\\"
OUT = r"E:\metrostroi2pz\game\content\trains\81-717_mvm\tooltips_ru.json"


def parse(path):
    out = {}
    for line in open(path, encoding="utf-8"):
        m = re.match(r"^([A-Za-z0-9_.\-:]+)\s*=\s*(.*?)\s*$", line.rstrip("\n"))
        if m:
            out[m.group(1)] = m.group(2)
    return out


table = {}
for f in ("ru_base.lua", "ru_717.lua"):
    table.update(parse(LANG + f))


def resolve(text, depth=0):
    if depth > 6:
        return text
    return re.sub(r"@\[([^\]]+)\]", lambda m: resolve(table.get(m.group(1), ""), depth + 1), text).strip()


tips = {}
pre = "Entities.gmod_subway_81-717_mvm.Buttons."
for k, v in table.items():
    if k.startswith(pre):
        bid = re.split(r"[.:]", k[len(pre):])[-1]
        text = resolve(v)
        if text:
            tips[bid] = text
json.dump(tips, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=0)
print(len(tips), "tooltips")
