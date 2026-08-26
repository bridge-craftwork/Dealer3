// Saving results to a file.
//
// Selecting text out of the page is clumsy — a select-all takes the whole
// document, and a long run is thousands of lines — so results are saved rather
// than copied.

/** Trigger a download of `text` as `filename`. */
export function downloadText(filename, text, mime = 'text/plain') {
  const blob = new Blob([text], { type: `${mime};charset=utf-8` })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  // Revoking immediately can cancel the download in some browsers; a tick is
  // enough for the navigation to have started.
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

/** A filename that sorts usefully and says where it came from. */
export function resultFilename(scenario, seed, extension) {
  const base = (scenario || 'dealer3').replace(/[^\w.-]+/g, '_')
  return `${base}-seed${seed}.${extension}`
}

/**
 * Render the statistics block as text, for saving alongside deals.
 *
 * Deliberately mirrors the CLI's layout rather than inventing one: someone
 * saving this has probably seen dealer output before.
 */
export function statisticsText(result) {
  const lines = []
  for (const a of result.averages || []) {
    lines.push(`${(a.label || 'Average').trim()}: ${a.value}`)
  }
  for (const f of result.frequencies || []) {
    lines.push(`Frequency ${(f.label || '').trim()}:`)
    if (f.below) lines.push(`Low\t${String(f.below).padStart(8)}`)
    for (const bin of f.bins) {
      lines.push(`${String(bin.value).padStart(5)}\t${String(bin.count).padStart(8)}`)
    }
    if (f.above) lines.push(`High\t${String(f.above).padStart(8)}`)
  }
  lines.push(`Generated ${result.generated} hands`)
  lines.push(`Produced ${result.produced} hands`)
  return lines.join('\n') + '\n'
}
