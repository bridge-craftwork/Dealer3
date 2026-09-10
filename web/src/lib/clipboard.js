// Copying text, from the two places that offer it.
//
// `navigator.clipboard` is absent on an insecure origin and can be refused
// outright, so the older path is a real fallback rather than politeness.

/**
 * Put text on the clipboard.
 *
 * @param {string} text
 * @returns {Promise<boolean>} whether it worked, so the caller can say so
 */
export async function copyText(text) {
  const value = text ?? ''
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value)
      return true
    }
    const area = document.createElement('textarea')
    area.value = value
    area.setAttribute('readonly', '')
    area.style.position = 'fixed'
    area.style.opacity = '0'
    document.body.appendChild(area)
    area.select()
    const ok = document.execCommand('copy')
    document.body.removeChild(area)
    return ok
  } catch {
    // Refused, or no clipboard at all. Saying so beats a control that looks as
    // though it worked.
    return false
  }
}
