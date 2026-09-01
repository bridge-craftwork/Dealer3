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
  out.push('  Run it:  https://bridge-craftwork.com/dealer3/')
  out.push('  Source:  https://github.com/bridge-craftwork/dealer3')
  out.push('  Same content as https://bridge-craftwork.com/dealer3/reference')
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
