#!/usr/bin/env python3
"""Les licences des crates compilées dans Solar (macOS, Apple Silicon) : nom,
version, licence déclarée, dépôt, puis les fichiers de licence de chaque crate,
tels quels. Écrit sur la sortie standard ; la release le joint au paquet.

    scripts/third-party-licenses.py > THIRD_PARTY_LICENSES.txt
"""
import json
import os
import subprocess
import sys

TARGET = "aarch64-apple-darwin"

MIT = """Copyright (c) <année> <titulaire>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""

meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET],
    check=True, capture_output=True, text=True,
).stdout)
packages = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
members = set(meta["workspace_members"])

# Les dépendances de l'app (normales et de build), sans celles des tests.
start = [i for i in members if packages[i]["name"] == "solar"]
seen, todo = set(), list(start)
while todo:
    node = nodes[todo.pop()]
    for dep in node["deps"]:
        kinds = {k["kind"] for k in dep["dep_kinds"]}
        if kinds - {"dev"} and dep["pkg"] not in seen:
            seen.add(dep["pkg"])
            todo.append(dep["pkg"])
seen -= members

out = sys.stdout
out.write("Licences des crates compilées dans Solar (%s), %d crates.\n" % (TARGET, len(seen)))
out.write("Solar lui-même : MIT ou Apache 2.0 (LICENSE-MIT, LICENSE-APACHE) ; autres fichiers : THIRD_PARTY.md.\n\n")
for pkg in sorted((packages[i] for i in seen), key=lambda p: (p["name"], p["version"])):
    out.write("=" * 78 + "\n")
    out.write("%s %s\n" % (pkg["name"], pkg["version"]))
    out.write("Licence : %s\n" % (pkg.get("license") or "voir le fichier ci-dessous"))
    if pkg.get("authors"):
        out.write("Auteurs : %s\n" % ", ".join(pkg["authors"]))
    if pkg.get("repository"):
        out.write("Dépôt : %s\n" % pkg["repository"])
    root = os.path.dirname(pkg["manifest_path"])
    files = sorted(
        f for f in os.listdir(root)
        if f.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE"))
        and os.path.isfile(os.path.join(root, f))
    )
    if pkg.get("license_file"):
        named = os.path.normpath(os.path.join(root, pkg["license_file"]))
        if os.path.isfile(named) and os.path.basename(named) not in files:
            files.append(os.path.relpath(named, root))
    for name in files:
        with open(os.path.join(root, name), encoding="utf-8", errors="replace") as text:
            out.write("\n--- %s ---\n\n%s\n" % (name, text.read().rstrip()))
    if not files:
        out.write("(aucun fichier de licence dans la crate : texte de référence en fin de fichier)\n")
    out.write("\n")

# Les textes de référence, pour les crates qui ne joignent pas le leur.
here = os.path.dirname(os.path.abspath(__file__))
out.write("=" * 78 + "\nTexte de référence : Apache License 2.0\n\n")
with open(os.path.join(here, "..", "LICENSE-APACHE"), encoding="utf-8") as text:
    out.write(text.read())
out.write("\n" + "=" * 78 + "\nTexte de référence : MIT (le titulaire est l'auteur de la crate)\n\n")
out.write(MIT)
