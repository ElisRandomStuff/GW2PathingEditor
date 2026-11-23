#!/usr/bin/env python3
"""Writes THIRD-PARTY-NOTICES.txt: the licences of the code compiled into the addon DLL.

Ship that file with every released DLL. Run it again after changing dependencies:

    python3 scripts/third_party_notices.py
"""

import json
import pathlib
import re
import subprocess

TARGET = "x86_64-pc-windows-gnu"
ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-NOTICES.txt"
LICENCE_FILE = re.compile(r"(?i)^(licen[cs]e|copying)")

MIT_TEMPLATE = """MIT License

Copyright (c) {holders}

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and
to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
IN THE SOFTWARE.
"""


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def licence_files(directory):
    if not directory.is_dir():
        return []
    return sorted(p for p in directory.iterdir() if p.is_file() and LICENCE_FILE.match(p.name))


def licence_text(package):
    """The package's licence, MIT when it offers a choice (all of them allow it but one)."""
    directory = pathlib.Path(package["manifest_path"]).parent
    # Crates in a git workspace keep their licence at the workspace root.
    files = licence_files(directory) or licence_files(directory.parent)
    spdx = package.get("license") or ""
    mit = [f for f in files if "mit" in f.name.lower()]
    if mit:
        return mit[0].read_text(errors="replace")
    for f in files:
        text = f.read_text(errors="replace")
        if "apache" not in f.name.lower() or len(files) == 1:
            return text
    if "MIT" in spdx or not spdx:
        holders = ", ".join(a.split("<")[0].strip() for a in package.get("authors", [])) or package["name"]
        return MIT_TEMPLATE.format(holders=f"the {package['name']} authors ({holders})")
    return f"Licensed under {spdx}. See {package.get('repository') or 'the crate'} for the full text.\n"


def main():
    metadata = json.loads(
        run("cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET)
    )
    tree = run("cargo", "tree", "--target", TARGET, "-e", "normal", "--prefix", "none", "--format", "{p}")
    used = {tuple(m.groups()) for m in re.finditer(r"^(\S+) v(\S+)", tree, re.M)}
    packages = sorted(
        (p for p in metadata["packages"] if (p["name"], p["version"]) in used and p["name"] != "pathing_editor"),
        key=lambda p: (p["name"], p["version"]),
    )

    imgui_sys = next(p for p in packages if p["name"] == "arcdps-imgui-sys")
    imgui_licence = pathlib.Path(imgui_sys["manifest_path"]).parent / "third-party" / "imgui" / "LICENSE.txt"

    sections = [
        (
            "Rust standard library (MIT OR Apache-2.0), https://github.com/rust-lang/rust",
            MIT_TEMPLATE.format(holders="The Rust Project Contributors"),
        ),
        ("Dear ImGui 1.80 (MIT), https://github.com/ocornut/imgui", imgui_licence.read_text()),
    ]
    for p in packages:
        source = p.get("repository") or p.get("homepage") or ""
        title = f"{p['name']} {p['version']} ({p.get('license') or 'MIT'}){', ' + source if source else ''}"
        sections.append((title, licence_text(p)))

    rule = "=" * 100
    out = [
        "Third-party notices for pathing_editor.dll",
        "",
        "The Pathing Editor addon (MIT, see LICENSE) is compiled together with the software below. Where a",
        "component offers a choice of licences, it is used under the MIT licence.",
        "",
        "The DLL also links the MinGW-w64 C runtime startup code (public domain / ZPL) and parts of libgcc",
        "(GPL with the GCC Runtime Library Exception, which needs no notice).",
        "",
    ]
    for title, text in sections:
        out += [rule, title, rule, "", text.strip(), ""]
    OUT.write_text("\n".join(out))
    print(f"Wrote {OUT.relative_to(ROOT)}: {len(sections)} components")


if __name__ == "__main__":
    main()
