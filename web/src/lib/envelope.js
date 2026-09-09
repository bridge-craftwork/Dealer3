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
