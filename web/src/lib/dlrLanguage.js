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

import { StreamLanguage, LanguageSupport } from '@codemirror/language'
import { tags } from '@lezer/highlight'

/** Longest first, so `spades` matches before `spade`. */
const byLengthDesc = (a, b) => b.length - a.length || a.localeCompare(b)

/** Case-insensitive lookup set — the grammar accepts any casing. */
function lowerSet(words) {
  return new Set(words.map((w) => w.toLowerCase()))
}

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

  // Sorted only so the behaviour is deterministic and testable.
  const sortedFunctions = [...info.functions].sort(byLengthDesc)

  return {
    name: 'dlr',

    startState: () => ({ inBlockComment: false }),

    token(stream, state) {
      if (state.inBlockComment) {
        if (stream.match(/^.*?\*\//)) state.inBlockComment = false
        else stream.skipToEnd()
        return 'comment'
      }

      if (stream.eatSpace()) return null

      // `# key: value` headers PBS scripts carry, then ordinary comments.
      if (stream.sol() && stream.match(/^\s*#\s*[a-zA-Z-]+:/)) {
        stream.skipToEnd()
        return 'docComment'
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
        return 'variableName'
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
export const DLR_TOKENS = {
  function: tags.function(tags.variableName),
}

export const LANGUAGE_ID = 'dlr'
