// What a short link looks like — the part both ends need.
//
// The page reads keys out of links and builds the link from a key; the service
// in `shortLinks.js` makes the keys. Kept apart from both so neither has to
// import the other.

/// How long a link lasts. It is in the URL as well — `/30-days/<key>` — so
/// changing this means changing that path, and the old links' path with it.
export const SHORT_LINK_DAYS = 30

/// The path segment a short link lives under. It says how long the link lasts
/// before anybody opens it: someone about to bookmark one is the person who
/// most needs to know, and a word in the URL reaches them first.
export const SHORT_LINK_PATH = `${SHORT_LINK_DAYS}-days`

/// Crockford's base32: no I, L, O or U, so nothing a phone's font or a
/// person's reading of it can confuse.
const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'

/// 32^8, about 10^12. At a thousand links a day a collision is not a real
/// prospect, and the service reads before it writes anyway.
export const KEY_LENGTH = 8

/// Said when a key holds nothing. KV cannot tell an expired key from one that
/// never existed, and nor can this; expiry is the likely one.
export const EXPIRED =
  `This short link has expired, or never existed. Short links last ${SHORT_LINK_DAYS} days — ` +
  'ask whoever sent it for a new one, or for the long link, which never expires.'

/** A fresh key. 256 is a multiple of 32, so masking a byte is unbiased. */
export function newKey(random = (bytes) => crypto.getRandomValues(bytes)) {
  const bytes = random(new Uint8Array(KEY_LENGTH))
  let key = ''
  for (const byte of bytes) key += ALPHABET[byte & 31]
  return key
}

/**
 * A key as somebody might have typed or received it, made canonical — or null.
 *
 * Crockford's decoding rules: case does not matter, `O` is zero, `I` and `L`
 * are one, and hyphens are ignored. A phone that capitalises the first letter
 * or a person reading a key aloud should not break a link.
 *
 * @param {string} text
 * @returns {string|null}
 */
export function normalizeKey(text) {
  const key = String(text || '')
    .toUpperCase()
    .replace(/-/g, '')
    .replace(/O/g, '0')
    .replace(/[IL]/g, '1')
  return key.length === KEY_LENGTH && [...key].every((c) => ALPHABET.includes(c)) ? key : null
}
