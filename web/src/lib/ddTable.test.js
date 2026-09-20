import { describe, expect, it } from 'vitest'
import { buildDdRows, collapseDdRows, ddRows, DISPLAY_SEATS, DISPLAY_STRAINS } from './ddTable.js'

// A solved deal as the engine hands it over: rows N,E,S,W, columns C,D,H,S,NT.
// N and S take the same tricks, as do E and W — the usual case, and the one the
// merge is for.
//
// The display turns both axes, so every expectation below is written in the
// display's order — rows N,S,E,W and columns NT,S,H,D,C — and a transposition
// anywhere in between shows up as a number in the wrong place.
const SOLVED = [
  [8, 7, 8, 10, 9], // N
  [5, 6, 4, 3, 4], // E
  [8, 7, 8, 10, 9], // S
  [5, 6, 4, 3, 4], // W
]

/**
 * A grid knowing only the cells named, as a live-solving script leaves it.
 *
 * Addressed in the **engine's** order — `[row, column, tricks]` with rows
 * N,E,S,W and columns C,D,H,S,NT. The engine never actually ships one of these
 * (it sends `null` instead), so these stand for a caller that assembled a grid
 * by hand and got it wrong.
 */
function knowing(...cells) {
  const grid = [
    [null, null, null, null, null],
    [null, null, null, null, null],
    [null, null, null, null, null],
    [null, null, null, null, null],
  ]
  for (const [row, column, tricks] of cells) grid[row][column] = tricks
  return grid
}

describe('buildDdRows', () => {
  it('reorders the engine rows into reading order', () => {
    expect(buildDdRows(SOLVED).map((r) => r.seat)).toEqual(DISPLAY_SEATS)
    expect(DISPLAY_SEATS).toEqual(['N', 'S', 'E', 'W'])
  })

  /*
   * The order this display deliberately does not share with the apps it was
   * vendored from. They draw the suits ascending with notrump last, inherited
   * from the BBO helper extension; the standard — and what PBN's two encodings
   * use — is notrump first, then spades down to clubs.
   *
   * Distinct values in every cell, so the assertion pins the mapping rather
   * than agreeing with several of them. `SOLVED` has a repeated 8 and would
   * pass a table that muddled clubs with hearts.
   */
  it('turns the columns into the standard order, notrump first', () => {
    expect(DISPLAY_STRAINS).toEqual(['NT', 'S', 'H', 'D', 'C'])
    //                   C  D  H  S  NT   — the engine's order
    const distinct = [[1, 2, 3, 4, 5], [6, 7, 8, 9, 10], [11, 12, 13, 0, 1], [2, 3, 4, 5, 6]]
    const bySeat = Object.fromEntries(buildDdRows(distinct).map((r) => [r.seat, r.cells]))
    //                          NT S  H  D  C
    expect(bySeat.N).toEqual([5, 4, 3, 2, 1])
    expect(bySeat.E).toEqual([10, 9, 8, 7, 6])
    expect(bySeat.S).toEqual([1, 0, 13, 12, 11])
    expect(bySeat.W).toEqual([6, 5, 4, 3, 2])
  })

  it('keeps each seat cells with that seat', () => {
    const bySeat = Object.fromEntries(buildDdRows(SOLVED).map((r) => [r.seat, r.cells]))
    // Engine [C,D,H,S,NT] = [8,7,8,10,9] reads as display [NT,S,H,D,C].
    expect(bySeat.N).toEqual([9, 10, 8, 7, 8])
    expect(bySeat.S).toEqual([9, 10, 8, 7, 8])
    expect(bySeat.E).toEqual([4, 3, 4, 6, 5])
    expect(bySeat.W).toEqual([4, 3, 4, 6, 5])
  })

  /*
   * A malformed grid would render as a table of confident zeros — every
   * contract making nothing. No table is the honest rendering.
   */
  it('returns null for a grid it cannot trust', () => {
    expect(buildDdRows(null)).toBeNull()
    expect(buildDdRows([])).toBeNull()
    expect(buildDdRows([[1, 2, 3, 4, 5]])).toBeNull()
    expect(buildDdRows([[1, 2, 3], [1, 2, 3], [1, 2, 3], [1, 2, 3]])).toBeNull()
  })

  /*
   * All twenty cells or no table (#129). One known cell reads as a broken
   * table rather than as a precise statement about what the run paid for, and
   * the alternative — solving the other nineteen — would let the display
   * decide how long a run takes.
   */
  it('refuses a grid that is not complete', () => {
    expect(buildDdRows(knowing())).toBeNull()
    expect(buildDdRows(knowing([0, 4, 9]))).toBeNull()
    // Nineteen of twenty is still not a table.
    const almost = SOLVED.map((row) => [...row])
    almost[2][1] = null
    expect(buildDdRows(almost)).toBeNull()
  })
})

describe('collapseDdRows', () => {
  it('merges a partnership whose tricks match', () => {
    const rows = collapseDdRows(buildDdRows(SOLVED))
    expect(rows.map((r) => r.seat)).toEqual(['NS', 'EW'])
    expect(rows[0].cells).toEqual([9, 10, 8, 7, 8])
    expect(rows[1].cells).toEqual([4, 3, 4, 6, 5])
  })

  it('keeps all four rows when a pair differs', () => {
    const uneven = [
      [8, 7, 8, 10, 9], // N
      [5, 6, 4, 3, 4], // E
      [8, 7, 8, 10, 8], // S — one cell apart from N
      [5, 6, 4, 3, 4], // W
    ]
    // NS cannot merge; EW still can, and the two pairs are independent.
    expect(collapseDdRows(buildDdRows(uneven)).map((r) => r.seat)).toEqual(['N', 'S', 'EW'])
  })

  it('passes null through', () => {
    expect(collapseDdRows(null)).toBeNull()
  })
})

describe('ddRows', () => {
  it('is the pair of calls a caller always wants', () => {
    expect(ddRows(SOLVED)).toEqual(collapseDdRows(buildDdRows(SOLVED)))
  })

  it('is null when there is no table', () => {
    expect(ddRows(null)).toBeNull()
    expect(ddRows(knowing())).toBeNull()
    expect(ddRows(knowing([0, 4, 9]))).toBeNull()
  })

  it('never returns an empty table, which would draw as a bare header', () => {
    const rows = ddRows(SOLVED)
    expect(rows.length).toBeGreaterThan(0)
    expect(rows.every((r) => r.cells.length === DISPLAY_STRAINS.length)).toBe(true)
  })
})
