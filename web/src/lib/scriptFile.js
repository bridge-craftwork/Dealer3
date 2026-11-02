// Scripts as files: export and import (#100).
//
// One script is a document (envelope.js) with `lastRun` beside it; a whole
// History is `{ v, scripts: [{ name, origin, revisions: [document, ...] }] }`, so
// every script in a backup is in the same format as a share link.
//
// `lastRun` is provenance, not input: it says what a script did on the machine
// that exported it. Nothing here or in the page treats it as a prediction.
//
// What makes this a backup is that import refuses a file it cannot read rather
// than taking part of it, which is why everything is checked here, before
// `mergeScripts` is given any of it.

import { DOCUMENT_VERSION, readDocument } from './envelope.js'
import { readLastRun } from './history.js'
import { identityName } from './scriptName.js'

/** A document with its last run beside it, if it has one. */
function withLastRun(doc, lastRun) {
  return lastRun ? { ...doc, lastRun } : doc
}

/**
 * The file for the script in the editor.
 *
 * The last run goes with it only if it describes this document: same text, same
 * settings. A runtime measured with other settings is not this script's.
 *
 * @param {object} doc the editor's document (`makeDocument`)
 * @param {object} [revision] the History revision it was opened or run as
 */
export function scriptFile(doc, revision) {
  const normal = (script, settings) =>
    JSON.stringify(readDocument({ v: DOCUMENT_VERSION, script, settings }).settings)
  const same =
    revision?.lastRun &&
    revision.script === doc.script &&
    normal(revision.script, revision.settings) === normal(doc.script, doc.settings)
  return withLastRun(doc, same ? revision.lastRun : null)
}

/**
 * The file for all of History, every version of every script.
 *
 * @param {object} history as from `loadHistory`
 * @param {string} exportedAt an ISO date
 */
export function historyFile(history, exportedAt) {
  return {
    v: DOCUMENT_VERSION,
    exportedAt,
    scripts: history.entries.map((entry) => ({
      name: entry.name,
      origin: entry.origin,
      revisions: entry.revisions.map(({ script, settings, at, lastRun }) => ({
        ...withLastRun({ v: DOCUMENT_VERSION, script, settings }, lastRun),
        at,
      })),
    })),
  }
}

/** `readDocument` speaks of links; here the reader is holding a file. */
function inFileWords(message) {
  return message.replace(/this link/gi, (found) => (found[0] === 'T' ? 'This file' : 'this file'))
}

/** One revision of a file, checked. Throws if it is not a document. */
function readRevision(value, now) {
  const doc = readDocument(value)
  const revision = {
    script: doc.script,
    settings: doc.settings,
    at: Number.isFinite(value.at) ? value.at : now,
  }
  const lastRun = readLastRun(value.lastRun)
  return lastRun ? { ...revision, lastRun } : revision
}

/**
 * Read an exported file, either kind, into scripts ready for `mergeScripts`.
 *
 * @param {string} text the file's contents
 * @param {number} now the date for a revision that does not carry one
 * @returns {Array<{name: string, origin: string, revisions: object[]}>}
 * @throws {Error} with a message meant to be shown, if any of it cannot be read
 */
export function readScriptFile(text, now) {
  let value
  try {
    value = JSON.parse(text)
  } catch {
    throw new Error('This file is not JSON, so it did not come from a dealer3 export.')
  }

  try {
    if (!Array.isArray(value?.scripts)) {
      const revision = readRevision(value, now)
      return [{ name: identityName(revision.script), origin: '', revisions: [revision] }]
    }
    // Any other version is refused by readDocument, whatever else the file holds.
    if (value.v !== DOCUMENT_VERSION) readDocument(value)
    if (!value.scripts.length) throw new Error('This file holds no scripts.')
    return value.scripts.map((script, index) => {
      if (!Array.isArray(script?.revisions) || !script.revisions.length) {
        throw new Error(`Script ${index + 1} in this file has no versions.`)
      }
      return {
        name: typeof script.name === 'string' ? script.name : '',
        origin: typeof script.origin === 'string' ? script.origin : '',
        revisions: script.revisions.map((revision) => readRevision(revision, now)),
      }
    })
  } catch (e) {
    throw new Error(inFileWords(e.message))
  }
}
