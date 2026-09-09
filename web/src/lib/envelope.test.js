import { describe, it, expect } from 'vitest'
import { runEnvelope, ENVELOPE_VERSION } from './envelope.js'

const settings = {
  seed: 7,
  produce: 5,
  maxGenerate: 100000,
  format: 'oneline',
  autoLevel: false,
  roundRobin: false,
}

const parsed = (script, options) => JSON.parse(runEnvelope(script, options))

describe('runEnvelope', () => {
  it('carries the version, so an older engine can refuse rather than half-read it', () => {
    expect(parsed('condition 1\n', settings).v).toBe(ENVELOPE_VERSION)
  })

  it('sends the settings under the names the engine reads', () => {
    // Not a restatement of the implementation: these exact spellings are what
    // `deny_unknown_fields` checks on the other side, so a rename here that
    // was not made there fails the run rather than being ignored.
    expect(Object.keys(parsed('condition 1\n', settings).settings).sort()).toEqual([
      'autoLevel',
      'format',
      'maxGenerate',
      'params',
      'produce',
      'roundRobin',
      'seed',
    ])
  })

  it('treats an untouched checkbox as off rather than as missing', () => {
    // The page can hand over settings with these absent. The engine requires
    // them, so coercing here is what keeps an ordinary absence from being an
    // error the user cannot act on.
    const s = parsed('condition 1\n', { ...settings, autoLevel: undefined, roundRobin: undefined })
      .settings
    expect(s.autoLevel).toBe(false)
    expect(s.roundRobin).toBe(false)
  })

  it('omits the measure budget rather than sending null, so the engine uses its own', () => {
    expect('measureSeconds' in parsed('condition 1\n', settings).settings).toBe(false)
    expect('measureSeconds' in parsed('condition 1\n', { ...settings, measureSeconds: null }).settings).toBe(false)
    expect(parsed('condition 1\n', { ...settings, measureSeconds: 2.5 }).settings.measureSeconds).toBe(2.5)
  })

  it('passes a missing required setting through, so the engine names it', () => {
    // Deliberately not defaulted. A run without a seed is not a run with seed
    // zero, it is a caller that forgot, and "missing field `seed`" is a better
    // outcome than numbers for a run nobody asked for.
    const s = parsed('condition 1\n', { ...settings, seed: undefined }).settings
    expect('seed' in s).toBe(false)
  })

  it('keeps the script exactly, newlines and all', () => {
    const script = 'condition hcp(north) >= 15\naction average "h" hcp(north)\n'
    expect(parsed(script, settings).script).toBe(script)
  })
})
