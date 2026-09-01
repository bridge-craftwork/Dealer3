import { describe, it, expect } from 'vitest'
import {
  HEAT_STEPS,
  rowLabels,
  columnLabels,
  peak,
  rowSum,
  columnSums,
  total,
  cellStyle,
} from './heatmap.js'

// The grid the engine hands over for
// `frequency "hcp vs spades" (hcp(north), 8, 12, spades(north), 3, 5)`:
// seven rows (Low, 8..12, High) by five columns (Low, 3..5, High).
const grid = {
  min1: 8,
  max1: 12,
  min2: 3,
  max2: 5,
  counts: [
    [25, 27, 23, 11, 5],
    [6, 11, 4, 1, 2],
    [11, 9, 5, 0, 2],
    [9, 9, 4, 3, 2],
    [12, 9, 9, 3, 2],
    [5, 3, 5, 2, 2],
    [29, 11, 24, 13, 2],
  ],
}

describe('a two-dimensional frequency grid', () => {
  it('labels both axes with an outlier bucket at each end', () => {
    expect(rowLabels(grid)).toEqual(['Low', '8', '9', '10', '11', '12', 'High'])
    expect(columnLabels(grid)).toEqual(['Low', '3', '4', '5', 'High'])
  })

  it('has a label for every row and column it draws', () => {
    expect(rowLabels(grid)).toHaveLength(grid.counts.length)
    expect(columnLabels(grid)).toHaveLength(grid.counts[0].length)
  })

  it('agrees with the terminal on the margins', () => {
    // The numbers the reference binary printed for these deals.
    expect(grid.counts.map(rowSum)).toEqual([91, 24, 27, 27, 35, 17, 79])
    expect(columnSums(grid)).toEqual([97, 79, 74, 33, 17])
    expect(total(grid)).toBe(300)
  })

  it('sums the same total both ways round', () => {
    expect(columnSums(grid).reduce((a, b) => a + b, 0)).toBe(total(grid))
  })
})

describe('the cell shading', () => {
  it('leaves an empty cell unfilled, so nothing reads as nothing', () => {
    expect(cellStyle(0, 29)).toEqual({})
  })

  it('gives the busiest cell the darkest step', () => {
    expect(cellStyle(peak(grid), peak(grid)).background).toBe(
      HEAT_STEPS[HEAT_STEPS.length - 1],
    )
  })

  it('gives the lightest step to the smallest count, not to zero', () => {
    expect(cellStyle(1, 29).background).toBe(HEAT_STEPS[0])
  })

  it('never runs off either end of the ramp', () => {
    for (const row of grid.counts) {
      for (const count of row) {
        const style = cellStyle(count, peak(grid))
        if (count) expect(HEAT_STEPS).toContain(style.background)
      }
    }
  })

  it('rises monotonically with the count', () => {
    const busiest = peak(grid)
    let previous = -1
    for (let count = 1; count <= busiest; count++) {
      const step = HEAT_STEPS.indexOf(cellStyle(count, busiest).background)
      expect(step).toBeGreaterThanOrEqual(previous)
      previous = step
    }
  })

  it('flips the ink to white once the fill is dark', () => {
    // Dark ink on the light half, white on the dark half — the counts are the
    // point of the grid, so they have to stay readable on every fill.
    expect(cellStyle(1, 29).color).toBe('var(--fg)')
    expect(cellStyle(29, 29).color).toBe('#ffffff')
  })
})

// Contrast is computable, so it is computed rather than eyeballed. This is the
// check that keeps a future tweak to the ramp from quietly producing a cell
// whose count cannot be read on it.
function luminance(hex) {
  const h = hex.replace('#', '')
  const channels = [0, 2, 4]
    .map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

describe('every step is readable with the ink it takes', () => {
  // `--fg` in the app's tokens. The style returns the token name, so the test
  // has to know the value behind it.
  const DARK_INK = '#1b1d20'

  it.each(HEAT_STEPS)('%s clears 4.5:1', (step) => {
    const ink = HEAT_STEPS.indexOf(step) >= 3 ? '#ffffff' : DARK_INK
    expect(contrast(step, ink)).toBeGreaterThanOrEqual(4.5)
  })

  it('picks the ink with the better contrast at every step', () => {
    for (const step of HEAT_STEPS) {
      const chosen = HEAT_STEPS.indexOf(step) >= 3 ? '#ffffff' : DARK_INK
      const other = chosen === '#ffffff' ? DARK_INK : '#ffffff'
      expect(contrast(step, chosen)).toBeGreaterThan(contrast(step, other))
    }
  })

  it('omits the step where neither ink is comfortable', () => {
    // #3987e5: dark 4.64:1, white 3.64:1 — the reason it is not in the ramp.
    expect(HEAT_STEPS).not.toContain('#3987e5')
  })
})
