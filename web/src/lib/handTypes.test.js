import { describe, it, expect } from 'vitest'
import { handTypeColor, handTypeTint, handTypePalette } from './handTypes.js'

describe('handTypeColor', () => {
  it('gives each of the first ten types its own colour', () => {
    const seen = new Set()
    for (let i = 0; i < 10; i++) seen.add(handTypeColor(i))
    expect(seen.size).toBe(10)
  })

  it('is stable for an index', () => {
    expect(handTypeColor(3)).toBe(handTypeColor(3))
  })

  it('wraps rather than running out', () => {
    expect(handTypeColor(10)).toBe(handTypeColor(0))
    expect(handTypeColor(-1)).toBe(handTypeColor(9))
  })

  it('avoids the two hues the bars already use', () => {
    // Orange is the natural share and blue the delivered one. A label in
    // either would look like it was making the bar's claim.
    for (let i = 0; i < 10; i++) {
      const hue = Number(handTypeColor(i).match(/hsl\((\d+)/)[1])
      expect(hue < 20 || hue > 45).toBe(true)
      expect(hue < 200 || hue > 225).toBe(true)
    }
  })
})

describe('handTypeTint', () => {
  it('shares its hue with the label colour', () => {
    const hue = (css) => css.match(/hsl\((\d+)/)[1]
    expect(hue(handTypeTint(2))).toBe(hue(handTypeColor(2)))
  })

  it('is translucent, so it tints rather than covers', () => {
    expect(handTypeTint(0)).toContain('/ 0.13')
  })
})

describe('handTypePalette', () => {
  const palette = handTypePalette(['12_14', '15_17', '18_up'])

  it('assigns by declaration order', () => {
    expect(palette.get('12_14').index).toBe(0)
    expect(palette.get('18_up').index).toBe(2)
    expect(palette.get('15_17').color).toBe(handTypeColor(1))
  })

  it('gives an untyped deal something neutral rather than a colour', () => {
    // A deal matching no type is not an error; only levelling insists the
    // types cover everything.
    for (const missing of [null, undefined, 'not_declared']) {
      expect(palette.get(missing).index).toBe(-1)
      expect(palette.get(missing).tint).toBe('transparent')
    }
  })
})
