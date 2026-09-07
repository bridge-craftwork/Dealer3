#!/usr/bin/env python3
"""Generate THIRD-PARTY-NOTICES from the dependency graph.

Hand-maintained notice files go stale the moment a dependency moves, and a
stale notice is worse than none — it asserts something untrue about what a
binary contains. So this reads what cargo actually resolved, finds each
package's own licence file, and copies the copyright line out of it rather
than guessing from the `authors` field, which is often empty and never
authoritative.

    scripts/third-party-notices.py            # write both notice files
    scripts/third-party-notices.py --check    # fail if either would change
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The two things this project distributes, and where each one's notices go.
# They are different graphs: the WebAssembly build pulls in wasm-bindgen,
# js-sys and wasm-bindgen-rayon, which the CLI never sees, and the CLI pulls
# in clap, which the browser never sees. One file copied to both places would
# be wrong about both.
TARGETS = [
    (ROOT, "dealer", ROOT / "THIRD-PARTY-NOTICES",
     "the dealer command-line binary"),
    (ROOT / "wasm", "dealer3-wasm", ROOT / "web" / "public" / "THIRD-PARTY-NOTICES",
     "the WebAssembly build and the web application"),
]

# Crates of this project, under this project's own licence. Not third party.
OURS = {
    "dealer", "dealer3-wasm", "dealer-core", "dealer-dds", "dealer-eval",
    "dealer-level", "dealer-parser", "dealer-pbn", "dealer-run",
}

LICENCE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE)", re.I)
COPYRIGHT = re.compile(r"^\s*(Copyright\b.*)$", re.M)


def metadata(directory):
    """The resolved dependency graph, without disturbing Cargo.lock.

    Reading the graph can rewrite the lock, and silently: a checkout with
    local `[patch]` overrides resolves its siblings to path dependencies and
    drops their `source` lines, which would break CI, where no such checkouts
    exist. `--locked` refuses rather than rewrites, but it also refuses in
    exactly that situation, so it cannot be used here. Putting the file back
    is what works for both.
    """
    lock = directory / "Cargo.lock"
    before = lock.read_bytes() if lock.exists() else None
    try:
        out = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--all-features"],
            cwd=directory, capture_output=True, text=True, check=True,
        )
    finally:
        if before is not None and lock.exists() and lock.read_bytes() != before:
            lock.write_bytes(before)
        elif before is None and lock.exists():
            lock.unlink()
    return json.loads(out.stdout)


def shipped(meta, root_name):
    """Package ids reachable from `root_name` through normal dependencies.

    Dev-dependencies are excluded: a test harness is not in the binary, and
    claiming otherwise would misstate what a copy contains. Build-dependencies
    and proc-macro crates are kept — their code does not end up in the binary
    either, but they are how it was produced, and being over-inclusive here
    errs towards crediting rather than away from it.
    """
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    roots = [p["id"] for p in meta["packages"] if p["name"] == root_name]
    seen, stack = set(), list(roots)
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes.get(current, {}).get("deps", []):
            kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
            if kinds and kinds == {"dev"}:
                continue
            stack.append(dep["pkg"])
    return seen


def copyrights(manifest: Path):
    """Copyright lines from a package's own licence files, deduplicated."""
    found, seen = [], set()
    for path in sorted(manifest.parent.iterdir()):
        if not path.is_file() or not LICENCE_FILE.match(path.name):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in COPYRIGHT.findall(text):
            line = " ".join(line.split())
            # Apache-2.0's boilerplate carries a specimen line, not a claim.
            if "[yyyy] [name of copyright owner]" in line:
                continue
            if line not in seen:
                seen.add(line)
                found.append(line)
    return found


def render(packages, what):
    lines = [
        "THIRD-PARTY NOTICES",
        "===================",
        "",
        f"For {what}.",
        "",
        "dealer3's own code is released into the public domain under The",
        "Unlicense. What it distributes is not only its own code: it is built",
        "from, and links, the packages below, whose licences ask that their",
        "copyright notices travel with copies. This file is how they travel.",
        "",
        "GENERATED by scripts/third-party-notices.py from the resolved",
        "dependency graph. Do not edit by hand; run the script.",
        "",
        f"{len(packages)} third-party packages, all under permissive licences.",
        "Dev-dependencies are excluded: a test harness is not in a binary.",
        "",
    ]

    by_licence = {}
    for pkg in packages:
        by_licence.setdefault(pkg["licence"], []).append(pkg)

    lines.append("Summary")
    lines.append("-------")
    for licence in sorted(by_licence):
        lines.append(f"  {len(by_licence[licence]):>3}  {licence}")
    lines.append("")
    lines.append("")
    lines.append("Packages")
    lines.append("--------")
    lines.append("")

    for pkg in packages:
        lines.append(f"{pkg['name']} {pkg['version']}")
        lines.append(f"    Licence: {pkg['licence']}")
        if pkg["repository"]:
            lines.append(f"    Source:  {pkg['repository']}")
        for line in pkg["copyrights"]:
            lines.append(f"    {line}")
        if not pkg["copyrights"]:
            if pkg["authors"]:
                lines.append(f"    Authors: {', '.join(pkg['authors'])}")
            elif "Unlicense" in pkg["licence"]:
                lines.append("    Dedicated to the public domain; "
                             "no copyright asserted.")
            else:
                lines.append("    (its licence files assert no copyright line, "
                             "and it names no authors)")
        lines.append("")

    lines.append("")
    lines.append("Licence texts")
    lines.append("-------------")
    lines.append("")
    lines.append("Apache-2.0: https://www.apache.org/licenses/LICENSE-2.0")
    lines.append("MIT:        https://opensource.org/licenses/MIT")
    lines.append("Unlicense:  https://unlicense.org/")
    lines.append("Unicode-3.0: https://www.unicode.org/license.txt")
    lines.append("")
    lines.append("A package offering a choice of licences is used under whichever")
    lines.append("the recipient prefers; the notice above is reproduced either way.")
    lines.append("")
    return "\n".join(lines)


def notices(directory, root_name, what):
    meta = metadata(directory)
    keep = shipped(meta, root_name)
    packages = []
    for pkg in meta["packages"]:
        if pkg["name"] in OURS or pkg["id"] not in keep:
            continue
        manifest = Path(pkg["manifest_path"])
        packages.append({
            "name": pkg["name"],
            "version": pkg["version"],
            "licence": pkg.get("license") or "see source",
            "repository": pkg.get("repository") or "",
            "copyrights": copyrights(manifest),
            "authors": [a for a in pkg.get("authors", []) if a],
        })
    packages.sort(key=lambda p: (p["name"].lower(), p["version"]))
    return render(packages, what), len(packages)


def main():
    check = "--check" in sys.argv
    failed = False
    for directory, root_name, out, what in TARGETS:
        text, count = notices(directory, root_name, what)
        where = out.relative_to(ROOT)
        if check:
            current = out.read_text() if out.exists() else ""
            if current != text:
                print(f"{where} is out of date; run "
                      "scripts/third-party-notices.py", file=sys.stderr)
                failed = True
            else:
                print(f"{where} up to date ({count} packages)")
        else:
            out.write_text(text)
            print(f"wrote {where} ({count} packages)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
