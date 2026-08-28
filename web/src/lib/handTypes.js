// Colours for a script's hand types.
//
// The same type has to be recognisable in three places at once — its row in the
// hand-types table, the badge on a board, and the tint of a line in the text
// view — because what those three together show is that the deals come out
// walking through the types rather than in the order they happened to fall.
//
// Assigned by declaration order rather than by hashing the name. A script's
// types are usually a ladder, so in order they read as one; and a name changed
// in the editor should not repaint every other row.

/// Eight colours, chosen by measured perceptual distance rather than by picking
/// hues that look far apart written down.
///
/// The first attempt spread hues evenly around the wheel and tested that they
/// were. They were, and two pairs were still hard to tell apart on screen:
/// hue is wildly uneven perceptually, and 95° and 165° are both simply green.
/// These maximise the smallest CIEDE2000 distance between any two of them, and
/// between any of them and the two colours the bars use — a label in the
/// natural orange or the levelled blue would look like it was making the bar's
/// claim. The worst pair among the first five is 27.5 where the hue-spaced set
/// managed 18.9.
///
/// Ordered so that any prefix is as separated as it can be, since a script with
/// three types uses the first three. It degrades past five, there being only so
/// much room left once orange and blue are spoken for.
const PALETTE = [
  [114, 82, 6], // dark gold
  [160, 4, 4], // deep red
  [15, 98, 30], // forest green
  [219, 34, 119], // magenta
  [113, 19, 176], // violet
  [139, 108, 174], // lilac
  [117, 128, 2], // olive
  [183, 97, 85], // clay
]

function at(index) {
  return PALETTE[((index % PALETTE.length) + PALETTE.length) % PALETTE.length]
}

/// Dark enough to read as bold text on the page's white.
export function handTypeColor(index) {
  const [r, g, b] = at(index)
  return `rgb(${r} ${g} ${b})`
}

/// The same colour as a wash, for a badge or a highlighted line.
export function handTypeTint(index, alpha = 0.15) {
  const [r, g, b] = at(index)
  return `rgb(${r} ${g} ${b} / ${alpha})`
}

/**
 * Look a type up by name, in one pass.
 *
 * Returns `{ color, tint, index }` for a name, or a neutral entry for a deal
 * that matched no type — which is not an error, only levelling insists the
 * types cover everything.
 */
export function handTypePalette(names) {
  const byName = new Map()
  names.forEach((name, i) => {
    byName.set(name, { index: i, color: handTypeColor(i), tint: handTypeTint(i) })
  })
  return {
    get(name) {
      if (name == null) return { index: -1, color: 'var(--fg-muted)', tint: 'transparent' }
      return (
        byName.get(name) || { index: -1, color: 'var(--fg-muted)', tint: 'transparent' }
      )
    },
  }
}

/// The palette as `[r, g, b]`, for a test that wants to measure it.
export const HAND_TYPE_RGB = PALETTE
