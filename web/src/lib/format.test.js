import { describe, it, expect } from 'vitest'
import { formatAverage, randomSeed } from './format.js'

describe('formatAverage', () => {
  it('leaves integers alone', () => {
    expect(formatAverage(100)).toBe('100')
    expect(formatAverage(0)).toBe('0')
  })

  it('cuts the engine full precision down to two decimals', () => {
    // What the engine actually returns for these averages.
    expect(formatAverage(15.882352941176471)).toBe('15.88')
    expect(formatAverage(9.176470588235293)).toBe('9.18')
    expect(formatAverage(5.882352941176471)).toBe('5.88')
    expect(formatAverage(23.529411764705884)).toBe('23.53')
  })

  it('drops trailing zeros so a column stays scannable', () => {
    expect(formatAverage(11.5)).toBe('11.5')
    expect(formatAverage(11.10)).toBe('11.1')
  })

  it('keeps a tiny value visible rather than rounding it to 0.00', () => {
    expect(formatAverage(0.0004)).toBe('0.0004')
    expect(formatAverage(-0.0004)).toBe('-0.0004')
  })

  it('handles negatives', () => {
    expect(formatAverage(-15.882352941176471)).toBe('-15.88')
  })

  it('does not crash on a non-finite value', () => {
    expect(formatAverage(NaN)).toBe('—')
    expect(formatAverage(Infinity)).toBe('—')
  })
})

describe('randomSeed', () => {
  it('stays inside the u32 range the engine accepts', () => {
    for (let i = 0; i < 200; i++) {
      const s = randomSeed()
      expect(Number.isInteger(s)).toBe(true)
      expect(s).toBeGreaterThanOrEqual(0)
      expect(s).toBeLessThanOrEqual(0xffffffff)
    }
  })

  it('does not return the same value every time', () => {
    const seen = new Set(Array.from({ length: 50 }, randomSeed))
    expect(seen.size).toBeGreaterThan(40)
  })
})
