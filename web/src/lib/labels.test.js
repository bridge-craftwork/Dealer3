import { describe, it, expect } from 'vitest'
import { labelOf, isIndented } from './labels.js'

describe('labelOf', () => {
  it('keeps the indent a script wrote', () => {
    expect(labelOf('    balanced', 'Average')).toBe('    balanced')
  })

  it('keeps an ordinary label untouched', () => {
    expect(labelOf('Openers', 'Average')).toBe('Openers')
  })

  it('falls back when there is no label at all', () => {
    expect(labelOf('', 'Average')).toBe('Average')
    expect(labelOf(undefined, 'Average')).toBe('Average')
    expect(labelOf(null, 'Average')).toBe('Average')
  })

  // A row labelled with four spaces reads as a bug, not as a heading.
  it('falls back when the label is only whitespace', () => {
    expect(labelOf('   ', 'Average')).toBe('Average')
    expect(labelOf('\t', 'Average')).toBe('Average')
  })
})

describe('isIndented', () => {
  it('is true only when leading space is doing work', () => {
    expect(isIndented('  balanced')).toBe(true)
    expect(isIndented('balanced')).toBe(false)
    expect(isIndented('   ')).toBe(false)
    expect(isIndented('')).toBe(false)
    expect(isIndented(undefined)).toBe(false)
  })

  it('does not treat a trailing space as an indent', () => {
    expect(isIndented('balanced  ')).toBe(false)
  })
})
