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
