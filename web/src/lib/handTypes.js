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

/// Hues far enough apart to tell at a glance, deliberately avoiding two bands.
///
/// Orange is the natural share and blue the delivered one, and a label in
/// either would look like it was making the same claim as the bar beside it.
/// So no hue between 20° and 45°, and none between 200° and 225°.
const HUES = [275, 165, 330, 95, 190, 310, 135, 250, 15, 60]

/// Dark enough to read as text on the page's white, saturated enough to tell
/// apart. The badge uses the same hue much lighter, for a tint rather than a
/// block of colour.
export function handTypeColor(index) {
  const hue = HUES[((index % HUES.length) + HUES.length) % HUES.length]
  return `hsl(${hue} 62% 36%)`
}

/// The same hue as a wash, for a badge or a highlighted line.
export function handTypeTint(index, alpha = 0.13) {
  const hue = HUES[((index % HUES.length) + HUES.length) % HUES.length]
  return `hsl(${hue} 62% 46% / ${alpha})`
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
