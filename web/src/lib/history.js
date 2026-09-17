// Scripts worked on before the current one (#97).
//
// ## The distinction that makes it a history
//
// Ten entries that are all one script mid-edit is not a history; it is the
// editor's undo stack again. So a revision is filed under a script, and what
// counts as the same script is decided in this order:
//
//   1. **Its name.** The `title` statement, or a PBS `# alias:`. Two scripts
//      with one name are one script.
//   2. **Where it came from.** Opening a PBS scenario, a demo or an entry here
//      and editing it makes revisions of that — the page says so by passing
//      the origin and the entry being edited. No threshold is involved, which
//      is the point: this covers nearly every case, exactly.
//   3. **Similarity, as a fallback,** for text with no name and no origin:
//      something typed or pasted from scratch, compared with the most recent
//      entry.
//
// One guard spans 2 and 3. Pasting a different script over the one being
// edited keeps its origin but is not a revision of it, so an origin only holds
// while the text still shares something with that entry's latest revision.
//
// ## How much is kept
//
// Each script keeps its own few revisions, and a new one pushes out only that
// script's oldest. Working on one script all afternoon therefore never costs a
// different script its place; a script leaves only when enough others have been
// used more recently.
//
// ## Storage
//
// One versioned blob, following session.js: every access guarded, and anything
// that fails validation dropped rather than half-read. Each revision is a
// document (envelope.js), so opening one applies its settings exactly as a
// shared link would.

import { readDocument } from './envelope.js'
import { identityName } from './scriptName.js'

const KEY = 'dealer3:history:v1'
const VERSION = 1

/// Scripts kept. A script past this, used least recently, is forgotten.
export const MAX_SCRIPTS = 30
/// Revisions kept per script.
export const MAX_REVISIONS = 5

/// With no name and no origin, text at least this similar to the most recent
/// entry is taken for another revision of it.
export const SAME_SCRIPT = 0.5
/// Below this, text has been replaced rather than edited, and is a new script
/// even though it arrived in an editor that had an origin.
export const REPLACED = 0.2

/// The same bound session.js and envelope.js use for a script.
const MAX_SCRIPT_CHARS = 256 * 1024

/** An empty history. */
export function emptyHistory() {
  return { v: VERSION, next: 1, entries: [] }
}

/**
 * How much of two scripts is the same, from 0 to 1.
 *
 * Compares non-blank lines, trimmed, as a multiset: the lines in common over
 * the lines in the longer script. Crude, and deliberately so — it only has to
 * tell "edited" from "replaced", and those are far apart.
 */
export function similarity(a, b) {
  const lines = (text) =>
    (text || '')
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
  const left = lines(a)
  const right = lines(b)
  if (!left.length && !right.length) return 1
  const counts = new Map()
  for (const line of left) counts.set(line, (counts.get(line) || 0) + 1)
  let shared = 0
  for (const line of right) {
    const n = counts.get(line) || 0
    if (n > 0) {
      shared += 1
      counts.set(line, n - 1)
    }
  }
  return shared / Math.max(left.length, right.length)
}

const sameName = (a, b) => !!a && !!b && a.toLowerCase() === b.toLowerCase()

/** The entry a revision belongs to, or null for a new script. */
function owningEntry(entries, { script, name, origin, lineage }) {
  const latest = (entry) => entry.revisions[0].script
  const stillThatScript = (entry) => similarity(latest(entry), script) >= REPLACED

  if (name) {
    const named = entries.find((entry) => sameName(entry.name, name))
    if (named) return named
    // A script that has just been given a title is still the one it was.
    const edited = entries.find((entry) => entry.id === lineage)
    return edited && !edited.name && stillThatScript(edited) ? edited : null
  }

  const edited = entries.find((entry) => entry.id === lineage)
  if (edited) return stillThatScript(edited) ? edited : null

  if (origin) {
    const same = entries.find((entry) => entry.origin === origin && !entry.name)
    if (same) return stillThatScript(same) ? same : null
    return null
  }

  const recent = entries[0]
  return recent && !recent.name && similarity(latest(recent), script) >= SAME_SCRIPT ? recent : null
}

/**
 * File a revision, returning the new history and the entry it went to.
 *
 * Pure: the caller saves. A revision identical to the script's latest one
 * refreshes it rather than adding a copy — running the same script twice is
 * not two revisions.
 *
 * @param {object} history as from `loadHistory`
 * @param {object} revision
 * @param {string} revision.script the script text
 * @param {object} revision.settings a document's settings (`makeDocument`)
 * @param {string} [revision.origin] where the script came from: `pbs:File`, `demo:id`
 * @param {string} [revision.lineage] the id of the entry being edited, if any
 * @param {number} now milliseconds since the epoch
 * @returns {{ history: object, id: string }}
 */
export function recordRevision(history, { script, settings, origin = '', lineage = '' }, now) {
  const name = identityName(script)
  const entries = history.entries.slice()
  const found = owningEntry(entries, { script, name, origin, lineage })
  let next = history.next

  let entry
  if (found) {
    entries.splice(entries.indexOf(found), 1)
    const revision = { script, settings, at: now }
    const revisions =
      found.revisions[0].script === script
        ? [revision, ...found.revisions.slice(1)]
        : [revision, ...found.revisions].slice(0, MAX_REVISIONS)
    entry = { ...found, name: found.name || name, revisions }
  } else {
    entry = { id: `h${next}`, name, origin, revisions: [{ script, settings, at: now }] }
    next += 1
  }

  entries.unshift(entry)
  return { history: { v: VERSION, next, entries: entries.slice(0, MAX_SCRIPTS) }, id: entry.id }
}

/**
 * Forget one script, every revision of it.
 *
 * @param {object} history
 * @param {string} id the entry's id
 * @returns {object} the history without it
 */
export function removeEntry(history, id) {
  return { ...history, entries: history.entries.filter((entry) => entry.id !== id) }
}

/** A stored revision, checked, or null. */
function readRevision(value) {
  if (!value || typeof value !== 'object') return null
  if (!Number.isFinite(value.at)) return null
  if (typeof value.script !== 'string' || value.script.length > MAX_SCRIPT_CHARS) return null
  try {
    const doc = readDocument({ v: 1, script: value.script, settings: value.settings })
    return { script: doc.script, settings: doc.settings, at: value.at }
  } catch {
    return null
  }
}

/** A stored entry, checked, or null. */
function readEntry(value) {
  if (!value || typeof value !== 'object') return null
  if (typeof value.id !== 'string' || !value.id) return null
  const revisions = Array.isArray(value.revisions)
    ? value.revisions.map(readRevision).filter(Boolean).slice(0, MAX_REVISIONS)
    : []
  if (!revisions.length) return null
  return {
    id: value.id,
    name: typeof value.name === 'string' ? value.name : '',
    origin: typeof value.origin === 'string' ? value.origin : '',
    revisions,
  }
}

/**
 * Read the history, or an empty one.
 *
 * Never throws. Storage that is blocked, a blob that is not JSON, or one from a
 * version this page does not know all read as empty; an entry that fails
 * validation is dropped and the rest kept.
 */
export function loadHistory() {
  let raw
  try {
    raw = localStorage.getItem(KEY)
  } catch {
    return emptyHistory()
  }
  if (!raw) return emptyHistory()
  try {
    const value = JSON.parse(raw)
    if (!value || value.v !== VERSION || !Array.isArray(value.entries)) return emptyHistory()
    const entries = value.entries.map(readEntry).filter(Boolean).slice(0, MAX_SCRIPTS)
    const next = Number.isInteger(value.next) && value.next >= 1 ? value.next : 1
    // An id handed out again would file a new script's revisions under an old
    // one, so `next` must be past every id kept.
    const highest = Math.max(0, ...entries.map((entry) => Number(entry.id.slice(1)) || 0))
    return { v: VERSION, next: Math.max(next, highest + 1), entries }
  } catch {
    return emptyHistory()
  }
}

/**
 * Save the history.
 *
 * When storage is full, the least recently used scripts go first until it
 * fits: losing the oldest script is better than failing to keep the newest.
 *
 * @param {object} history
 * @returns {object} what was actually kept
 */
export function saveHistory(history) {
  let kept = history
  for (;;) {
    try {
      localStorage.setItem(KEY, JSON.stringify(kept))
      return kept
    } catch {
      if (kept.entries.length <= 1) return kept // blocked, or one script too big
      kept = { ...kept, entries: kept.entries.slice(0, -1) }
    }
  }
}
