// Turning a deal's double-dummy table into display rows.
//
// VENDORED from bridge-solver's `web/src/lib/ddTable.js`, which vendored it in
// turn from Bridge-Classroom's `utils/handAnalysis.js` (`buildDdRows`,
// `collapseDdRows`). The row order is identical in all three, and so is the
// merge, which is the part worth sharing: a double-dummy table is read with
// partners adjacent, and a partnership that takes the same tricks reads better
// as one row than as two identical ones.
//
// Two things are different here. The first is not about dealer3 at all:
//
//   - **The columns run `NT, S, H, D, C`**, which is the standard order and not
//     the `C, D, H, S, NT` all three of those apps draw. That came from the BBO
//     helper extension and spread by being copied; it is not continued here.
//     See [`DISPLAY_STRAINS`].
//   - **No contract.** The solver marks the cell the contract landed on and
//     says whether it made. A generated deal has no auction and no declarer, so
//     `isContract`, `contractTarget` and the strain-matching behind them are
//     dropped rather than carried as dead weight. A cell here is a number.
//
// **A table is all twenty cells or it is not drawn** (#129). The engine hands
// over `null` for any deal it does not know in full, so that rule is mostly
// enforced before this file sees anything; [`buildDdRows`] checks again because
// a grid of the wrong shape is worth refusing at the point it is read rather
// than rendering as a table of confident zeros.
//
// The rule is a choice, and the alternative was tried. A script asking
// `tricks(north, notrump)` genuinely knows that one cell, and drawing it gives
// a table with one number and nineteen blanks, which reads as broken rather
// than as precise. Filling the rest in would mean nineteen searches a deal at
// roughly ten milliseconds each — the display deciding how long a run takes.
// So the tables drawn are the ones the run already has: every deal from the
// pre-solved library, and any script whose words need the whole table anyway
// (`par`, `trix`).

/** Row order of the engine's grid, seating order. */
const ENGINE_SEATS = ['N', 'E', 'S', 'W']

/** Column order of the engine's grid, low strain first. */
const ENGINE_STRAINS = ['C', 'D', 'H', 'S', 'NT']

/** Row order of the display: partners adjacent. */
export const DISPLAY_SEATS = ['N', 'S', 'E', 'W']

/**
 * Column order of the display: notrump, then spades down to clubs.
 *
 * The standard one. It is what PBN's two encodings both use — see
 * `bridge-encodings`' `pbn::dd::COLUMN_ORDER`, verified byte for byte against
 * bridgewebs BSOL — so the table on screen reads the same way as the
 * `[OptimumResultTable]` this program writes into a PBN.
 *
 * It is deliberately **not** what the apps this was vendored from draw.
 * bridge-solver and Bridge-Classroom both display `♣ ♦ ♥ ♠ NT`, inherited from
 * the BBO helper extension, and dealer3 does not continue that.
 */
export const DISPLAY_STRAINS = ['NT', 'S', 'H', 'D', 'C']

/**
 * Build the display rows from the engine's grid.
 *
 * `tricks` is `tricks[seat][strain]` with rows [`ENGINE_SEATS`] and columns
 * [`ENGINE_STRAINS`]. Returns `{ seat, cells }` per seat in [`DISPLAY_SEATS`],
 * each row's cells in [`DISPLAY_STRAINS`] — so both axes are turned here, and
 * no caller downstream has to know the engine's order at all.
 *
 * Returns `null` for anything that is not a complete, well-formed grid. A
 * half-built one would otherwise render as a table of confident zeros — a table
 * claiming every contract makes nothing — and no table is the honest rendering
 * of that.
 */
export function buildDdRows(tricks) {
  if (!Array.isArray(tricks) || tricks.length !== ENGINE_SEATS.length) return null
  const complete = (row) =>
    Array.isArray(row) &&
    row.length === ENGINE_STRAINS.length &&
    row.every((cell) => Number.isInteger(cell))
  if (!tricks.every(complete)) return null

  return DISPLAY_SEATS.map((seat) => {
    const row = tricks[ENGINE_SEATS.indexOf(seat)]
    return {
      seat,
      cells: DISPLAY_STRAINS.map((strain) => row[ENGINE_STRAINS.indexOf(strain)]),
    }
  })
}

/**
 * Merge a partnership's rows when their trick counts match — `N`+`S` → `NS`.
 *
 * Lossless by construction: a pair only merges when every cell already agrees,
 * so nothing is hidden, and it halves the height in the common case, since the
 * two hands of a partnership usually take the same tricks double dummy. When
 * they differ — the interesting case, and the one worth looking at — all four
 * rows stay.
 */
export function collapseDdRows(rows) {
  if (!rows) return rows
  const bySeat = Object.fromEntries(rows.map((r) => [r.seat, r]))

  const same = (a, b) =>
    a && b && a.cells.length === b.cells.length && a.cells.every((c, i) => c === b.cells[i])

  const out = []
  for (const [one, other, pair] of [
    ['N', 'S', 'NS'],
    ['E', 'W', 'EW'],
  ]) {
    const a = bySeat[one]
    const b = bySeat[other]
    if (same(a, b)) out.push({ seat: pair, cells: [...a.cells] })
    else for (const row of [a, b]) if (row) out.push(row)
  }
  return out
}

/**
 * The engine's grid for one deal, ready to draw, or `null` when the deal has
 * no table.
 *
 * The pair of calls above is always wanted together, so this is what a
 * component uses.
 */
export function ddRows(tricks) {
  const rows = collapseDdRows(buildDdRows(tricks))
  return rows && rows.length ? rows : null
}
