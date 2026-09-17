// The Demos tab (#104): scripts chosen to show what the tool can do.
//
// PBS already shows filtering for a hand type, 350 times over, so these lean to
// analysis — averages, two-dimensional tables, the solved library.
//
// ## A demo is a document, and nothing else
//
// Each file in `src/demos/` is a document as `envelope.js` defines it, the
// same `{v, script, settings}` a share link and an export carry. There is no
// manifest: the title is the script's own `title` statement and the description
// its opening comment, so adding a demo is adding a file, and nothing beside it
// can fall out of step. Files are listed in name order, which is why they are
// numbered.
//
// Bundled rather than fetched, so a demo versions with the page and cannot 404.
//
// ## NT Ladder, the one exception
//
// The one PBS scenario worth showing here, because it is the corpus's only
// example of levelling across several hand types. Its script is not copied into
// `src/demos/`: it is imported from `examples/NT_Ladder.stock.dlr`, which is
// already in the repository and which CI already checks still reproduces
// `NT_Ladder.leveled.dlr`. A second copy would drift from that one; fetching
// PBS's could 404. Its settings are here, since a `.dlr` cannot carry them.

import { makeDocument, readDocument } from './envelope.js'
import { displayName, scriptDescription } from './scriptName.js'
import ntLadderScript from '../../../examples/NT_Ladder.stock.dlr?raw'

/**
 * Demos from documents, checked and ordered.
 *
 * A document that does not read is left out rather than breaking the tab. The
 * test that every bundled one reads is what stops that happening in practice.
 *
 * @param {Record<string, unknown>} files parsed JSON by path, as `import.meta.glob` gives
 * @param {object[]} [extras] demos built in code: `{ id, document, title?, description? }`
 * @returns {{ id: string, title: string, description: string, document: object }[]}
 */
export function buildDemos(files, extras = []) {
  const fromFiles = Object.entries(files).map(([path, value]) => ({
    id: path.split('/').pop().replace(/\.json$/, ''),
    document: value,
  }))

  const demos = []
  for (const demo of [...fromFiles, ...extras]) {
    let document
    try {
      document = readDocument(demo.document)
    } catch {
      continue
    }
    demos.push({
      id: demo.id,
      title: demo.title || displayName(document.script) || demo.id,
      description: demo.description ?? scriptDescription(document.script),
      document,
    })
  }
  return demos.sort((a, b) => a.id.localeCompare(b.id))
}

/// NT Ladder's settings: levelling on, so the five bands come out in equal
/// shares instead of the natural 58/29/8/3/1% — which is the point of it.
const NT_LADDER = {
  id: '90-nt-ladder',
  title: 'NT Ladder: levelled hand types',
  description:
    'Five HCP bands for a balanced South, dealt in equal shares rather than ' +
    'their natural 58/29/8/3/1% — levelling, from a real PBS scenario.',
  document: makeDocument(ntLadderScript, {
    produce: 20,
    maxGenerate: 1000000,
    format: 'oneline',
    autoLevel: true,
    roundRobin: false,
    dealSource: 'random',
  }),
}

const FILES = import.meta.glob('../demos/*.json', { eager: true, import: 'default' })

/** Every bundled demo, in order. */
export const DEMOS = buildDemos(FILES, [NT_LADDER])
