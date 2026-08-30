#!/usr/bin/env python3
"""
generate-tmlanguage.py - Regenerate the DLR TextMate grammar from the engine's
own constants.

Three sources, none of them this file:

    dealer-parser/src/vocabulary.rs   the words the grammar accepts, themselves
                                     checked against grammar.pest
    dealer-level/src/lib.rs           the names the levelling machinery reads
    web/src/lib/dlrLanguage.js        the `# key: value` headers PBS reads,
                                     the one list nothing in the engine owns

The result drives highlighting in the Practice-Bidding-Scenarios VS Code
extension. The web editor builds its own tokenizer from `language_info()` at
runtime, from those same constants, so the two agree by construction rather
than by anyone remembering to update both.

Run after adding a function or keyword to the language:

    python3 scripts/generate-tmlanguage.py --also-update-vscode
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
LEVEL = os.path.join(REPO, "dealer-level", "src", "lib.rs")
WEBLANG = os.path.join(REPO, "web", "src", "lib", "dlrLanguage.js")
OUT = os.path.join(REPO, "dealer-parser", "syntaxes", "dlr.tmLanguage.json")
VSCODE = os.path.normpath(os.path.join(
    REPO, "..", "Practice-Bidding-Scenarios", "vs-code", "syntaxes", "dlr.tmLanguage.json"))


def const_list(src: str, name: str, path: str):
    """The strings in a `pub const NAME: ... = [...]`, one line or many.

    Stopping at the first `];` rather than at a newline before it: the lists
    that fit on one line used to fall through to the *next* list's closing
    bracket, which swept every symbolic operator into `and|or|not` and left
    three of the grammar's patterns as regexes Oniguruma refuses to compile.
    VS Code drops a rule it cannot compile and says nothing, so `north` and
    `and` went uncoloured for as long as that lasted.
    """
    body = re.search(rf'pub const {name}:[^=]*= &?\[(.*?)\];', src, re.S)
    if not body:
        sys.exit(f"Error: could not find `{name}` in {path}")
    return re.findall(r'"([^"]+)"', body.group(1))


def const_str(src: str, name: str, path: str):
    """The value of a `pub const NAME: &str = "...";`."""
    m = re.search(rf'pub const {name}: &str = "([^"]*)";', src)
    if not m:
        sys.exit(f"Error: could not find `{name}` in {path}")
    return m.group(1)


def js_array(src: str, name: str, path: str):
    """The strings in an `export const NAME = [...]` in a JavaScript module."""
    m = re.search(rf'export const {name} = \[(.*?)\]', src, re.S)
    if not m:
        sys.exit(f"Error: could not find `{name}` in {path}")
    return re.findall(r"'([^']+)'", m.group(1))


# Only the characters that mean something to a regex. `re.escape` also escapes
# `#` and spaces, which is valid but turns `### BEGIN GENERATED LEVELING ###`
# into something no one can read in the file or search for in a test.
META = set(r"\^$.|?*+()[]{}")


def lit(text):
    return "".join("\\" + c if c in META else c for c in text)


def alternation(words):
    """Longest-first, so `spades` wins over `spade` and `top2` is not shadowed."""
    return "|".join(lit(w) for w in sorted(set(words), key=lambda w: (-len(w), w)))


def word_pattern(words):
    return r"\b(%s)\b" % alternation(words)


def main():
    vocab = open(VOCAB).read()
    funcs = const_list(vocab, "FUNCTIONS", VOCAB)
    stmts = const_list(vocab, "STATEMENT_KEYWORDS", VOCAB)
    acts = const_list(vocab, "ACTIONS", VOCAB)
    pos = const_list(vocab, "POSITIONS", VOCAB)
    vuln = const_list(vocab, "VULNERABILITIES", VOCAB)
    logic = const_list(vocab, "LOGICAL_WORDS", VOCAB)
    other = const_list(vocab, "OTHER_KEYWORDS", VOCAB)

    level = open(LEVEL).read()
    hand_prefix = const_str(level, "HAND_TYPE_PREFIX", LEVEL)
    level_prefix = const_str(level, "LEVEL_TYPE_PREFIX", LEVEL)
    share_suffix = const_str(level, "SHARE_SUFFIX", LEVEL)
    verdicts = const_list(level, "VERDICTS", LEVEL)
    no_leveling = const_str(level, "NO_LEVELING", LEVEL)
    block_begin = const_str(level, "LEVEL_BEGIN", LEVEL)
    block_end = const_str(level, "LEVEL_END", LEVEL)
    stamp = const_str(level, "LEVEL_STAMP", LEVEL)

    metadata_keys = js_array(open(WEBLANG).read(), "METADATA_KEYS", WEBLANG)

    g = json.load(open(OUT))
    g["information_for_contributors"] = [
        "Generated from dealer3: dealer-parser/src/vocabulary.rs (the words),",
        "dealer-level/src/lib.rs (the levelling names) and",
        "web/src/lib/dlrLanguage.js (the PBS metadata keys).",
        "Do not hand-edit those parts: dealer-parser/tests/tmlanguage_matches_vocabulary.rs",
        "fails the build if they drift from the parser's grammar.",
        "Regenerate with scripts/generate-tmlanguage.py.",
    ]

    # The vocabulary, and the literals the grammar recognises by shape.
    single = [p for p in pos if len(p) == 1]
    g["repository"]["keywords"]["patterns"] = [
        {"name": "keyword.control.dlr", "match": word_pattern(stmts + acts)},
        {"name": "keyword.other.condition.dlr", "match": word_pattern(logic)},
        {"name": "support.function.dlr", "match": word_pattern(funcs)},
        {"name": "constant.language.dlr",
         "match": word_pattern([p for p in pos if len(p) > 1] + other)},
        {"name": "constant.language.vulnerability.dlr",
         "match": "(?i)" + word_pattern(vuln)},
        # A bare `n` is a position and also the commonest variable name there
        # is, so only where an argument can stand.
        {"name": "constant.language.compass.dlr",
         "match": r"\b(%s)\b(?=\s*[,)])" % alternation(single)},
        {"name": "constant.other.holding.dlr", "match": r"\b([SHDC][AKQJT98765432]+)\b"},
        {"name": "constant.other.card.dlr", "match": r"\b([AKQJT98765432][SHDC])\b"},
        # `(%s)?` is the literal prefix a shape may carry, not a format slot.
        {"name": "constant.numeric.shape.dlr", "match": r"(%s)?\b[0-9xX]{4}\b"},
    ]

    # The levelling conventions. Ordinary variables to the grammar — which is
    # the point of them, since a script using them still parses on BBO — so
    # nothing but this tells an author which names the engine acts on.
    #
    # Prefix and suffix are both matched without regard to case, because that is
    # what dealer-level does: `handtype_12` is a hand type, and colouring it as
    # an ordinary variable would be the grammar disagreeing with the run. The
    # variable is still case-sensitive to *refer* to, as it is in dealer.exe,
    # which is a fact about the name rather than about the convention.
    prefixes = "|".join(lit(p) for p in (hand_prefix, level_prefix))
    g["repository"]["leveling"] = {"patterns": [
        {"name": "support.type.leveling.share.dlr",
         "match": r"\b(?i:%s)[A-Za-z0-9_]*(?i:%s)\b" % (prefixes, lit(share_suffix))},
        {"name": "support.type.leveling.dlr",
         "match": r"\b(?i:%s)[A-Za-z0-9_]*\b" % prefixes},
        {"name": "support.type.leveling.dlr",
         "match": r"\b(?:%s)\b" % "|".join(
             lit(w) for w in list(verdicts) + [no_leveling])},
    ]}
    if {"include": "#leveling"} not in g["patterns"]:
        g["patterns"].insert(g["patterns"].index({"include": "#variable-definition"}),
                             {"include": "#leveling"})

    # The generated block's markers and stamp, and the headers PBS reads. Both
    # are comments, so they go in ahead of the rule that claims the rest.
    comments = g["repository"]["comments"]["patterns"]
    marker = {"name": "comment.line.leveling.marker.dlr",
              "match": r"^\s*(?:%s|%s|%s).*$" % (
                  lit(block_begin), lit(block_end), lit(stamp))}
    metadata = {
        "name": "comment.line.hash.metadata.dlr",
        # Only a key PBS reads. A mistyped one falls through to the plain
        # comment rule below and loses its colour, which is the whole point:
        # before this it was indistinguishable from one that works.
        "match": r"^(#)\s*(?i:(%s))\s*(:)\s*(.*)$" % alternation(metadata_keys),
        "captures": {
            "1": {"name": "punctuation.definition.comment.dlr"},
            "2": {"name": "entity.name.tag.metadata.dlr"},
            "3": {"name": "punctuation.separator.dlr"},
            "4": {"name": "string.unquoted.metadata.dlr"},
        },
    }
    keep = [p for p in comments
            if p.get("name") not in ("comment.line.hash.metadata.dlr",
                                     "comment.line.leveling.marker.dlr")]
    section = next(i for i, p in enumerate(keep)
                   if p.get("name") == "comment.line.hash.section.dlr")
    g["repository"]["comments"]["patterns"] = (
        keep[:section + 1] + [marker, metadata] + keep[section + 1:])

    # Every pattern this writes, compiled. Oniguruma is not Python's `re`, but
    # it agrees about the failure that mattered here — an alternation with an
    # operator dropped into it unescaped — and VS Code reports a rule it cannot
    # compile by quietly not applying it.
    for section_name, node in g["repository"].items():
        for pattern in node.get("patterns", []):
            for key in ("match", "begin", "end"):
                if key not in pattern:
                    continue
                try:
                    re.compile(pattern[key])
                except re.error as e:
                    sys.exit(f"Error: {section_name}/{pattern.get('name')} "
                             f"{key} does not compile: {e}\n  {pattern[key]}")

    json.dump(g, open(OUT, "w"), indent=2)
    open(OUT, "a").write("\n")
    print(f"wrote {OUT} ({len(funcs)} functions, {len(stmts) + len(acts)} keywords, "
          f"{len(metadata_keys)} metadata keys)")

    if "--also-update-vscode" in sys.argv:
        if not os.path.isdir(os.path.dirname(VSCODE)):
            sys.exit(f"Error: VS Code extension not found at {VSCODE}")
        shutil.copy(OUT, VSCODE)
        print(f"copied to {VSCODE}")


if __name__ == "__main__":
    main()
