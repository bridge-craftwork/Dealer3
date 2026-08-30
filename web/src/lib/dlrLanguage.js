// CodeMirror language support for dealer scripts.
//
// The tokenizer is BUILT AT RUNTIME from the engine's own `language_info()`,
// rather than shipping a second copy of the word lists. That export comes from
// `dealer_parser::vocabulary`, which is itself checked against `grammar.pest` by
// two tests in dealer-parser. So highlighting cannot advertise a function the
// parser does not accept, or miss one it does — the failure mode that left 19
// functions unhighlighted in the VS Code extension for years.
//
// This is also why the editor does not load `dlr.tmLanguage.json` directly. That
// file still exists and is still what VS Code uses, but both it and this are
// generated from the same vocabulary, so they agree by construction rather than
// by anyone remembering to update two places.

import {
  HighlightStyle,
  LanguageSupport,
  StreamLanguage,
  syntaxHighlighting,
} from '@codemirror/language'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'

/** Longest first, so `spades` matches before `spade`. */
const byLengthDesc = (a, b) => b.length - a.length || a.localeCompare(b)

/** Case-insensitive lookup set — the grammar accepts any casing. */
function lowerSet(words) {
  return new Set(words.map((w) => w.toLowerCase()))
}

/**
 * The `# key: value` headers a PBS scenario carries.
 *
 * Hand-kept, unlike everything else here, because these belong to PBS and not
 * to the engine — nothing in dealer reads them, so there is no vocabulary to
 * derive them from.
 *
 * What the list buys is the misspelling. A key PBS reads is coloured; anything
 * else falls through to an ordinary comment, so `# scenario-titel:` simply does
 * not light up, where before it looked exactly like one that works.
 * Colouring the good rather than marking the bad, deliberately: PBS may add a
 * key before this list hears of it, and a new key reading as a plain comment is
 * a smaller lie than a new key marked wrong.
 */
export const METADATA_KEYS = [
  'alias',
  'auction-filter',
  'bba-works',
  'button-text',
  'convention-card',
  'convention-card-ew',
  'convention-card-ns',
  'gib-works',
  'scenario-title',
]

/**
 * A CodeMirror stream tokenizer for the dealer language.
 *
 * `info` is the engine's `language_info()` output.
 */
export function dlrStreamParser(info) {
  const functions = lowerSet(info.functions)
  const keywords = lowerSet([...info.statement_keywords, ...info.actions])
  // Single-letter positions (`n`, `s`, `e`, `w`) are valid but are also common
  // variable names; the evaluator lets a variable shadow them. Colouring every
  // `n` would be worse than leaving them plain.
  const constants = lowerSet([
    ...info.positions.filter((p) => p.length > 1),
    ...info.vulnerabilities,
    ...info.other_keywords,
  ])
  const logical = lowerSet(info.logical_words)
  const metadataKeys = lowerSet(METADATA_KEYS)

  // Sorted only so the behaviour is deterministic and testable.
  const sortedFunctions = [...info.functions].sort(byLengthDesc)

  // The levelling conventions, from the engine's own constants. A build too old
  // to send them colours nothing rather than guessing at them — a second copy
  // of `HandType` here is exactly what the rest of this file exists to avoid.
  const level = info.leveling ?? null
  const verdicts = new Set(level ? [...level.verdicts, level.no_leveling] : [])

  /**
   * Which levelling token, if any, a name earns.
   *
   * Prefixes are matched with regard to case and the share suffix without,
   * because that is what `dealer_level` does: `handtype_12` is not a hand type
   * and will silently not be one, so it must not be coloured as though it were,
   * while `HandType_12_share` is a share and is.
   */
  const levelingToken = (name) => {
    if (!level) return null
    if (name.startsWith(level.hand_type_prefix) || name.startsWith(level.level_type_prefix)) {
      // A share carries its type's prefix — a bare `foo_Share` weights nothing.
      const tail = name.slice(-level.share_suffix.length)
      const share =
        name.length > level.share_suffix.length &&
        tail.toLowerCase() === level.share_suffix.toLowerCase()
      return share ? 'levelingShare' : 'levelingName'
    }
    return verdicts.has(name) ? 'levelingName' : null
  }

  return {
    name: 'dlr',

    startState: () => ({ inBlockComment: false, inMetaValue: false }),

    token(stream, state) {
      if (state.inBlockComment) {
        if (stream.match(/^.*?\*\//)) state.inBlockComment = false
        else stream.skipToEnd()
        return 'comment'
      }

      // The value half of a `# key: value` header runs to the end of its line.
      // Without this `# scenario-title: Jacoby 2NT` would carry on into the
      // ordinary rules and colour `Jacoby` as a variable. An empty value leaves
      // the flag set with nothing to consume, hence the `sol` guard.
      if (state.inMetaValue) {
        state.inMetaValue = false
        if (!stream.sol()) {
          stream.skipToEnd()
          return 'metaValue'
        }
      }

      if (stream.eatSpace()) return null

      // The generated levelling block's own lines: the two markers and the
      // stamp. They are comments — that is what lets a levelled scenario still
      // run on BBO — so without this nothing tells them from an author's aside,
      // and the block that must not be edited by hand looks like prose.
      if (stream.sol() && level) {
        if (
          stream.match(level.block_begin) ||
          stream.match(level.block_end) ||
          stream.match(level.stamp)
        ) {
          stream.skipToEnd()
          return 'levelingMarker'
        }
      }

      // `# key: value` headers PBS scripts carry, then ordinary comments.
      if (stream.sol()) {
        const header = stream.match(/^\s*#\s*([a-zA-Z][a-zA-Z0-9-]*)\s*:/)
        if (header) {
          if (metadataKeys.has(header[1].toLowerCase())) {
            state.inMetaValue = true
            return 'metaKey'
          }
          // Not a key PBS reads. Left as the comment it really is.
          stream.skipToEnd()
          return 'comment'
        }
      }
      if (stream.match('#') || stream.match('//')) {
        stream.skipToEnd()
        return 'comment'
      }
      if (stream.match('/*')) {
        state.inBlockComment = true
        if (stream.match(/^.*?\*\//)) state.inBlockComment = false
        else stream.skipToEnd()
        return 'comment'
      }

      if (stream.match(/^"[^"]*"?/)) return 'string'

      // Shape patterns before numbers, which would otherwise eat the digits.
      if (stream.match(/^%s?\d{4}\b/) || stream.match(/^[0-9xX]{4}\b/)) return 'number'

      // Card literals (AS, TC) and predeal holdings (SAKQ, HT62).
      if (stream.match(/^[AKQJT2-9][SHDC]\b/)) return 'atom'
      if (stream.match(/^[SHDC][AKQJT2-9]+\b/)) return 'atom'

      const word = stream.match(/^[a-zA-Z_][a-zA-Z0-9_]*/)
      if (word) {
        const w = word[0].toLowerCase()
        if (keywords.has(w)) return 'keyword'
        if (functions.has(w)) return 'function'
        if (constants.has(w)) return 'atom'
        if (logical.has(w)) return 'operatorKeyword'
        return levelingToken(word[0]) ?? 'variableName'
      }

      if (stream.match(/^\d+/)) return 'number'
      if (stream.match(/^(==|!=|>=|<=|&&|\|\||[-+*/%<>?:!=])/)) return 'operator'
      if (stream.match(/^[()]/)) return 'paren'
      if (stream.match(/^[,;]/)) return 'separator'

      stream.next()
      return null
    },

    languageData: {
      commentTokens: { line: '#', block: { open: '/*', close: '*/' } },
      closeBrackets: { brackets: ['(', '"'] },
    },

    // Exposed for the completion source and for tests.
    _vocabulary: { sortedFunctions, info },
  }
}

/** Completion over the engine's vocabulary. */
export function dlrCompletion(info) {
  const options = [
    // Functions all take arguments, so complete the parentheses too.
    ...info.functions.map((label) => ({
      label,
      type: 'function',
      detail: 'function',
      apply: `${label}()`,
    })),
    ...info.statement_keywords.map((label) => ({ label, type: 'keyword', detail: 'statement' })),
    ...info.actions.map((label) => ({ label, type: 'keyword', detail: 'action' })),
    ...info.positions
      .filter((p) => p.length > 1)
      .map((label) => ({ label, type: 'constant', detail: 'position' })),
    ...info.vulnerabilities.map((label) => ({ label, type: 'constant', detail: 'vulnerability' })),
    ...info.logical_words.map((label) => ({ label, type: 'keyword', detail: 'operator' })),
  ]

  return (context) => {
    const word = context.matchBefore(/[a-zA-Z_][a-zA-Z0-9_]*/)
    if (!word || (word.from === word.to && !context.explicit)) return null
    return { from: word.from, options, validFor: /^[a-zA-Z_][a-zA-Z0-9_]*$/ }
  }
}

/**
 * Run the stream tokenizer over one line, returning `[token, text]` pairs.
 *
 * CodeMirror drives the tokenizer itself through its own stream; this is a
 * minimal stand-in so the same tokens can be produced outside an editor — the
 * print view needs them to colour the script on paper, where CodeMirror's
 * generated class names (`ͼ1`, `ͼ2`…) are not stable enough to target.
 */
export function tokenizeLine(parser, line, state = parser.startState()) {
  const out = []
  const stream = {
    pos: 0,
    string: line,
    sol() { return this.pos === 0 },
    peek() { return this.string[this.pos] },
    next() { return this.string[this.pos++] },
    eatSpace() {
      const start = this.pos
      while (/\s/.test(this.string[this.pos])) this.pos++
      return this.pos > start
    },
    skipToEnd() { this.pos = this.string.length },
    match(pattern, consume = true) {
      if (typeof pattern === 'string') {
        if (this.string.startsWith(pattern, this.pos)) {
          if (consume) this.pos += pattern.length
          return true
        }
        return false
      }
      const m = this.string.slice(this.pos).match(pattern)
      if (m && m.index === 0) {
        if (consume) this.pos += m[0].length
        return m
      }
      return null
    },
  }

  let guard = 0
  while (stream.pos < line.length && guard++ < 2000) {
    const from = stream.pos
    const token = parser.token(stream, state)
    if (stream.pos === from) stream.pos++ // never stall on an unmatched char
    out.push([token, line.slice(from, stream.pos)])
  }
  return out
}

/** The full language extension. */
export function dlrLanguage(info) {
  return new LanguageSupport(
    StreamLanguage.define({ ...dlrStreamParser(info), tokenTable: DLR_TOKENS }),
    // The classes and what they look like, so any view of a script gets both.
    [dlrHighlighting, dlrTokenColors],
  )
}

/// Token names the parser emits that CodeMirror cannot resolve on its own.
///
/// It looks a name up in `@lezer/highlight`'s `tags`, and most of what the
/// parser returns — `keyword`, `atom`, `number`, `operator`, `variableName` —
/// is there. `function` is not: it is a modifier there, applied to something
/// else, so returning it as a bare name silently produced no styling at all.
/// Every function in every script was the colour of plain text for as long as
/// the highlighter has existed, and the unit test asserting `hcp` tokenises as
/// `function` passed throughout, because that much was true.
///
/// The levelling and header tokens are here for the same reason: they name
/// nothing `@lezer/highlight` knows, so without an entry they would style
/// nothing, in exactly the same silence.
export const DLR_TOKENS = {
  function: tags.function(tags.variableName),

  // Names the levelling machinery reads. `special` and `definition` are chosen
  // for being tags a stream parser can hand back, not for what a theme makes of
  // them — `dlrHighlighting` below is what decides how they look.
  levelingName: tags.special(tags.variableName),
  levelingShare: tags.definition(tags.variableName),
  levelingMarker: tags.meta,

  // The two halves of a `# key: value` header.
  metaKey: tags.propertyName,
  metaValue: tags.attributeValue,

  // Ordinary variables. The same tag CodeMirror would have found on its own —
  // it is here only so `DLR_TOKEN_CLASSES` can hang a class on it and the
  // editor's theme can take the colour off one-dark.
  variableName: tags.variableName,
}

/// Plain class names for the tokens this language adds, so the editor's theme
/// can colour them by name.
///
/// A `HighlightStyle` normally generates its own class and puts the colour in
/// it. These tokens want the opposite: the tags they map to are ones the base
/// theme already has an opinion about — `special(variableName)` is what one-dark
/// paints atoms — and two generated classes on one span settle it by whichever
/// stylesheet happened to be written last. A named class the editor's theme
/// styles wins on specificity instead, which is a fact about CSS rather than a
/// fact about load order.
///
/// Exported as a map because a class here with no rule in `ScriptEditor.vue` is
/// the same silent nothing `function` was, and a test can only check for it if
/// it can read the list.
export const DLR_TOKEN_CLASSES = {
  variableName: 'dlr-variable',
  levelingName: 'dlr-leveling-name',
  levelingShare: 'dlr-leveling-share',
  levelingMarker: 'dlr-leveling-marker',
  metaKey: 'dlr-meta-key',
  metaValue: 'dlr-meta-value',
}

/// The classes for those tokens, layered over whatever base theme is in use.
export const dlrHighlighting = syntaxHighlighting(
  HighlightStyle.define(
    Object.entries(DLR_TOKEN_CLASSES).map(([token, cls]) => ({
      tag: DLR_TOKENS[token],
      class: cls,
    })),
  ),
)

/// And their colours, tuned for one-dark's dark ground.
///
/// Here rather than in a component, because there are two of them — the editor
/// and the read-only viewer the Leveled tab shows — and for a while only one
/// had these rules. The other emitted every class and coloured none of them,
/// so a generated scenario's `HandType_` names and `# key:` headers fell back
/// to whatever one-dark makes of the underlying tags, which is coral for both.
/// A language that ships its own classes should ship what they look like.
export const dlrTokenColors = EditorView.theme({
  // A variable is the one thing in a script that is *not* a word the language
  // knows, and one-dark paints it coral — the most emphatic hue in the palette,
  // spent on the token carrying the least meaning. A script is mostly
  // variables, so most of the pane was shouting. Near-white instead, which is
  // what VS Code does with the same files, and it leaves the keywords and
  // functions to be the coloured things.
  '.dlr-variable': { color: '#ccd1d9' },

  // The names the levelling machinery reads, which are ordinary variables to
  // the grammar and so would be that same near-white without this. One-dark's
  // eight hues are all spoken for, and a ninth close enough to fit would be
  // close enough to confuse — so these are marked by a dotted rule under a
  // brighter white, which no other token wears. The underline's colour is the
  // only thing separating a hand type from the share that weights it.
  '.dlr-leveling-name': {
    color: '#dfe4ec',
    textDecoration: 'underline dotted #61afef',
    textUnderlineOffset: '3px',
  },
  '.dlr-leveling-share': {
    color: '#dfe4ec',
    textDecoration: 'underline dotted #e5c07b',
    textUnderlineOffset: '3px',
  },

  // The generated block's markers and stamp. Comments, and left the colour of
  // comments — they have to be comments for a levelled scenario to run on BBO —
  // but weighted, so the region you must not edit by hand is bracketed visibly.
  '.dlr-leveling-marker': { color: '#93a1b5', fontWeight: '700' },

  // A `# key: value` header PBS reads, in one-dark's own hues: the key in the
  // whiskey it paints constants, which cannot appear in a header, the value in
  // the green it paints text. The effect that matters is on the line this does
  // *not* match — a mistyped key leaves the whole header the flat grey of an
  // ordinary comment, which is what it has become.
  '.dlr-meta-key': { color: '#d19a66', fontWeight: '700' },
  '.dlr-meta-value': { color: '#98c379' },
})

export const LANGUAGE_ID = 'dlr'
