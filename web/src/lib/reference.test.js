import { describe, it, expect } from 'vitest'
import {
  functionSections,
  operatorLevels,
  statementSections,
  matches,
  filterInfo,
  countEntries,
  wordLists,
  codeSpans,
} from './reference.js'

// Shaped like the engine's language_info() output, cut down to what these
// transforms read.
const info = {
  function_groups: ['Hand evaluation', 'Suit length'],
  positions: ['north', 'south', 'east', 'west', 'n', 's', 'e', 'w'],
  vulnerabilities: ['none', 'ns', 'ew', 'all'],
  other_keywords: ['any', 'deal'],
  function_docs: [
    {
      name: 'hcp',
      group: 'Hand evaluation',
      signature: 'hcp(compass)',
      summary: 'High card points.',
      example: 'hcp(north) >= 12',
      alias_of: null,
      note: null,
    },
    {
      name: 'loser',
      group: 'Hand evaluation',
      signature: 'loser(compass)',
      summary: 'Singular spelling of `losers`.',
      example: 'loser(south) <= 6',
      alias_of: 'losers',
      note: null,
    },
    {
      name: 'losers',
      group: 'Hand evaluation',
      signature: 'losers(compass)',
      summary: 'Losing trick count.',
      example: 'losers(south) <= 6',
      alias_of: null,
      note: null,
    },
    {
      name: 'spades',
      group: 'Suit length',
      signature: 'spades(compass)',
      summary: 'Number of spades held.',
      example: 'spades(north) >= 5',
      alias_of: null,
      note: null,
    },
  ],
  operator_docs: [
    { symbol: '!', word: 'not', precedence: 1, summary: 'Negation.', example: 'not x', note: null },
    { symbol: '*', word: null, precedence: 2, summary: 'Multiply.', example: 'a * 2', note: null },
    { symbol: '/', word: null, precedence: 2, summary: 'Divide.', example: 'a / 2', note: null },
    { symbol: '+', word: null, precedence: 3, summary: 'Add.', example: 'a + b', note: null },
  ],
  statement_docs: [
    {
      keyword: 'condition',
      form: 'condition <expression>',
      summary: 'Keep a deal.',
      example: 'condition hcp(north) >= 15',
      note: null,
    },
    {
      keyword: null,
      form: '<name> = <expression>',
      summary: 'Names an expression.',
      example: 'fit = spades(north)',
      note: null,
    },
  ],
  action_docs: [{ name: 'printall', summary: 'All four hands.', note: null }],
  not_supported: [{ name: 'notrumps', instead: 'Write the number 4.' }],
}

describe('functionSections', () => {
  it('follows the order the engine declares its groups in', () => {
    expect(functionSections(info).map((s) => s.group)).toEqual([
      'Hand evaluation',
      'Suit length',
    ])
  })

  it('puts alternative spellings after the functions they point at', () => {
    const [handEval] = functionSections(info)
    expect(handEval.entries.map((e) => e.name)).toEqual(['hcp', 'losers', 'loser'])
  })

  it('drops a declared group with nothing in it, rather than showing an empty heading', () => {
    const sections = functionSections({ ...info, function_groups: [...info.function_groups, 'Empty'] })
    expect(sections.map((s) => s.group)).not.toContain('Empty')
  })

  it('keeps a function whose group the engine did not declare', () => {
    // Otherwise adding a group in Rust and forgetting FUNCTION_GROUPS would
    // silently drop functions from the page.
    const stray = { name: 'imps', group: 'Scoring', signature: 'imps(x)', summary: 'IMPs.', example: 'imps(1)' }
    const sections = functionSections({ ...info, function_docs: [...info.function_docs, stray] })
    const other = sections.find((s) => s.group === 'Other')
    expect(other.entries.map((e) => e.name)).toEqual(['imps'])
  })

  it('reads an empty payload without throwing', () => {
    expect(functionSections({})).toEqual([])
  })
})

describe('operatorLevels', () => {
  it('groups adjacent operators sharing a precedence', () => {
    expect(operatorLevels(info).map((l) => [l.precedence, l.entries.map((e) => e.symbol)])).toEqual([
      [1, ['!']],
      [2, ['*', '/']],
      [3, ['+']],
    ])
  })

  it('preserves the engine order rather than re-sorting', () => {
    const levels = operatorLevels(info)
    expect(levels.map((l) => l.precedence)).toEqual([1, 2, 3])
  })
})

describe('statementSections', () => {
  it('separates keyword statements from the forms that have no keyword', () => {
    const { keyword, other } = statementSections(info)
    expect(keyword.map((d) => d.keyword)).toEqual(['condition'])
    expect(other.map((d) => d.form)).toEqual(['<name> = <expression>'])
  })
})

describe('matches', () => {
  const hcp = info.function_docs[0]

  it('matches an empty query', () => {
    expect(matches(hcp, '')).toBe(true)
    expect(matches(hcp, '   ')).toBe(true)
  })

  it('ignores case', () => {
    expect(matches(hcp, 'HCP')).toBe(true)
  })

  it('looks at the summary and the example, not only the name', () => {
    expect(matches(hcp, 'high card')).toBe(true)
    expect(matches(hcp, 'north')).toBe(true)
  })

  it('narrows on each extra word rather than widening', () => {
    expect(matches(hcp, 'hcp points')).toBe(true)
    expect(matches(hcp, 'hcp spades')).toBe(false)
  })

  it('searches an operator by its word form', () => {
    expect(matches(info.operator_docs[0], 'not')).toBe(true)
  })
})

describe('filterInfo', () => {
  it('narrows every kind of entry at once', () => {
    const filtered = filterInfo(info, 'losers')
    expect(filtered.function_docs.map((d) => d.name)).toEqual(['loser', 'losers'])
    expect(filtered.operator_docs).toEqual([])
  })

  it('keeps the fields the page reads besides the entry lists', () => {
    const filtered = filterInfo(info, 'nothing at all')
    expect(filtered.function_groups).toEqual(info.function_groups)
    expect(countEntries(filtered)).toBe(0)
  })

  it('returns everything for an empty query', () => {
    expect(countEntries(filterInfo(info, ''))).toBe(countEntries(info))
  })
})

describe('wordLists', () => {
  it('shows the spelled-out compass words and notes the single letters', () => {
    const [compass] = wordLists(info)
    expect(compass.words).toEqual(['north', 'south', 'east', 'west'])
    expect(compass.note).toContain('n, s, e, w')
  })

  it('omits a list the engine sent nothing for', () => {
    const titles = wordLists({ positions: ['north'] }).map((l) => l.title)
    expect(titles).toEqual(['Compass'])
  })
})

describe('codeSpans', () => {
  it('marks backticked spans as code and leaves the rest as prose', () => {
    expect(codeSpans('Write `controls` instead.')).toEqual([
      { text: 'Write ', code: false },
      { text: 'controls', code: true },
      { text: ' instead.', code: false },
    ])
  })

  it('handles several spans in one sentence', () => {
    expect(codeSpans('`a` and `b`').filter((p) => p.code).map((p) => p.text)).toEqual(['a', 'b'])
  })

  it('handles a span at the very start and end', () => {
    expect(codeSpans('`hcp`')).toEqual([{ text: 'hcp', code: true }])
  })

  it('leaves an unclosed backtick as literal text rather than eating the sentence', () => {
    // Otherwise a typo in a Rust description would silently blank the rest of
    // the line on the page.
    expect(codeSpans('a stray ` backtick')).toEqual([{ text: 'a stray ` backtick', code: false }])
  })

  it('returns prose untouched when there is no markup', () => {
    expect(codeSpans('Number of spades held.')).toEqual([
      { text: 'Number of spades held.', code: false },
    ])
  })

  it('reads empty and missing text without throwing', () => {
    expect(codeSpans('')).toEqual([])
    expect(codeSpans(null)).toEqual([])
    expect(codeSpans(undefined)).toEqual([])
  })
})
