// Every `var(--token)` resolves to a token something defines.
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

/** Every `.vue` under `web/src`, since that is where all the styling lives. */
function vueFiles(dir = ROOT, found = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) vueFiles(path, found)
    else if (name.endsWith('.vue')) found.push(path)
  }
  return found
}

/** Comments hold prose about tokens; only real declarations count. */
const withoutComments = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/<!--[\s\S]*?-->/g, '')

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
