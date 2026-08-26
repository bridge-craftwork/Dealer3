#!/usr/bin/env python3
"""
generate-tmlanguage.py - Regenerate the DLR TextMate grammar from the parser's
vocabulary.

The word lists in dealer-parser/src/vocabulary.rs are the source of truth, and
are themselves checked against grammar.pest by
dealer-parser/tests/vocabulary_matches_grammar.rs. This script projects them
into the TextMate grammar used for syntax highlighting by both the web editor
and the Practice-Bidding-Scenarios VS Code extension.

Run after adding a function or keyword to the language:

    python3 scripts/generate-tmlanguage.py
    cargo test -p dealer-parser

Writes:
    dealer-parser/syntaxes/dlr.tmLanguage.json

Pass --also-update-vscode to copy the result into the PBS extension as well.
"""
import json
import os
import re
import shutil
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VOCAB = os.path.join(REPO, "dealer-parser", "src", "vocabulary.rs")
OUT = os.path.join(REPO, "dealer-parser", "syntaxes", "dlr.tmLanguage.json")
VSCODE = os.path.normpath(os.path.join(
    REPO, "..", "Practice-Bidding-Scenarios", "vs-code", "syntaxes", "dlr.tmLanguage.json"))


def const_list(src: str, name: str):
    body = re.search(rf'pub const {name}: &\[&str\] = &\[(.*?)\n\];', src, re.S)
    if not body:
        sys.exit(f"Error: could not find `{name}` in {VOCAB}")
    return re.findall(r'"([^"]+)"', body.group(1))


def alternation(words):
    """Longest-first, so `spades` wins over `spade` and `top2` is not shadowed."""
    return "|".join(sorted(set(words), key=lambda w: (-len(w), w)))


def main():
    src = open(VOCAB).read()
    funcs = const_list(src, "FUNCTIONS")
    stmts = const_list(src, "STATEMENT_KEYWORDS")
    acts = const_list(src, "ACTIONS")
    pos = const_list(src, "POSITIONS")
    vuln = const_list(src, "VULNERABILITIES")
    logic = const_list(src, "LOGICAL_WORDS")
    other = const_list(src, "OTHER_KEYWORDS")

    g = json.load(open(OUT))
    g["information_for_contributors"] = [
        "Generated from dealer3's dealer-parser/src/vocabulary.rs.",
        "Do not hand-edit the word lists: dealer-parser/tests/tmlanguage_matches_vocabulary.rs",
        "fails the build if they drift from the parser's grammar.",
        "Regenerate with scripts/generate-tmlanguage.py.",
    ]

    kw = g["repository"]["keywords"]["patterns"]
    kw[0] = {"name": "keyword.control.dlr",
             "match": r"\b(%s)\b" % alternation(stmts + acts)}
    kw[1] = {"name": "keyword.other.condition.dlr",
             "match": r"\b(%s)\b" % alternation(logic)}
    kw[2] = {"name": "support.function.dlr",
             "match": r"\b(%s)\b" % alternation(funcs)}
    kw[3] = {"name": "constant.language.dlr",
             "match": r"\b(%s)\b" % alternation([p for p in pos if len(p) > 1] + other)}

    json.dump(g, open(OUT, "w"), indent=2)
    open(OUT, "a").write("\n")
    print(f"wrote {OUT} ({len(funcs)} functions, {len(stmts) + len(acts)} keywords)")

    if "--also-update-vscode" in sys.argv:
        if not os.path.isdir(os.path.dirname(VSCODE)):
            sys.exit(f"Error: VS Code extension not found at {VSCODE}")
        shutil.copy(OUT, VSCODE)
        print(f"copied to {VSCODE}")


if __name__ == "__main__":
    main()
