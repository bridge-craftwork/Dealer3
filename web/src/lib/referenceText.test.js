import { describe, it, expect } from 'vitest'
import { renderReferenceText, expectedNames } from './referenceText.js'

/** A miniature `language_info()`, shaped exactly like the engine's. */
const info = {
  function_groups: ['Hand evaluation'],
  function_docs: [
    {
      name: 'hcp',
      group: 'Hand evaluation',
      signature: 'hcp(compass)',
      summary: 'High card points.',
      example: 'hcp(north) >= 12',
      alias_of: null,
      note: 'A `pointcount` statement replaces the scale.',
    },
    { name: 'hcps', group: 'Hand evaluation', summary: 'Alias.', alias_of: 'hcp', note: null },
    { name: 'stray', group: 'Undeclared group', summary: 'Orphan.', alias_of: null, note: null },
  ],
  operator_docs: [
    { symbol: '!', word: 'not', precedence: 1, summary: 'Negation.', example: 'not x', note: null },
    { symbol: '+', word: null, precedence: 2, summary: 'Addition.', example: 'a + b', note: null },
  ],
  statement_docs: [
    { keyword: 'condition', form: 'condition <expr>', summary: 'Keep a deal.', example: 'condition x', note: null },
    { keyword: null, form: 'x = <expr>', summary: 'Assignment.', example: 'x = 1', note: null },
  ],
  action_docs: [{ name: 'printall', summary: 'All four hands.', note: null }],
  positions: ['north', 'east', 'n', 'e'],
  vulnerabilities: ['none', 'all'],
  other_keywords: ['spades'],
  not_supported: [{ name: 'evalcontract', instead: 'Use `score`.' }],
}

describe('renderReferenceText', () => {
  const text = renderReferenceText(info, '9.9.9')

  it('names every entry the engine declares', () => {
    for (const name of expectedNames(info)) expect(text).toContain(name)
  })

  it('keeps a function whose group was never declared', () => {
    // functionSections() collects these into an "Other" section rather than
    // dropping them; losing one silently is the failure worth a test.
    expect(text).toContain('stray')
  })

  it('carries the engine version', () => {
    expect(text).toContain('9.9.9')
  })

  it('renders every section heading', () => {
    for (const h of ['STATEMENTS', 'FUNCTIONS', 'OPERATORS', 'ACTIONS', 'WORDS', 'NOT SUPPORTED']) {
      expect(text).toContain(h)
    }
  })

  it('shows an operator word alongside its symbol', () => {
    expect(text).toMatch(/!\s+\(not\)/)
  })

  it('labels an alias rather than presenting it as its own function', () => {
    expect(text).toMatch(/Alias of:\s+hcp/)
  })

  it('is deterministic — no timestamp, so builds do not churn', () => {
    expect(renderReferenceText(info, '9.9.9')).toBe(text)
  })

  it('wraps prose rather than emitting one long line', () => {
    for (const line of text.split('\n')) expect(line.length).toBeLessThanOrEqual(80)
  })

  it('survives an engine that declares nothing', () => {
    expect(() => renderReferenceText({}, '')).not.toThrow()
  })
})
