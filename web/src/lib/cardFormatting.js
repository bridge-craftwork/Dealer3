// Card and suit display primitives.
//
// VENDORED from Bridge-Classroom/src/utils/cardFormatting.js, trimmed to what a
// static deal grid needs. The full module also covers bids, vulnerability,
// auction text flow and suit-shorthand colouring, none of which applies here.
//
// HandDisplay.vue was not vendored: at 521 lines it is built for an interactive
// table — clickable cards, a card-selector popup, per-card marks and dynamic
// fit — and needs almost none of that here. These primitives were the reusable
// part.

export const SUIT_SYMBOLS = { spades: '♠', hearts: '♥', diamonds: '♦', clubs: '♣' }
export const SUIT_ORDER = ['spades', 'hearts', 'diamonds', 'clubs']

/** Red suits, for the conventional two-colour deal display. */
export const RED_SUITS = new Set(['hearts', 'diamonds'])

/** Bridge display order within a suit: high to low. */
export const RANK_ORDER = ['A', 'K', 'Q', 'J', 'T', '9', '8', '7', '6', '5', '4', '3', '2']

const HCP = { A: 4, K: 3, Q: 2, J: 1 }

/** Milton Work count for a list of ranks. */
export function countHCP(ranks) {
  return ranks.reduce((sum, r) => sum + (HCP[String(r).toUpperCase()] || 0), 0)
}

/**
 * Sort a suit's ranks into display order, tolerant of source order.
 * Accepts 'T' or '10' for the ten; unknown ranks sort to the end.
 */
export function sortSuitDescending(ranks) {
  const index = (c) => {
    const r = String(c).toUpperCase() === '10' ? 'T' : String(c).toUpperCase()
    const i = RANK_ORDER.indexOf(r)
    return i === -1 ? RANK_ORDER.length : i
  }
  return [...ranks].sort((a, b) => index(a) - index(b))
}

/**
 * Parse one deal in the engine's oneline format into hands.
 *
 *   "n AKQ.J6.K42.95 e 652.AK42.A87.T4 s J74.QT95.T.AK863 w 98.873.965.QJ72"
 *
 * Returns `{ north, east, south, west }`, each a map of suit -> rank array,
 * plus `hcp`. Returns null if the line is not a deal, so a caller can filter a
 * mixed block of output without exception handling.
 */
export function parseOnelineDeal(line) {
  const parts = String(line).trim().split(/\s+/)
  if (parts.length !== 8) return null

  const seats = { n: 'north', e: 'east', s: 'south', w: 'west' }
  const deal = {}
  for (let i = 0; i < 8; i += 2) {
    const seat = seats[parts[i].toLowerCase()]
    if (!seat) return null
    // A hand is four dot-separated suits, high to low: spades.hearts.diamonds.clubs
    const suits = parts[i + 1].split('.')
    if (suits.length !== 4) return null

    const hand = {}
    let hcp = 0
    SUIT_ORDER.forEach((suitName, idx) => {
      const ranks = sortSuitDescending((suits[idx] || '').split('').filter(Boolean))
      hand[suitName] = ranks
      hcp += countHCP(ranks)
    })
    hand.hcp = hcp
    hand.length = SUIT_ORDER.reduce((n, s) => n + hand[s].length, 0)
    deal[seat] = hand
  }

  // A deal that does not hold 52 cards means the line was malformed rather than
  // simply unrecognised, and silently showing 12-card hands would be worse than
  // skipping it.
  const total = ['north', 'east', 'south', 'west'].reduce((n, s) => n + deal[s].length, 0)
  if (total !== 52) return null

  return deal
}

/** Parse a block of oneline output, skipping anything that is not a deal. */
export function parseOnelineDeals(text) {
  return String(text)
    .split('\n')
    .map(parseOnelineDeal)
    .filter(Boolean)
}
