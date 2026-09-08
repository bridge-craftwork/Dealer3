/**
 * A statistic's label, as the script wrote it.
 *
 * Scripts indent labels with leading spaces to group what belongs together —
 *
 *     average "Openers"          hcp(north),
 *     average "    balanced"     shape(north, any 4333 + any 4432),
 *     average "    unbalanced"   1 - shape(north, any 4333 + any 4432)
 *
 * — and the command line prints them exactly as given, so the indent is part
 * of what the script author wrote and not incidental whitespace. This used to
 * be trimmed away, which is why the page and the terminal disagreed about the
 * same script.
 *
 * A label of nothing but spaces still gets the fallback: it names nothing, and
 * a row labelled with four spaces reads as a bug rather than as a heading.
 */
export function labelOf(label, fallback) {
  return label && label.trim() ? label : fallback
}

/**
 * Does this label lean on its leading space to mean something?
 *
 * Only then is the monospace treatment worth it. An ordinary label reads
 * better in the interface's own face; an indented one has to line up with its
 * neighbours, which needs every space the same width.
 */
export function isIndented(label) {
  return typeof label === 'string' && /^\s/.test(label) && label.trim() !== ''
}
