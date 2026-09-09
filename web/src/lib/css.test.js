// CSS that silently does nothing.
//
// Two failures in two pull requests, both invisible to the build, the console
// and every other test: a rule that names a token nobody defines, and a rule
// that styles a class nothing wears. Neither is an error anywhere in the
// toolchain; both just quietly stop applying.
//
// ## Undefined custom properties
//
// CSS fails silently and specifically: an undefined `var()` is not ignored,
// it makes the whole declaration invalid at computed-value time, and the
// property falls back to its initial value. `border: 1px solid var(--nope)`
// does not draw a default border — it resolves to `border-style: none` and
// draws nothing at all.
//
// So a typo in a token name deletes a rule, in a build that succeeds, with no
// warning from Vite, no console error, and no failing test. The settings
// button shipped with `var(--border)` and `var(--bg-raised)`, neither of which
// exists, and had no border, no background and no box; it took someone looking
// at it to notice.
//
// `var(--x, fallback)` is deliberately allowed. A fallback is the language's
// own way of saying "this may not be defined", and `var(--muted, #777)` is
// correct code.

import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'

const ROOT = new URL('../', import.meta.url).pathname

/** Every source file under `web/src`, by extension. */
function sourceFiles(ext, dir = ROOT, found = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) sourceFiles(ext, path, found)
    else if (ext.some((e) => name.endsWith(e))) found.push(path)
  }
  return found
}

/** Styling lives only in `.vue`; a class can be worn from `.js` as well. */
const vueFiles = () => sourceFiles(['.vue'])

/** Comments hold prose about tokens; only real declarations count. */
const withoutComments = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/<!--[\s\S]*?-->/g, '')

/** Everything but the style block: where a class can actually be worn. */
const withoutStyles = (source) => {
  const at = source.indexOf('<style')
  return at === -1 ? source : source.slice(0, at)
}

/** The style half, with comments stripped: prose mentions selectors. */
const styleOf = (source) =>
  withoutComments(source.slice(source.indexOf('<style')))

const files = vueFiles()
const sources = new Map(files.map((f) => [f, withoutComments(readFileSync(f, 'utf8'))]))

/** Tokens defined anywhere: they are inherited, so `:root` in App.vue serves all. */
const defined = new Set()
for (const source of sources.values()) {
  for (const [, name] of source.matchAll(/(--[a-z0-9-]+)\s*:/g)) defined.add(name)
}

describe('CSS custom properties', () => {
  it('finds the app to check', () => {
    // If the walk breaks, every assertion below passes vacuously.
    expect(files.length).toBeGreaterThan(5)
    expect(defined.size).toBeGreaterThan(15)
  })

  it.each([...sources.keys()].map((f) => [f.slice(ROOT.length), f]))(
    '%s uses only tokens that exist',
    (_label, file) => {
      // Bare `var(--x)` only. `var(--x, fallback)` says out loud that the
      // token may be missing, and is correct.
      const used = [...sources.get(file).matchAll(/var\(\s*(--[a-z0-9-]+)\s*\)/g)].map((m) => m[1])
      const missing = [...new Set(used)].filter((name) => !defined.has(name))
      expect(missing).toEqual([])
    },
  )
})

// ## Orphaned selectors
//
// A class selector that nothing wears is dead. It survives a rename of whatever
// used to wear it, keeps compiling, and takes its styling with it: renaming
// `.controls` to `.settings-panel` orphaned six rules, and the number fields
// silently lost the widths those rules gave them.
//
// "Worn" is checked against every source file with its style block removed,
// not against the component's own template. A class can reach an element three
// legitimate ways that a template search alone would miss: `:class="{ on: x }"`
// never writes `class="on"`; `guide.js` emits `class="table-scroll"` into HTML
// rendered with `v-html`; and a scoped rule may target a child component's root.
// So this catches the case with no honest explanation — a name mentioned in a
// stylesheet and nowhere else in the app.
describe('scoped styles', () => {
  // Every source file, `.js` included: `guide.js` emits `class="table-scroll"`
  // into HTML that `Leveling.vue` renders, and a `.vue`-only search calls that
  // dead when it is not.
  const worn = sourceFiles(['.vue', '.js'])
    .map((f) => withoutStyles(readFileSync(f, 'utf8')))
    .join('\n')

  it.each(files.map((f) => [f.slice(ROOT.length), f]))(
    "%s styles only classes something wears",
    (_label, file) => {
      const source = readFileSync(file, 'utf8')
      if (!source.includes('<style')) return
      const styled = new Set([...styleOf(source).matchAll(/\.([a-z][a-z0-9-]*)/g)].map((m) => m[1]))
      expect([...styled].filter((name) => !worn.includes(name))).toEqual([])
    },
  )
})
