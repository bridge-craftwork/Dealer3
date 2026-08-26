// Practice-Bidding-Scenarios (PBS) repo access: the scenario menu and the .dlr
// scripts behind it.
//
// VENDORED from Bridge-Classroom/src/utils/pbsScenarios.js, trimmed to what this
// site needs (the manifest and the dealer scripts; not PBN deals, curated sets or
// the table service payloads). Kept as plain functions with no framework or
// backend coupling, matching the original.
//
// The manifest is built by PBS CI (PBS issue #167) so the whole menu arrives in
// ONE fetch instead of scanning ~400 files. raw.githubusercontent.com serves
// `Access-Control-Allow-Origin: *`, so this works from the browser with no
// backend and no build-time copying — the same way Bridge-Classroom does it.

export const PBS = {
  RAW_BASE:
    'https://raw.githubusercontent.com/bridge-craftwork/Practice-Bidding-Scenarios/main',
  MANIFEST_DIR: '/manifest',
  DLR_DIR: '/dlr',
}

/** `Some_Script_Name` -> `Some Script Name`, for scenarios with no buttonText. */
export function prettifyLabel(file) {
  return file.replace(/_/g, ' ').replace(/-/g, ' ').trim()
}

/** Suit tokens used in PBS chat/button text: !C !D !H !S -> ♣ ♦ ♥ ♠ */
export function suitSymbols(s) {
  return (s || '')
    .replace(/!C/g, '♣')
    .replace(/!D/g, '♦')
    .replace(/!H/g, '♥')
    .replace(/!S/g, '♠')
}

/** First non-empty line of a scenario's `chat`, as a one-line description. */
function firstChatLine(chat) {
  if (!chat) return ''
  const line = String(chat)
    .split(/\\n|\n/)
    .map((l) => l.trim())
    .find((l) => l && !l.startsWith('---'))
  return suitSymbols((line || '').replace(/^---\s*/, ''))
}

/**
 * Fetch the pre-built menu manifest and flatten it into sections.
 *
 * Returns `{ sections, meta }`:
 *   sections: [{ label, items: [{ file, label, description }] }]
 *   meta:     keyed by file name
 *
 * `tier` is 'release' (public), 'beta' or 'test'. This site uses release.
 */
export async function fetchScenarioManifest(tier = 'release') {
  const resp = await fetch(`${PBS.RAW_BASE}${PBS.MANIFEST_DIR}/manifest-${tier}.json`)
  if (!resp.ok) throw new Error(`Could not load the scenario list (HTTP ${resp.status})`)
  const m = await resp.json()

  const scenarios = m.scenarios || {}
  const meta = {}
  for (const [name, sc] of Object.entries(scenarios)) {
    meta[name] = {
      buttonText: suitSymbols(sc.buttonText || prettifyLabel(name)),
      description: firstChatLine(sc.chat),
    }
  }

  // The layout is a flat ordered list of major/section/row nodes. Rows carry the
  // scenario names; sections group them. `major` headings are dropped: with 20
  // sections a second level of nesting costs more than it explains.
  const sections = []
  let current = null
  for (const node of m.layout || []) {
    if (node.type === 'section') {
      current = { label: node.title, items: [] }
      sections.push(current)
    } else if (node.type === 'row' && current) {
      for (const b of node.buttons || []) {
        // `---` is a layout spacer, not a scenario. `missing` marks a layout
        // entry whose script was never published.
        const sc = scenarios[b.name]
        if (!b.name || b.name === '---' || !sc || sc.missing) continue
        current.items.push({
          file: b.name,
          label: meta[b.name].buttonText,
          description: meta[b.name].description,
        })
      }
    }
  }

  return { sections: sections.filter((s) => s.items.length), meta }
}

/** Fetch one scenario's dealer script. */
export async function fetchScenarioScript(file) {
  const resp = await fetch(`${PBS.RAW_BASE}${PBS.DLR_DIR}/${file}.dlr`)
  if (!resp.ok) throw new Error(`No dealer script for ${file} (HTTP ${resp.status})`)
  return resp.text()
}
