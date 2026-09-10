import { describe, it, expect, vi } from 'vitest'
import {
  runEnvelope,
  ENVELOPE_VERSION,
  DOCUMENT_VERSION,
  DOCUMENT_DEFAULTS,
  DOCUMENT_FORMATS,
  makeDocument,
  readDocument,
  paramValuesFrom,
  settingsDelta,
} from './envelope.js'
import { loadSession } from './session.js'
import { readFileSync } from 'node:fs'

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
      'ddTags',
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

describe('a document', () => {
  const page = {
    seed: 8391,
    produce: 20,
    maxGenerate: 1000000,
    format: 'oneline',
    autoLevel: false,
    roundRobin: false,
    params: [],
    ddTags: 'optimum',
    dealSource: 'library',
    newSeedEachRun: true,
    scenario: 'Sup_X_By_Advancer',
  }

  it('carries the caller settings the engine never sees', () => {
    const doc = makeDocument('condition 1\n', page)
    expect(doc.settings.dealSource).toBe('library')
    expect(doc.settings.newSeedEachRun).toBe(true)
    expect(doc.settings.scenario).toBe('Sup_X_By_Advancer')
  })

  it('narrows to the engine envelope by being read through it', () => {
    // The point of the two living in one file: there is no second list of the
    // engine's fields to keep in step. `runEnvelope` names them, so handing it
    // a document's settings drops the caller's by not asking for them — and
    // the engine refuses what it does not know, so a leak here fails a run.
    const doc = makeDocument('condition 1\n', page)
    const envelope = JSON.parse(runEnvelope(doc.script, doc.settings))
    expect(Object.keys(envelope.settings).sort()).toEqual([
      'autoLevel',
      'ddTags',
      'format',
      'maxGenerate',
      'params',
      'produce',
      'roundRobin',
      'seed',
    ])
    expect(envelope.settings.seed).toBe(8391)
  })

  it('round-trips through a reader', () => {
    const doc = makeDocument('condition hcp(north) >= 15\n', { ...page, measureSeconds: 12 })
    expect(readDocument(JSON.parse(JSON.stringify(doc)))).toEqual({
      v: DOCUMENT_VERSION,
      script: 'condition hcp(north) >= 15\n',
      settings: { ...page, measureSeconds: 12 },
    })
  })

  it('refuses a version it does not know, by name, rather than half-loading', () => {
    expect(() => readDocument({ v: 2, script: 'condition 1\n' })).toThrow(/version 2/)
    expect(() => readDocument({ v: 2, script: 'condition 1\n' })).toThrow(/newer/)
  })

  it('refuses what is not a document at all', () => {
    expect(() => readDocument(null)).toThrow(/does not hold/)
    expect(() => readDocument([1, 2])).toThrow(/does not hold/)
    expect(() => readDocument({ v: 1 })).toThrow(/no script/)
    expect(() => readDocument({ v: 1, script: '   ' })).toThrow(/no script/)
    expect(() => readDocument({ v: 1, script: 'x'.repeat(300 * 1024) })).toThrow(/larger/)
  })

  it('opens the script rather than failing over a setting it cannot use', () => {
    // A hand-edited or truncated fragment. The script is the thing that was
    // shared; refusing the whole link over `format=xml` would throw it away to
    // no purpose, and the engine would refuse the value anyway.
    const s = readDocument({
      v: 1,
      script: 'condition 1\n',
      settings: { format: 'xml', produce: -3, dealSource: 'wat', params: 'nope', nonsense: 1 },
    }).settings
    expect(s.format).toBe('oneline')
    expect(s.produce).toBe(20)
    expect(s.dealSource).toBe('random')
    expect(s.params).toEqual([])
    expect('nonsense' in s).toBe(false)
  })

  it('leaves out a seed it was not given, so the reader rolls one', () => {
    // Not defaulted to a number. A link that names no seed is about a script
    // rather than about particular hands, and every visitor should see a
    // different sample rather than all of them seeing seed 1.
    expect('seed' in readDocument({ v: 1, script: 'condition 1\n' }).settings).toBe(false)
    expect('seed' in readDocument({ v: 1, script: 'c\n', settings: { seed: -1 } }).settings).toBe(false)
    expect(readDocument({ v: 1, script: 'c\n', settings: { seed: 7 } }).settings.seed).toBe(7)
  })

  it('agrees with the session about what a setting means when nobody said', () => {
    // Two readers of the same fields — one from localStorage, one from a link.
    // They are separate on purpose (a session holds UI state a link must not
    // carry), which is exactly the shape that drifts, so this pins the overlap.
    vi.stubGlobal('localStorage', {
      getItem: () => '{}',
      setItem: () => {},
      removeItem: () => {},
    })
    const stored = loadSession()
    vi.unstubAllGlobals()
    for (const key of ['produce', 'maxGenerate', 'format', 'roundRobin', 'dealSource']) {
      expect([key, stored[key]]).toEqual([key, DOCUMENT_DEFAULTS[key]])
    }
  })
})

describe('paramValuesFrom', () => {
  it('projects the engine spelling back to the fields it was typed into', () => {
    // Miss this and a shared parameterised scenario opens with its fields
    // blank — running on the script's declared defaults, which is not the run
    // that was shared.
    expect(paramValuesFrom(['0=west', '1=15'])).toEqual({ 0: 'west', 1: '15' })
  })

  it('keeps a value containing an equals sign whole', () => {
    expect(paramValuesFrom(['2=hcp(north) >= 15'])).toEqual({ 2: 'hcp(north) >= 15' })
  })

  it('ignores anything that is not a parameter', () => {
    expect(paramValuesFrom(['west', '=x', 'x=1', undefined, 12])).toEqual({})
    expect(paramValuesFrom(undefined)).toEqual({})
  })
})

describe('settingsDelta', () => {
  it('keeps only what a link has to say', () => {
    expect(settingsDelta({ ...DOCUMENT_DEFAULTS, seed: 5, produce: 20, format: 'pbn' })).toEqual({
      seed: 5,
      format: 'pbn',
    })
  })

  it('always keeps the seed, which has no default to be equal to', () => {
    expect(settingsDelta({ seed: 0 }).seed).toBe(0)
  })

  it('drops an empty parameter list and keeps a full one', () => {
    expect('params' in settingsDelta({ params: [] })).toBe(false)
    expect(settingsDelta({ params: ['0=west'] }).params).toEqual(['0=west'])
  })
})

describe('the formats a document may name', () => {
  it('are the ones the page offers', () => {
    // A document is read back into a `<select>`, and a select holding a value
    // no option carries shows blank — so a format the menu dropped would open
    // a link with the field empty and run something else. Two lists, one of
    // them in a template, is exactly the pair that drifts.
    const app = readFileSync(new URL('../App.vue', import.meta.url), 'utf8')
    const menu = app.slice(app.indexOf('<select v-model="format">'))
    const offered = [...menu.slice(0, menu.indexOf('</select>')).matchAll(/value="([^"]+)"/g)].map(
      (m) => m[1],
    )
    expect(offered).toEqual(DOCUMENT_FORMATS)
  })
})

describe('double-dummy tags', () => {
  it('default to the encoding PBN specifies, as the command line does', () => {
    // `--dd-tags` defaults to `optimum` too, so a file saved from the page and
    // one saved from the terminal hold the same thing.
    const settings = JSON.parse(runEnvelope('c\n', { seed: 1, produce: 1 })).settings
    expect(settings.ddTags).toBe('optimum')
  })

  it('read back only the two values the page can show', () => {
    // The engine also takes `tricks` and `both`. A link carrying one would
    // arrive with no control to show it in, and would quietly change what the
    // recipient's next PBN download held.
    const read = (ddTags) =>
      readDocument({ v: 1, script: 'c\n', settings: { ddTags } }).settings.ddTags
    expect(read('none')).toBe('none')
    expect(read('optimum')).toBe('optimum')
    expect(read('both')).toBe('optimum')
    expect(read(undefined)).toBe('optimum')
  })
})

describe('what the page sends and what the engine reads', () => {
  // The bug this exists for: `generate()` takes its options apart into named
  // parameters and puts them back together for the worker, so a setting added
  // to the envelope but not to both of those lists is silently dropped — and
  // the engine, seeing no field, uses its default. The run works. It just
  // quietly ignores what was asked for, which is how a ticked box wrote
  // double-dummy tables into a file that was meant not to have them.
  const source = readFileSync(new URL('./engine.js', import.meta.url), 'utf8')
  const accepted = source.slice(
    source.indexOf('export async function generate('),
    source.indexOf('const message = await runInWorker('),
  )
  const forwarded = source.slice(
    source.indexOf('const message = await runInWorker('),
    source.indexOf('const raw = JSON.parse(message.raw)'),
  )

  const settings = Object.keys(
    JSON.parse(runEnvelope('condition 1\n', { seed: 1, produce: 1, measureSeconds: 5 })).settings,
  )

  it('finds both halves of the projection to check', () => {
    expect(accepted.length).toBeGreaterThan(200)
    expect(forwarded.length).toBeGreaterThan(50)
    expect(settings.length).toBeGreaterThan(6)
  })

  it.each(settings)('generate() accepts and forwards %s', (name) => {
    expect(accepted).toContain(`${name} =`)
    expect(forwarded).toContain(name)
  })
})
