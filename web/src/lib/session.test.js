import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { loadSession, saveSession, clearSession } from './session.js'

function memoryStorage() {
  const map = new Map()
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
    _map: map,
  }
}

beforeEach(() => vi.stubGlobal('localStorage', memoryStorage()))
afterEach(() => vi.unstubAllGlobals())

const session = {
  script: 'condition hcp(north) >= 15\n',
  seed: 42,
  produce: 25,
  roundRobin: false,
  maxGenerate: 100000,
  format: 'printall',
  dealSource: 'library',
  ddTags: 'optimum',
  scenario: 'Weak_2_Bids',
  pickerOpen: true,
  settingsOpen: false,
  paramValues: { 0: 'west', 1: '15' },
}

describe('saveSession / loadSession', () => {
  it('round-trips a session', () => {
    saveSession(session)
    expect(loadSession()).toEqual(session)
  })

  it('reads a session saved before the settings panel existed as closed', () => {
    // The panel exists to get these controls off the screen. A session from
    // before it must not arrive with it open, which would undo the change for
    // everyone who was already using the page.
    const { settingsOpen, ...older } = session
    localStorage.setItem('dealer3:session:v1', JSON.stringify(older))
    expect(loadSession().settingsOpen).toBe(false)
  })

  it('reads a session saved before round robin existed as off', () => {
    // Not a defaulted `undefined`, because there is nothing to tell apart: a
    // session that never chose was taking deals as they came.
    const { roundRobin, ...older } = session
    localStorage.setItem('dealer3:session:v1', JSON.stringify(older))
    expect(loadSession().roundRobin).toBe(false)
  })

  it('reads a session saved before the library existed as random deals', () => {
    // A session with no opinion must not come back asking to download a
    // library, and nor must a stored value that no longer means anything.
    const { dealSource, ...older } = session
    localStorage.setItem('dealer3:session:v1', JSON.stringify(older))
    expect(loadSession().dealSource).toBe('random')
    localStorage.setItem('dealer3:session:v1', JSON.stringify({ ...session, dealSource: 'wat' }))
    expect(loadSession().dealSource).toBe('random')
  })

  it('reads a session saved before script parameters existed as none', () => {
    const { paramValues, ...older } = session
    localStorage.setItem('dealer3:session:v1', JSON.stringify(older))
    expect(loadSession().paramValues).toEqual({})
  })

  it('returns null when nothing is stored', () => {
    expect(loadSession()).toBeNull()
  })

  it('clears', () => {
    saveSession(session)
    clearSession()
    expect(loadSession()).toBeNull()
  })
})

describe('resilience', () => {
  it('returns null and discards a corrupt value', () => {
    localStorage.setItem('dealer3:session:v1', '{not json')
    expect(loadSession()).toBeNull()
    // Dropped, so it cannot trip over the same value on every later load.
    expect(localStorage.getItem('dealer3:session:v1')).toBeNull()
  })

  it('fills in defaults for missing or wrong-typed fields', () => {
    localStorage.setItem('dealer3:session:v1', JSON.stringify({ script: 'x', seed: 'nope' }))
    const s = loadSession()
    expect(s.script).toBe('x')
    expect(s.seed).toBe(1)
    expect(s.produce).toBe(20)
    expect(s.maxGenerate).toBe(1000000)
    expect(s.format).toBe('oneline')
  })

  it('survives storage being unavailable', () => {
    // Safari in private mode throws rather than returning null.
    vi.stubGlobal('localStorage', {
      getItem() { throw new Error('denied') },
      setItem() { throw new Error('denied') },
      removeItem() { throw new Error('denied') },
    })
    expect(loadSession()).toBeNull()
    expect(() => saveSession(session)).not.toThrow()
    expect(() => clearSession()).not.toThrow()
  })

  it('survives the quota being exceeded', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem() { throw new Error('QuotaExceededError') },
      removeItem() {},
    })
    expect(() => saveSession(session)).not.toThrow()
  })

  it('refuses an implausibly large script rather than filling the quota', () => {
    saveSession({ ...session, script: 'x'.repeat(300 * 1024) })
    expect(loadSession()).toBeNull()
  })
})

describe('the checkbox settings', () => {
  // Both were dropped by the whitelist when they were added: `autoLevel`
  // invisibly, because it re-ticks itself whenever a script names hand types,
  // and `newSeedEachRun` by quietly turning itself off on every page load.
  it('come back as they were left', () => {
    saveSession({ ...session, autoLevel: true, newSeedEachRun: true })
    expect(loadSession().autoLevel).toBe(true)
    expect(loadSession().newSeedEachRun).toBe(true)

    saveSession({ ...session, autoLevel: false, newSeedEachRun: false })
    expect(loadSession().autoLevel).toBe(false)
    expect(loadSession().newSeedEachRun).toBe(false)
  })

  // Unlike those two, the scenario list has a right answer for someone who has
  // never chosen: it is how a first visit finds anything to run. So it defaults
  // to open rather than to undefined — but a visit that closed it must not have
  // it re-open on the next load, which is what this pins.
  it('remember the scenario list being closed', () => {
    saveSession({ ...session, pickerOpen: false })
    expect(loadSession().pickerOpen).toBe(false)

    saveSession({ ...session, pickerOpen: true })
    expect(loadSession().pickerOpen).toBe(true)
  })

  it('show the scenario list to a session that predates it, or one that says nonsense', () => {
    const { pickerOpen, ...older } = session
    localStorage.setItem('dealer3:session:v1', JSON.stringify(older))
    expect(loadSession().pickerOpen).toBe(true)
    localStorage.setItem('dealer3:session:v1', JSON.stringify({ ...session, pickerOpen: 'shut' }))
    expect(loadSession().pickerOpen).toBe(true)
  })

  // Kept out of the whitelist, this came back undefined on every load and the
  // field then refilled itself from the engine's default — so a reader who had
  // set it to 60 got 20 back on their next visit, silently.
  it('remember how long to spend characterizing', () => {
    saveSession({ ...session, measureSeconds: 45 })
    expect(loadSession().measureSeconds).toBe(45)
  })

  // Undefined, not a number copied from the engine: whoever has never chosen
  // gets whatever the engine says its default is, and there is no second copy
  // of that number to go stale.
  it('leave the characterizing budget unset when nobody has set one', () => {
    saveSession(session)
    expect(loadSession().measureSeconds).toBeUndefined()
    localStorage.setItem('dealer3:session:v1', JSON.stringify({ ...session, measureSeconds: 'ages' }))
    expect(loadSession().measureSeconds).toBeUndefined()
  })

  it('are undefined when never chosen, which is not the same as false', () => {
    // Auto-level ticks itself the first time it sees hand types, and stops
    // doing that once someone has had an opinion. It cannot tell the two apart
    // from a bare `false`.
    saveSession(session)
    expect(loadSession().autoLevel).toBeUndefined()
    expect(loadSession().newSeedEachRun).toBeUndefined()
  })
})

describe('double-dummy tags', () => {
  it('reads a session saved before the box existed as writing them', () => {
    // The box is on by default, so a session from before it must not come back
    // with the tables switched off: a deal from the pre-solved library knows
    // all twenty cells, and dropping them is the whole reason for reading it.
    const { ddTags, ...older } = session
    localStorage.setItem('dealer3:session:v1', JSON.stringify(older))
    expect(loadSession().ddTags).toBe('optimum')
  })

  it('keeps an explicit no', () => {
    localStorage.setItem('dealer3:session:v1', JSON.stringify({ ...session, ddTags: 'none' }))
    expect(loadSession().ddTags).toBe('none')
  })
})
