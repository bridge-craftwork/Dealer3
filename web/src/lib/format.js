// Display formatting shared by the screen and the printed document.

/**
 * Format an average for display.
 *
 * The engine returns full f64 — `9.176470588235293` — and six significant
 * digits (`9.17647`) is still more than an average over a few dozen deals can
 * support. Two decimals is the resolution that means anything here; the deal
 * count is the real limit on precision, not the arithmetic.
 *
 * Trailing zeros are dropped so a column of values stays easy to scan, and
 * values too small for two decimals fall back to significant digits rather than
 * rendering as a misleading "0.00".
 */
export function formatAverage(value) {
  if (!Number.isFinite(value)) return '—'
  if (Number.isInteger(value)) return String(value)

  const magnitude = Math.abs(value)
  if (magnitude > 0 && magnitude < 0.01) {
    // Real but tiny: show that it is non-zero rather than rounding it away.
    return String(Number(value.toPrecision(2)))
  }
  return String(Number(value.toFixed(2)))
}

/** A seed in the range the engine accepts (u32). */
export function randomSeed() {
  // crypto for a good spread; the fallback matters only where it is unavailable.
  if (globalThis.crypto?.getRandomValues) {
    return globalThis.crypto.getRandomValues(new Uint32Array(1))[0]
  }
  return Math.floor(Math.random() * 0xffffffff)
}
