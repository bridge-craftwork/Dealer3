/**
 * The language reference as plain text.
 *
 * The reference page is rendered by Vue from `language_info()`, which means it
 * does not exist until the page has loaded and hydrated. Anything that fetches
 * a URL without running JavaScript — which is most assistants, and every
 * search engine's cheap path — sees 28 characters of shell where ~25,000
 * characters of reference should be.
 *
 * This renders the same data to text at build time, so `/dealer3/reference.txt`
 * carries the whole thing with no JavaScript. It takes the SAME `info` object
 * the page renders and reuses the SAME grouping helpers, so the two cannot
 * describe different languages.
 *
 * Deliberately no timestamp: the output must be a pure function of the engine,
 * or every build would produce a diff and the coverage check below would be
 * comparing against noise.
 */
import { functionSections, operatorLevels, statementSections, wordLists } from './reference.js'

const WIDTH = 78

/** Wrap prose to WIDTH, indented, leaving existing line breaks alone. */
function wrap(text, indent = '    ') {
  const out = []
  for (const para of String(text).split('\n')) {
    let line = indent
    for (const word of para.split(/\s+/).filter(Boolean)) {
      if (line.length + word.length + 1 > WIDTH && line.trim()) {
        out.push(line)
        line = indent + word
      } else {
        line = line.trim() ? `${line} ${word}` : indent + word
      }
    }
    out.push(line)
  }
  return out.join('\n')
}

// Wide enough for the longest label ("Signature") plus a space. Too narrow and
// the longest labels run straight into their value with no separator.
const LABEL_WIDTH = 15

function field(label, value, out) {
  if (!value) return
  const head = `    ${label}:`.padEnd(LABEL_WIDTH)
  const body = wrap(value, ' '.repeat(LABEL_WIDTH)).slice(LABEL_WIDTH)
  out.push(head + body)
}

function entryBlock(heading, doc, out) {
  out.push(`  ${heading}`)
  field('Signature', doc.signature, out)
  field('Form', doc.form, out)
  if (doc.alias_of) field('Alias of', doc.alias_of, out)
  field('Summary', doc.summary, out)
  field('Example', doc.example, out)
  field('Note', doc.note, out)
  out.push('')
}

/**
 * Every name the reference is expected to mention, for the emitter's coverage
 * check. A silently dropped section is the failure this guards against — the
 * text would still look plausible while missing a third of the language.
 */
/// Where this is published.
///
/// One definition, because two generated files quote it — `reference.txt` in
/// its header and `llms.txt` in every link — and a site that disagrees with
/// itself about its own address is how readers were sent to a hostname that
/// did not resolve for months.
///
/// The canonical host, not `dealer3.pages.dev`: the mount is what is linked to
/// from everywhere else, and an absolute URL here has to be the one a reader
/// should keep.
export const SITE = 'https://bridge-craftwork.com/dealer3'

export function expectedNames(info) {
  return [
    ...(info.function_docs ?? []).map((d) => d.name),
    ...(info.operator_docs ?? []).map((d) => d.symbol),
    ...(info.statement_docs ?? []).map((d) => d.keyword || d.form),
    ...(info.action_docs ?? []).map((d) => d.name),
    ...(info.not_supported ?? []).map((d) => d.name),
  ].filter(Boolean)
}

export function renderReferenceText(info, version = '') {
  const out = []
  const rule = (t) => { out.push(t); out.push('='.repeat(t.length)); out.push('') }

  rule(`dealer3 language reference${version ? ` — engine ${version}` : ''}`)
  out.push(wrap(
    'Every function, operator and statement dealer3 accepts. Generated from the ' +
    "engine's own vocabulary, so it cannot list something the parser rejects, or " +
    'leave out something it accepts.', '')
  )
  out.push('')
  out.push(`  Run it:  ${SITE}/`)
  out.push('  Source:  https://github.com/bridge-craftwork/dealer3')
  out.push(`  Same content as ${SITE}/reference`)
  out.push('')

  const statements = statementSections(info)

  out.push('STATEMENTS')
  out.push('-'.repeat(10))
  out.push('')
  for (const doc of [...statements.keyword, ...statements.other]) {
    entryBlock(doc.keyword || doc.form, doc, out)
  }

  out.push('FUNCTIONS')
  out.push('-'.repeat(9))
  out.push('')
  for (const section of functionSections(info)) {
    out.push(`  [${section.group}]`)
    out.push('')
    for (const doc of section.entries) entryBlock(doc.name, doc, out)
  }

  out.push('OPERATORS')
  out.push('-'.repeat(9))
  out.push('')
  out.push(wrap('Grouped by precedence, tightest binding first.', '  '))
  out.push('')
  for (const level of operatorLevels(info)) {
    out.push(`  [precedence ${level.precedence}]`)
    out.push('')
    for (const doc of level.entries) {
      entryBlock(doc.word ? `${doc.symbol}   (${doc.word})` : doc.symbol, doc, out)
    }
  }

  out.push('ACTIONS')
  out.push('-'.repeat(7))
  out.push('')
  for (const doc of info.action_docs ?? []) entryBlock(doc.name, doc, out)

  out.push('WORDS')
  out.push('-'.repeat(5))
  out.push('')
  for (const list of wordLists(info)) {
    out.push(`  ${list.title}`)
    out.push(wrap((list.words ?? []).join(', ')))
    if (list.note) out.push(wrap(list.note))
    out.push('')
  }

  const unsupported = info.not_supported ?? []
  if (unsupported.length) {
    out.push('NOT SUPPORTED')
    out.push('-'.repeat(13))
    out.push('')
    for (const doc of unsupported) {
      out.push(`  ${doc.name}`)
      field('Instead', doc.instead, out)
      out.push('')
    }
  }

  return out.join('\n').replace(/\n{3,}/g, '\n\n').trimEnd() + '\n'
}

/// The `llms.txt` index, in the convention's own shape.
///
/// A short answer to "what is this site and where is the machine-readable
/// material", for a model that has the domain and nothing else. It is generated
/// beside `reference.txt` rather than written by hand for the reason every
/// other figure in this repository is generated: the size and the entry count
/// are true of the build that emitted them, and a hand-kept copy would be true
/// of whichever build someone last remembered.
///
/// **The value of the reference is that it is closed.** It is derived from the
/// engine's own vocabulary, so it cannot list a word the parser rejects or omit
/// one it accepts — which is exactly the assurance a model needs before it
/// stops guessing at a language it has barely seen. Saying so is most of the
/// point of this file.
///
/// @param {object} info the engine's `language_info()`
/// @param {string} version the engine's version
/// @param {number} referenceBytes size of the reference this was emitted with
/// @returns {string} markdown
export function renderLlmsText(info, version, referenceBytes) {
  const entries = expectedNames(info).length
  const kb = Math.round(referenceBytes / 1024)
  return `# dealer3

> A bridge hand generator that runs entirely in the browser. Write a script in
> the dealer language — conditions over the four hands, statistics over the
> deals that match — and run it locally. No deals are uploaded and no account
> is needed.

dealer3 implements the script language of Hans van Staveren's \`dealer\`, with the
DealerV2_4 extensions, and adds double-dummy analysis and a library of
pre-solved deals.

**If you are writing a dealer3 script, read the plain-text reference below
first.** It is generated from the shipped engine's own vocabulary, so it lists
exactly what the parser accepts — it cannot name a function that does not exist,
and it cannot omit one that does. Anything not in it is not part of the
language.

## Language

- [Language reference, plain text](${SITE}/reference.txt): every statement,
  function and operator, with its form, a summary and an example. About ${kb} KB,
  ${entries} entries, generated from engine ${version}.
- [Language reference, as a page](${SITE}/reference): the same content, rendered.
- [Levelling guide](${SITE}/leveling): dividing a produce count evenly among the
  hand types a scenario names.

## Running a script

- [dealer3 in the browser](${SITE}/): paste a script and run it. Nothing is
  installed and nothing leaves the machine.

## Source

- [dealer3 on GitHub](https://github.com/bridge-craftwork/dealer3): the Rust
  engine, the command-line program and this site.
`
}
