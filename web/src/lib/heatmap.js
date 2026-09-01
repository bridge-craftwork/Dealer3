// A two-dimensional `frequency` is a cross-tabulation, so it is drawn as one:
// a grid of counts shaded by magnitude. Both axes run low-outliers, then each
// value in the range, then high-outliers — the same shape the terminal prints.
//
// The arithmetic lives here rather than in the component so it can be tested
// without a DOM, which is how the rest of this app is arranged.

/// One hue, light to dark, which is what a sequential scale is for — magnitude,
/// not identity. Steps 100 to 700 of the blue ramp.
export const HEAT_STEPS = [
  '#cde2fb',
  '#9ec5f4',
  '#6da7ec',
  '#3987e5',
  '#256abf',
  '#184f95',
  '#0d366b',
]

/// Below this step, dark ink clears 4.5:1 on the fill; from it, white does.
/// The counts are the point of the grid, so they stay readable either way.
const INK_FLIP = 4

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
