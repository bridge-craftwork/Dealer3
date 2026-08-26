import { describe, it, expect } from 'vitest'
import {
  countHCP,
  sortSuitDescending,
  parseOnelineDeal,
  parseOnelineDeals,
} from './cardFormatting.js'

const DEAL =
  'n AKQT3.J6.KJ42.95 e 652.AK42.AQ87.T4 s J74.QT95.T.AK863 w 98.873.9653.QJ72'

describe('countHCP', () => {
  it('counts the Milton Work honours', () => {
    expect(countHCP(['A', 'K', 'Q', 'J'])).toBe(10)
  })
  it('ignores spot cards', () => {
    expect(countHCP(['T', '9', '2'])).toBe(0)
  })
  it('is case-insensitive', () => {
    expect(countHCP(['a', 'k'])).toBe(7)
  })
})

describe('sortSuitDescending', () => {
  it('orders high to low regardless of input order', () => {
    expect(sortSuitDescending(['4', 'A', '8', 'J'])).toEqual(['A', 'J', '8', '4'])
  })
  it('accepts 10 as well as T', () => {
    expect(sortSuitDescending(['2', '10'])).toEqual(['10', '2'])
  })
})

describe('parseOnelineDeal', () => {
  it('splits a deal into four hands', () => {
    const d = parseOnelineDeal(DEAL)
    expect(Object.keys(d)).toEqual(['north', 'east', 'south', 'west'])
    expect(d.north.spades).toEqual(['A', 'K', 'Q', 'T', '3'])
    expect(d.north.hearts).toEqual(['J', '6'])
  })

  it('computes HCP per hand', () => {
    const d = parseOnelineDeal(DEAL)
    // North: AKQ spades (9) + J hearts (1) + KJ diamonds (4) = 14
    expect(d.north.hcp).toBe(14)
    const total = d.north.hcp + d.east.hcp + d.south.hcp + d.west.hcp
    expect(total).toBe(40)
  })

  it('gives every hand thirteen cards', () => {
    const d = parseOnelineDeal(DEAL)
    for (const seat of ['north', 'east', 'south', 'west']) {
      expect(d[seat].length, seat).toBe(13)
    }
  })

  it('handles a void', () => {
    const d = parseOnelineDeal(
      'n AKQJT98765432... e .AKQJT98765432.. s ..AKQJT98765432. w ...AKQJT98765432',
    )
    expect(d.north.hearts).toEqual([])
    expect(d.north.spades).toHaveLength(13)
  })

  it('returns null for anything that is not a deal', () => {
    expect(parseOnelineDeal('Generated 156 hands')).toBeNull()
    expect(parseOnelineDeal('')).toBeNull()
    expect(parseOnelineDeal('n AKQ.J6.K42.95')).toBeNull()
  })

  it('returns null rather than showing a short deal', () => {
    // 51 cards. Rendering this would quietly display a 12-card hand, which is
    // worse than skipping the line.
    expect(parseOnelineDeal(DEAL.replace('AKQT3', 'AKQT'))).toBeNull()
  })
})

describe('parseOnelineDeals', () => {
  it('skips the statistics lines mixed in with deals', () => {
    const block = [DEAL, DEAL, 'Generated 156 hands', 'Produced 2 hands'].join('\n')
    expect(parseOnelineDeals(block)).toHaveLength(2)
  })
  it('returns an empty list for output in another format', () => {
    expect(parseOnelineDeals('   1.\nA K Q  6 5 2  J 7 4  9 8')).toEqual([])
  })
})
