import { describe, it, expect } from 'vitest'
import {
  handTypeColor,
  handTypeTint,
  handTypePalette,
  HAND_TYPE_RGB,
} from './handTypes.js'

// CIEDE2000, because the cheaper answers are wrong in exactly the way that
// matters here. The first palette spread hues evenly and had a test asserting
// they were evenly spread; it passed, and two of five colours were still hard
// to tell apart, because hue is nothing like uniform perceptually — 95° and
// 165° are both green. CIE76 missed it too, calling the worst pair 30 when the
// eye and CIEDE2000 both put it at 19.
function lab([r, g, b]) {
  const lin = (c) => {
    c /= 255
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  const [R, G, B] = [lin(r), lin(g), lin(b)]
  const x = (R * 0.4124 + G * 0.3576 + B * 0.1805) / 0.95047
  const y = R * 0.2126 + G * 0.7152 + B * 0.0722
  const z = (R * 0.0193 + G * 0.1192 + B * 0.9505) / 1.08883
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116)
  return [116 * f(y) - 16, 500 * (f(x) - f(y)), 200 * (f(y) - f(z))]
}

function deltaE(c1, c2) {
  const [L1, a1, b1] = lab(c1)
  const [L2, a2, b2] = lab(c2)
  const rad = Math.PI / 180
  const C1 = Math.hypot(a1, b1)
  const C2 = Math.hypot(a2, b2)
  const Cb = (C1 + C2) / 2
  const G = 0.5 * (1 - Math.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)))
  const a1p = (1 + G) * a1
  const a2p = (1 + G) * a2
  const C1p = Math.hypot(a1p, b1)
  const C2p = Math.hypot(a2p, b2)
  const h1p = (Math.atan2(b1, a1p) / rad + 360) % 360
  const h2p = (Math.atan2(b2, a2p) / rad + 360) % 360
  const dLp = L2 - L1
  const dCp = C2p - C1p
  const dhp = C1p * C2p === 0 ? 0 : (((h2p - h1p + 180) % 360) - 180)
  const dHp = 2 * Math.sqrt(C1p * C2p) * Math.sin((dhp * rad) / 2)
  const Lbp = (L1 + L2) / 2
  const Cbp = (C1p + C2p) / 2
  let hbp
  if (C1p * C2p === 0) hbp = h1p + h2p
  else if (Math.abs(h1p - h2p) <= 180) hbp = (h1p + h2p) / 2
  else hbp = h1p + h2p < 360 ? (h1p + h2p + 360) / 2 : (h1p + h2p - 360) / 2
  const T =
    1 -
    0.17 * Math.cos((hbp - 30) * rad) +
    0.24 * Math.cos(2 * hbp * rad) +
    0.32 * Math.cos((3 * hbp + 6) * rad) -
    0.2 * Math.cos((4 * hbp - 63) * rad)
  const Rc = 2 * Math.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7))
  const Sl = 1 + (0.015 * (Lbp - 50) ** 2) / Math.sqrt(20 + (Lbp - 50) ** 2)
  const Sc = 1 + 0.045 * Cbp
  const Sh = 1 + 0.015 * Cbp * T
  const Rt = -Rc * Math.sin(2 * (30 * Math.exp(-(((hbp - 275) / 25) ** 2))) * rad)
  return Math.sqrt(
    (dLp / Sl) ** 2 +
      (dCp / Sc) ** 2 +
      (dHp / Sh) ** 2 +
      Rt * (dCp / Sc) * (dHp / Sh),
  )
}

/// What the bars use: the natural share and the levelled one.
const BAR_ORANGE = [217, 131, 36]
const BAR_BLUE = [47, 111, 178]

function worstPair(colours) {
  let worst = Infinity
  for (let i = 0; i < colours.length; i++) {
    for (let j = i + 1; j < colours.length; j++) {
      worst = Math.min(worst, deltaE(colours[i], colours[j]))
    }
  }
  return worst
}

describe('the palette', () => {
  it('keeps the first five well apart from each other', () => {
    // Five is the common case, and the number the old palette got wrong: its
    // worst pair was 18.9 and two of them read as the same colour on screen.
    expect(worstPair(HAND_TYPE_RGB.slice(0, 5))).toBeGreaterThan(25)
  })

  it('keeps every prefix apart, since a script may declare any number', () => {
    for (let n = 2; n <= 5; n++) {
      expect(worstPair(HAND_TYPE_RGB.slice(0, n))).toBeGreaterThan(25)
    }
    // Past five there is only so much room left once orange and blue are
    // spoken for, but they stay distinguishable.
    expect(worstPair(HAND_TYPE_RGB)).toBeGreaterThan(15)
  })

  it('keeps away from the two colours the bars use', () => {
    // A label in the natural orange or the levelled blue would look like it
    // was making the bar's claim.
    for (const c of HAND_TYPE_RGB.slice(0, 5)) {
      expect(deltaE(c, BAR_ORANGE)).toBeGreaterThan(25)
      expect(deltaE(c, BAR_BLUE)).toBeGreaterThan(25)
    }
  })

  it('is dark enough to read as text on white', () => {
    for (const c of HAND_TYPE_RGB) {
      expect(lab(c)[0]).toBeLessThan(62)
    }
  })
})

describe('handTypeColor', () => {
  it('gives each of the eight its own colour', () => {
    const seen = new Set()
    for (let i = 0; i < 8; i++) seen.add(handTypeColor(i))
    expect(seen.size).toBe(8)
  })

  it('is stable for an index', () => {
    expect(handTypeColor(3)).toBe(handTypeColor(3))
  })

  it('wraps rather than running out', () => {
    expect(handTypeColor(8)).toBe(handTypeColor(0))
    expect(handTypeColor(-1)).toBe(handTypeColor(7))
  })
})

describe('handTypeTint', () => {
  it('is the label colour, translucent', () => {
    expect(handTypeTint(2)).toBe(handTypeColor(2).replace(')', ' / 0.15)'))
  })
})

describe('handTypePalette', () => {
  const palette = handTypePalette(['12_14', '15_17', '18_up'])

  it('assigns by declaration order', () => {
    expect(palette.get('12_14').index).toBe(0)
    expect(palette.get('18_up').index).toBe(2)
    expect(palette.get('15_17').color).toBe(handTypeColor(1))
  })

  it('gives an untyped deal something neutral rather than a colour', () => {
    // A deal matching no type is not an error; only levelling insists the
    // types cover everything.
    for (const missing of [null, undefined, 'not_declared']) {
      expect(palette.get(missing).index).toBe(-1)
      expect(palette.get(missing).tint).toBe('transparent')
    }
  })
})
