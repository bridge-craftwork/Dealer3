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
  maxGenerate: 100000,
  format: 'printall',
  scenario: 'Weak_2_Bids',
}

describe('saveSession / loadSession', () => {
  it('round-trips a session', () => {
    saveSession(session)
    expect(loadSession()).toEqual(session)
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

  it('are undefined when never chosen, which is not the same as false', () => {
    // Auto-level ticks itself the first time it sees hand types, and stops
    // doing that once someone has had an opinion. It cannot tell the two apart
    // from a bare `false`.
    saveSession(session)
    expect(loadSession().autoLevel).toBeUndefined()
    expect(loadSession().newSeedEachRun).toBeUndefined()
  })
})
