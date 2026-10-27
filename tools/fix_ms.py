p = r"E:\metrostroi2pz\tools\make_share.py"
s = open(p, encoding="utf-8").read()
i = s.index("GITIGNORE = ")
j = s.index('(OUT / ".gitignore")')
s = s[:i] + 'GITIGNORE = "\\n".join(["sim/target/", "*.exe", "game/content/maps/*/map.bsp", "game/screenshots/", "game/logs/", ""])\n' + s[j:]
open(p, "w", encoding="utf-8").write(s)
