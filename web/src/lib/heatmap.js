// A two-dimensional `frequency` is a cross-tabulation, so it is drawn as one:
// a grid of counts shaded by magnitude. Both axes run low-outliers, then each
// value in the range, then high-outliers — the same shape the terminal prints.
//
// The arithmetic lives here rather than in the component so it can be tested
// without a DOM, which is how the rest of this app is arranged.

/// One hue, light to dark, which is what a sequential scale is for — magnitude,
/// not identity. Steps of the blue ramp.
///
/// Step 400 (`#3987e5`) is deliberately absent. It is the one step where
/// neither ink is comfortable: dark ink clears 4.5:1 by a hair (4.64) and white
/// does not clear it at all (3.64). A cell there looks heavy in black and would
/// be measurably worse in white, so the answer is to skip the step rather than
/// to argue about its ink. Every remaining step clears 5.3:1 with the ink it
/// takes, which `heatmap.test.js` checks by computing the ratios.
export const HEAT_STEPS = [
  '#cde2fb',
  '#9ec5f4',
  '#6da7ec',
  '#256abf',
  '#184f95',
  '#0d366b',
]

/// Below this step, dark ink is the readable one; from it, white is. The
/// counts are the point of the grid, so they stay readable either way.
const INK_FLIP = 3

function axisLabels(min, max) {
  const labels = ['Low']
  for (let value = min; value <= max; value++) labels.push(String(value))
  labels.push('High')
  return labels
}

export function rowLabels(grid) {
  return axisLabels(grid.min1, grid.max1)
}

export function columnLabels(grid) {
  return axisLabels(grid.min2, grid.max2)
}

export function peak(grid) {
  return Math.max(1, ...grid.counts.flat())
}

export function rowSum(row) {
  return row.reduce((total, n) => total + n, 0)
}

export function columnSums(grid) {
  const width = grid.counts[0]?.length ?? 0
  return Array.from({ length: width }, (_, column) =>
    grid.counts.reduce((total, row) => total + row[column], 0),
  )
}

export function total(grid) {
  return grid.counts.reduce((sum, row) => sum + rowSum(row), 0)
}

/// Is this the Low or the High bucket — the ends of an axis, which hold what
/// fell outside the script's range?
///
/// Their margins keep an explicit zero where an in-range margin is blanked. A
/// blank there would be ambiguous, and the question it answers is one a reader
/// actually has: a zero says the range caught everything, where nothing at all
/// could equally mean the bucket does not apply.
export function isOutlierBucket(index, length) {
  return index === 0 || index === length - 1
}

/// The fill and ink for one cell.
///
/// Zero is left unfilled rather than given the lightest step: on a sparse grid
/// "none here" should read as nothing, not as a faint something. The darkest
/// step is reserved for the busiest cell, so the ramp always spans the data
/// rather than whatever fraction of it happens to be used.
export function cellStyle(count, busiest) {
  if (!count) return {}
  const step = Math.min(
    HEAT_STEPS.length - 1,
    Math.ceil((count / busiest) * HEAT_STEPS.length) - 1,
  )
  return {
    background: HEAT_STEPS[step],
    color: step >= INK_FLIP ? '#ffffff' : 'var(--fg)',
  }
}
