// Shaping the engine's `language_info()` into the sections a reference page
// shows.
//
// Everything here is a pure transform of that payload, which is why it lives
// apart from the component: the page can then be checked without a browser,
// and the component is left with nothing but markup.
//
// Nothing in this file names a function, an operator or a keyword. If it did,
// the page could describe a language the parser does not accept — the exact
// failure the vocabulary tables exist to prevent.

/**
 * Functions in the engine's group order, aliases last within each group.
 *
 * `function_groups` is the order the parser crate declares, so a new group
 * appears here without this file being touched. A group the engine lists but
 * has no entries for is dropped rather than rendered as an empty heading.
 */
export function functionSections(info) {
  const groups = info.function_groups ?? []
  const docs = info.function_docs ?? []

  const sections = groups.map((group) => ({
    group,
    entries: docs.filter((d) => d.group === group).sort(aliasesLast),
  }))

  // A function whose group is not in the declared list would otherwise vanish
  // silently. Collect any such into a final section rather than losing them.
  const known = new Set(groups)
  const orphans = docs.filter((d) => !known.has(d.group)).sort(aliasesLast)
  if (orphans.length) sections.push({ group: 'Other', entries: orphans })

  return sections.filter((s) => s.entries.length > 0)
}

/** Real entries first, then alternative spellings, each keeping engine order. */
function aliasesLast(a, b) {
  return Number(Boolean(a.alias_of)) - Number(Boolean(b.alias_of))
}

/**
 * Operators grouped into precedence levels, tightest first.
 *
 * The engine returns them already ordered — a test in dealer-parser enforces
 * it — so this only has to break the run into levels, not sort it.
 */
export function operatorLevels(info) {
  const levels = []
  for (const doc of info.operator_docs ?? []) {
    const last = levels[levels.length - 1]
    if (last && last.precedence === doc.precedence) last.entries.push(doc)
    else levels.push({ precedence: doc.precedence, entries: [doc] })
  }
  return levels
}

/**
 * Statements that have a keyword, and the forms that do not, kept apart.
 *
 * Assignment and a bare expression are statements too, but a reader looking
 * for "the keywords" should not have to pick them out of a list they are not
 * part of.
 */
export function statementSections(info) {
  const docs = info.statement_docs ?? []
  return {
    keyword: docs.filter((d) => d.keyword),
    other: docs.filter((d) => !d.keyword),
  }
}

/** Everything a search should look at, for one entry of any kind. */
function searchableText(entry) {
  return [
    entry.name,
    entry.symbol,
    entry.word,
    entry.keyword,
    entry.signature,
    entry.form,
    entry.summary,
    entry.example,
    entry.note,
    entry.instead,
    entry.alias_of,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}

/** Whether one entry matches a query. An empty query matches everything. */
export function matches(entry, query) {
  const q = query.trim().toLowerCase()
  if (!q) return true
  // Every word must appear, so "north shape" narrows rather than widens.
  return q.split(/\s+/).every((word) => searchableText(entry).includes(word))
}

/**
 * The whole payload narrowed to entries matching `query`.
 *
 * Returns the same shape as the input, so the page renders filtered and
 * unfiltered views through one path rather than two.
 */
export function filterInfo(info, query) {
  const keep = (list) => (list ?? []).filter((e) => matches(e, query))
  return {
    ...info,
    function_docs: keep(info.function_docs),
    operator_docs: keep(info.operator_docs),
    statement_docs: keep(info.statement_docs),
    action_docs: keep(info.action_docs),
    not_supported: keep(info.not_supported),
  }
}

/** How many entries a filtered payload holds, for an "N results" line. */
export function countEntries(info) {
  return (
    (info.function_docs?.length ?? 0) +
    (info.operator_docs?.length ?? 0) +
    (info.statement_docs?.length ?? 0) +
    (info.action_docs?.length ?? 0) +
    (info.not_supported?.length ?? 0)
  )
}

/**
 * Compass and vulnerability words, for the short "words the language knows"
 * list.
 *
 * The single letters `n`, `s`, `e` and `w` are valid positions but are also
 * ordinary variable names, and the evaluator lets a variable shadow them —
 * the same reason the editor does not colour them. Shown, but noted.
 */
export function wordLists(info) {
  const positions = info.positions ?? []
  return [
    {
      title: 'Compass',
      words: positions.filter((p) => p.length > 1),
      note:
        positions.some((p) => p.length === 1)
          ? `Also ${positions
              .filter((p) => p.length === 1)
              .join(', ')} — though a variable of the same name takes precedence.`
          : null,
    },
    { title: 'Vulnerability', words: info.vulnerabilities ?? [], note: null },
    { title: 'Other keywords', words: info.other_keywords ?? [], note: null },
  ].filter((list) => list.words.length > 0)
}

/**
 * Splits prose on backtick spans, so `hcp` in a description renders as code.
 *
 * The descriptions are written in Rust source where backticks are the natural
 * way to mark a function name, and they read well there. Rendering them raw put
 * literal backticks on the page. This is deliberately not a markdown parser —
 * backticks are the only markup the descriptions use, and pulling in a parser
 * to handle one construct would be worse than the construct.
 *
 * Returns `[{ text, code }]`. An unclosed backtick is treated as literal text
 * rather than swallowing the rest of the sentence.
 */
export function codeSpans(text) {
  if (!text) return []
  const parts = []
  let rest = text

  while (rest.length) {
    const open = rest.indexOf('`')
    if (open === -1) break

    const close = rest.indexOf('`', open + 1)
    if (close === -1) break

    if (open > 0) parts.push({ text: rest.slice(0, open), code: false })
    parts.push({ text: rest.slice(open + 1, close), code: true })
    rest = rest.slice(close + 1)
  }

  if (rest.length) parts.push({ text: rest, code: false })
  return parts
}
