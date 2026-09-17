// The `# key: value` headers PBS scenarios carry, in one place.
//
// Kept out of dlrLanguage.js so that code with no editor in it — History telling
// scripts apart, Demos titling one — can read them without importing CodeMirror.

/**
 * The `# key: value` headers a PBS scenario carries.
 *
 * Hand-kept, unlike everything else here, because these belong to PBS and not
 * to the engine — nothing in dealer reads them, so there is no vocabulary to
 * derive them from.
 *
 * What the list buys is the misspelling. A key PBS reads is coloured; anything
 * else falls through to an ordinary comment, so `# scenario-titel:` simply does
 * not light up, where before it looked exactly like one that works.
 * Colouring the good rather than marking the bad, deliberately: PBS may add a
 * key before this list hears of it, and a new key reading as a plain comment is
 * a smaller lie than a new key marked wrong.
 */
export const METADATA_KEYS = [
  'alias',
  'auction-filter',
  'bba-works',
  'button-text',
  'convention-card',
  'convention-card-ew',
  'convention-card-ns',
  'gib-works',
  'scenario-title',
]
