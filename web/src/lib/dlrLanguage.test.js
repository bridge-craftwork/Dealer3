import { describe, it, expect } from 'vitest'
import { tags } from '@lezer/highlight'
import { dlrStreamParser, dlrCompletion, DLR_TOKENS } from './dlrLanguage.js'

// Shaped like the engine's language_info() output.
const info = {
  functions: ['hcp', 'shape', 'spade', 'spades', 'top2', 'pt0', 'cccc'],
  statement_keywords: ['condition', 'produce', 'csvrpt'],
  actions: ['printall', 'printoneline'],
  positions: ['north', 'south', 'n', 's'],
  vulnerabilities: ['none', 'all'],
  logical_words: ['and', 'or', 'not'],
  other_keywords: ['any', 'deal'],
  operators: ['==', '>=', '>', '='],
}

/** Run the stream tokenizer over a line and return [token, text] pairs. */
function tokenize(line, parser = dlrStreamParser(info)) {
  const state = parser.startState()
  const out = []
  let pos = 0
  const stream = {
    pos: 0,
    string: line,
    sol() { return this.pos === 0 },
    peek() { return this.string[this.pos] },
    next() { return this.string[this.pos++] },
    eatSpace() {
      const start = this.pos
      while (/\s/.test(this.string[this.pos])) this.pos++
      return this.pos > start
    },
    skipToEnd() { this.pos = this.string.length },
    match(pattern, consume = true) {
      if (typeof pattern === 'string') {
        if (this.string.startsWith(pattern, this.pos)) {
          if (consume) this.pos += pattern.length
          return true
        }
        return false
      }
      const m = this.string.slice(this.pos).match(pattern)
      if (m && m.index === 0) {
        if (consume) this.pos += m[0].length
        return m
      }
      return null
    },
  }
  let guard = 0
  while (stream.pos < line.length && guard++ < 500) {
    pos = stream.pos
    const tok = parser.token(stream, state)
    if (stream.pos === pos) stream.pos++
    if (tok) out.push([tok, line.slice(pos, stream.pos).trim()])
  }
  return out
}

describe('dlr tokenizer', () => {
  it('marks known functions as functions', () => {
    const t = tokenize('hcp(north) >= 15')
    expect(t.find(([, text]) => text === 'hcp')[0]).toBe('function')
  })

  it('marks statement keywords as keywords', () => {
    expect(tokenize('condition x')[0][0]).toBe('keyword')
    // csvrpt was missing from the old TextMate grammar entirely.
    expect(tokenize('csvrpt(deal)')[0][0]).toBe('keyword')
  })

  it('covers point-count functions the old grammar missed', () => {
    for (const fn of ['top2', 'pt0']) {
      const t = tokenize(`${fn}(north)`)
      expect(t[0], `${fn} should tokenize`).toEqual(['function', fn])
    }
  })

  it('does not mistake a longer word for a shorter function', () => {
    // `spades` must not tokenize as `spade` + `s`.
    const t = tokenize('spades(north)')
    expect(t[0]).toEqual(['function', 'spades'])
  })

  it('leaves unknown identifiers as variables', () => {
    const t = tokenize('myOpener = 1')
    expect(t[0]).toEqual(['variableName', 'myOpener'])
  })

  it('does not colour single-letter positions', () => {
    // `n` is a valid position but also a common variable name, and the
    // evaluator lets a variable shadow it.
    expect(tokenize('n = 4')[0][0]).toBe('variableName')
    expect(tokenize('north')[0][0]).toBe('atom')
  })

  it('is case-insensitive, matching the grammar', () => {
    expect(tokenize('HCP(NORTH)')[0][0]).toBe('function')
  })

  it('recognises comments, including PBS metadata headers', () => {
    expect(tokenize('# alias: Foo')[0][0]).toBe('docComment')
    expect(tokenize('# just a comment')[0][0]).toBe('comment')
    expect(tokenize('// slash comment')[0][0]).toBe('comment')
  })

  it('recognises shape patterns before numbers', () => {
    expect(tokenize('4333')[0]).toEqual(['number', '4333'])
    expect(tokenize('5xxx')[0]).toEqual(['number', '5xxx'])
  })

  it('recognises card literals and predeal holdings', () => {
    expect(tokenize('AS')[0][0]).toBe('atom')
    expect(tokenize('SAKQ')[0][0]).toBe('atom')
  })

  it('tracks block comments across a line', () => {
    const parser = dlrStreamParser(info)
    const state = parser.startState()
    expect(state.inBlockComment).toBe(false)
  })
})

describe('dlr completion', () => {
  const complete = dlrCompletion(info)
  const context = (text, explicit = false) => ({
    explicit,
    matchBefore: (re) => {
      const m = text.match(re)
      return m ? { from: text.length - m[0].length, to: text.length, text: m[0] } : null
    },
  })

  it('offers every function from the vocabulary', () => {
    const r = complete(context('hc'))
    for (const f of info.functions) {
      expect(r.options.some((o) => o.label === f), `${f} missing`).toBe(true)
    }
  })

  it('completes functions with their parentheses', () => {
    const r = complete(context('hc'))
    expect(r.options.find((o) => o.label === 'hcp').apply).toBe('hcp()')
  })

  it('labels the kind of each suggestion', () => {
    const r = complete(context('co'))
    expect(r.options.find((o) => o.label === 'condition').detail).toBe('statement')
    expect(r.options.find((o) => o.label === 'north').detail).toBe('position')
  })

  it('returns nothing on an empty implicit request', () => {
    expect(complete(context(''))).toBeNull()
  })
})

describe('the token table', () => {
  // The tokeniser returning a name is only half of it: CodeMirror has to be
  // able to resolve that name to a highlight tag, and it silently styles
  // nothing when it cannot. `function` is a modifier in @lezer/highlight
  // rather than a tag, so it needs saying explicitly — which is why every
  // function in every script rendered as plain text while the test above,
  // asserting `hcp` tokenises as `function`, passed.
  it('maps every token name the parser emits', () => {
    const emitted = new Set()
    const parser = dlrStreamParser(info)
    const lines = [
      'x = hcp(north) >= 15 and shape(south, any 4333)',
      '# a comment',
      'action average "label" cccc(west), printall',
      'predeal north SAKQ, HT62',
      'condition top2(north, spades) == 2 ? 1 : 0',
    ]
    for (const line of lines) {
      for (const [token] of tokenize(line, parser)) if (token) emitted.add(token)
    }
    for (const name of emitted) {
      const resolvable = tags[name] !== undefined || DLR_TOKENS[name] !== undefined
      expect(resolvable, `\`${name}\` resolves to no highlight tag, so it is styled as nothing`)
        .toBe(true)
    }
    // And the one that caught this.
    expect(emitted.has('function')).toBe(true)
  })
})
