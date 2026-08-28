// Remembering the last thing you were working on.
//
// Deliberately ONE session, not a library: the script currently in the editor
// and the parameters beside it. Keeping a history would need naming, listing and
// deleting — a feature in its own right — and the common case is simply coming
// back to what you had open.
//
// Results are not stored. They can run to megabytes, they are reproducible from
// the script and seed in a fraction of a second, and a stored result could
// silently disagree with the script shown next to it.

const KEY = 'dealer3:session:v1'

// Well beyond any real script (the largest of the ~350 PBS scripts is ~20 kB)
// but far short of the ~5 MB localStorage typically allows.
const MAX_SCRIPT_BYTES = 256 * 1024

/**
 * Read the saved session, or null.
 *
 * Every access is guarded: localStorage throws outright in some privacy modes
 * rather than returning null, and a corrupt or outdated value should mean
 * "start fresh", never a page that fails to load.
 */
export function loadSession() {
  let raw
  try {
    raw = localStorage.getItem(KEY)
  } catch {
    return null // storage disabled or blocked
  }
  if (!raw) return null

  try {
    const v = JSON.parse(raw)
    if (!v || typeof v !== 'object') return null
    return {
      script: typeof v.script === 'string' ? v.script : '',
      seed: Number.isFinite(v.seed) ? v.seed : 1,
      produce: Number.isFinite(v.produce) ? v.produce : 20,
      maxGenerate: Number.isFinite(v.maxGenerate) ? v.maxGenerate : 1000000,
      format: typeof v.format === 'string' ? v.format : 'oneline',
      scenario: typeof v.scenario === 'string' ? v.scenario : '',
      // Left undefined rather than defaulted, so the caller can tell "never
      // chosen" from "chosen false" — auto-level ticks itself the first time a
      // script names hand types, and only until someone has had an opinion.
      autoLevel: typeof v.autoLevel === 'boolean' ? v.autoLevel : undefined,
      newSeedEachRun:
        typeof v.newSeedEachRun === 'boolean' ? v.newSeedEachRun : undefined,
    }
  } catch {
    // Malformed: drop it rather than tripping over it on every load.
    try {
      localStorage.removeItem(KEY)
    } catch {
      /* nothing useful to do */
    }
    return null
  }
}

/** Save the session. Silently does nothing when storage is unavailable. */
export function saveSession(session) {
  if (typeof session?.script === 'string' && session.script.length > MAX_SCRIPT_BYTES) {
    return // implausibly large; not worth filling the quota with
  }
  try {
    localStorage.setItem(KEY, JSON.stringify(session))
  } catch {
    // Full, or disabled. Losing the session is not worth interrupting anyone.
  }
}

export function clearSession() {
  try {
    localStorage.removeItem(KEY)
  } catch {
    /* nothing useful to do */
  }
}
