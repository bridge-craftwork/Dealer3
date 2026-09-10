// The one shape a run is described in.
//
// The engine takes this (`run_json`). A share link, an export file and a demo's
// manifest entry describe more than the engine does — which deal source to draw
// from, whether to roll a fresh seed each run — so a saved document is this
// plus those, and projects back down to this by dropping them. The engine
// refuses fields it does not know, deliberately, so a document must be narrowed
// before it is run rather than passed through whole.
//
// Defining both here is what keeps them from drifting: a document's engine half
// is not a copy of this shape, it is this shape.
//
// It replaced ten positional arguments across the wasm boundary. JavaScript had
// no types to check there: `produce` and `maxGenerate` are both numbers,
// `autoLevel` and `roundRobin` are both booleans, so transposing either pair
// ran perfectly. The engine now refuses an unknown field by name and refuses a
// version it does not know, which are the two mistakes that actually happen.

/// Bumped when a change would make an older reader wrong rather than merely
/// out of date. The engine refuses a version it does not recognise.
export const ENVELOPE_VERSION = 1

/**
 * Describe a run as the JSON string `run_json` takes.
 *
 * Booleans and `params` are coerced, because a checkbox nobody touched means
 * off and no parameters means none — both are ordinary absences with an
 * obvious answer.
 *
 * `seed`, `produce`, `maxGenerate` and `format` are passed through as they
 * arrive, deliberately. A run missing one of those is broken, and the engine
 * refuses it **by name** — "missing field `seed`" — which is a better outcome
 * than quietly substituting a default and returning numbers for a run nobody
 * asked for.
 *
 * `measureSeconds` is genuinely optional: left out, the engine uses its own
 * characterizing budget.
 *
 * @param {string} script the script text
 * @param {object} options the run's settings, in the page's own names
 * @returns {string} JSON, ready for `run_json`
 */
export function runEnvelope(script, options = {}) {
  const settings = {
    seed: options.seed,
    produce: options.produce,
    maxGenerate: options.maxGenerate,
    format: options.format,
    autoLevel: !!options.autoLevel,
    roundRobin: !!options.roundRobin,
    params: options.params || [],
  }
  // Omitted rather than sent as null: the engine reads an absent field as
  // "use the default budget", and `null` would have to mean the same thing in
  // one more place.
  if (options.measureSeconds != null) settings.measureSeconds = options.measureSeconds

  return JSON.stringify({ v: ENVELOPE_VERSION, script, settings })
}

// --- A document: a run somebody else can open -----------------------------
//
// The envelope above is the engine's half. A shared link, an export file and a
// demo's manifest entry all describe more than the engine does, and a document
// is that: the engine's settings plus the caller's own.
//
// The caller's two are `dealSource` and `newSeedEachRun`, and neither is a
// field the engine could act on. It infers the source from whether deals were
// handed to it, so being *told* "library" would say nothing; and it takes a
// definite seed, because a run whose seed it invented would not be
// reproducible and the report does not echo it back. Both are resolved before
// the call, by whoever is driving.
//
// `scenario` travels with them: which entry in the scenario list this came
// from, so a link opens with the list pointing at it.
//
// Narrowing is not a function here, it is `runEnvelope` itself: it names the
// engine's fields, so handing it a document's settings drops the caller's by
// not asking for them. There is deliberately no second place that lists them.
//
// What must NOT travel is UI state — `pickerOpen`, `settingsOpen`. Whether a
// panel was open is about the sender's window, not about the run.

/// Bumped when a change would make an older reader wrong rather than merely out
/// of date. A reader refuses a version it does not know rather than half-
/// reading it.
export const DOCUMENT_VERSION = 1

/// What the page's Format menu offers. A document naming something else is
/// read back as the default: the value has to reach a `<select>`, and one that
/// holds a value with no option shows blank.
export const DOCUMENT_FORMATS = ['oneline', 'printall', 'pbn', 'none']

/// What a setting means when a link does not mention it.
///
/// This is what lets a link for an unmodified scenario be `#s=Some_Scenario`
/// plus the two or three things that were actually changed, rather than ten
/// fields most of which say what the page would have done anyway.
///
/// `seed` is deliberately absent, and so is `measureSeconds`. A link that names
/// no seed is a link about a script rather than about particular hands, and the
/// reader rolls one; an absent characterizing budget means the engine's own,
/// which is a number this file would otherwise have to keep a copy of.
export const DOCUMENT_DEFAULTS = Object.freeze({
  produce: 20,
  maxGenerate: 1000000,
  format: 'oneline',
  autoLevel: false,
  roundRobin: false,
  params: [],
  dealSource: 'random',
  newSeedEachRun: false,
  scenario: '',
})

/// Well beyond any real script — the largest of the ~350 PBS scripts is ~20 kB
/// — and the same bound `session.js` uses for the same reason.
const MAX_SCRIPT_CHARS = 256 * 1024

/// A u32, as the engine and the seed field both take.
const MAX_SEED = 4294967295

/**
 * Describe a run as a document: the engine's settings plus the caller's.
 *
 * Takes the same options object `runEnvelope` does, so a caller builds one
 * from the page's state without a second projection in between.
 *
 * @param {string} script the script text
 * @param {object} options the run's settings, in the page's own names
 * @returns {{v: number, script: string, settings: object}}
 */
export function makeDocument(script, options = {}) {
  const settings = {
    seed: options.seed,
    produce: options.produce,
    maxGenerate: options.maxGenerate,
    format: options.format,
    autoLevel: !!options.autoLevel,
    roundRobin: !!options.roundRobin,
    params: options.params || [],
    dealSource: options.dealSource === 'library' ? 'library' : 'random',
    newSeedEachRun: !!options.newSeedEachRun,
  }
  if (options.measureSeconds != null) settings.measureSeconds = options.measureSeconds
  if (options.scenario) settings.scenario = options.scenario
  return { v: DOCUMENT_VERSION, script, settings }
}

/**
 * Read a document that arrived from outside — a link, a file, a manifest.
 *
 * Untrusted input: it parses or it is refused with a sentence the recipient
 * can act on. An unknown version says so rather than half-loading, which is
 * the failure worth spending a version field on — a link that opened with two
 * of its five settings applied would look like it had worked.
 *
 * Settings are read field by field and anything unrecognised falls back to
 * `DOCUMENT_DEFAULTS`, so a truncated or hand-edited fragment opens the script
 * rather than failing over a stray `format=xml`. Fields this version does not
 * know are dropped: they cannot reach the engine, which refuses what it does
 * not recognise.
 *
 * @param {unknown} value the parsed JSON
 * @returns {{v: number, script: string, settings: object}}
 * @throws {Error} with a message meant to be shown
 */
export function readDocument(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('This link does not hold a dealer3 script.')
  }
  if (value.v !== DOCUMENT_VERSION) {
    throw new Error(
      Number.isFinite(value.v) && value.v > DOCUMENT_VERSION
        ? `This link was made by a newer dealer3 — it is version ${value.v}, and this page ` +
          `reads version ${DOCUMENT_VERSION}. Reload the page and try it again.`
        : 'This link is not in a form this page recognises.',
    )
  }
  if (typeof value.script !== 'string' || !value.script.trim()) {
    throw new Error('This link carries no script.')
  }
  if (value.script.length > MAX_SCRIPT_CHARS) {
    throw new Error('The script in this link is far larger than any real one.')
  }

  const from =
    value.settings && typeof value.settings === 'object' && !Array.isArray(value.settings)
      ? value.settings
      : {}

  const settings = {
    ...DOCUMENT_DEFAULTS,
    produce: whole(from.produce, DOCUMENT_DEFAULTS.produce),
    maxGenerate: whole(from.maxGenerate, DOCUMENT_DEFAULTS.maxGenerate),
    format: DOCUMENT_FORMATS.includes(from.format) ? from.format : DOCUMENT_DEFAULTS.format,
    autoLevel: from.autoLevel === true,
    roundRobin: from.roundRobin === true,
    dealSource: from.dealSource === 'library' ? 'library' : 'random',
    newSeedEachRun: from.newSeedEachRun === true,
    scenario: typeof from.scenario === 'string' ? from.scenario : '',
    params: readParams(from.params),
  }
  // Left out rather than defaulted, both of them. An absent seed means the
  // reader rolls one; an absent budget means the engine's own.
  if (Number.isFinite(from.seed) && from.seed >= 0 && from.seed <= MAX_SEED) {
    settings.seed = Math.floor(from.seed)
  }
  if (Number.isFinite(from.measureSeconds) && from.measureSeconds >= 1) {
    settings.measureSeconds = Math.min(300, from.measureSeconds)
  }

  return { v: DOCUMENT_VERSION, script: value.script, settings }
}

/** A whole number of at least one, or the default. */
function whole(value, fallback) {
  return Number.isFinite(value) && value >= 1 ? Math.floor(value) : fallback
}

/// `--param`'s own spelling, `N=TEXT`, with `N` a single digit — which is all
/// the language has. Anything else is dropped rather than passed to the engine
/// to be refused there, where the message would be about a script the reader
/// did not write.
function readParams(value) {
  if (!Array.isArray(value)) return []
  return value.filter((spec) => typeof spec === 'string' && /^[0-9]=/.test(spec)).slice(0, 10)
}

/**
 * `['0=west', '1=15']` back to `{ 0: 'west', 1: '15' }`.
 *
 * The one conversion a document needs and the engine does not. The page keeps
 * what was typed into the parameter fields by parameter number, and projects
 * that to `--param`'s positional spelling before a run; a document carries the
 * engine's side of it, so opening one has to project back the other way. Miss
 * this and a parameterised scenario arrives with its fields blank — running,
 * on its declared defaults, and not the run that was shared.
 *
 * @param {string[]} params specs as the engine takes them
 * @returns {object} values by parameter number
 */
export function paramValuesFrom(params) {
  const values = {}
  for (const spec of readParams(params)) {
    values[Number(spec[0])] = spec.slice(2)
  }
  return values
}

/**
 * The settings a link has to state, being those that differ from the defaults.
 *
 * `seed` is always among them when the document has one: it is what makes the
 * link show the same hands, and there is no default it could be compared to.
 *
 * @param {object} settings a document's settings
 * @returns {object} the subset worth writing down
 */
export function settingsDelta(settings = {}) {
  const delta = {}
  for (const [key, value] of Object.entries(settings)) {
    if (value == null) continue
    const fallback = DOCUMENT_DEFAULTS[key]
    if (fallback === undefined) {
      delta[key] = value // seed, measureSeconds: nothing to compare against
    } else if (Array.isArray(value)) {
      if (value.length) delta[key] = value
    } else if (value !== fallback) {
      delta[key] = value
    }
  }
  return delta
}
