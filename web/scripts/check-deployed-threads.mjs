#!/usr/bin/env node
// Whether the deployed page actually deals on the thread pool.
//
//   node scripts/check-deployed-threads.mjs https://bridge-craftwork.com/dealer3/
//
// A build that silently ran on one thread passes every other test here, because
// it produces the right deals. `check-threaded.mjs` guards the bundle; this
// guards what the bundle does once it is served: the headers could be missing,
// the pool could fail to start, or a gate could come back in `wasm/src/lib.rs`.
//
// Run from CI after a deploy, on a machine with more than one core. Playwright
// is not a dependency of the site: pages.yml installs it for this step alone.

import { chromium } from 'playwright'

const url = process.argv[2]
if (!url) {
  console.error('usage: check-deployed-threads.mjs <url>')
  process.exit(2)
}

const TIMEOUT_MS = 90_000

const browser = await chromium.launch()
let failure = null
const consoleLines = []
try {
  const page = await browser.newPage()
  page.setDefaultTimeout(TIMEOUT_MS)
  // Includes the worker's console, which is where the pool says why it fell back.
  page.on('console', (message) => consoleLines.push(message.text()))
  page.on('pageerror', (error) => consoleLines.push(`pageerror: ${error.message}`))

  const response = await page.goto(url)
  console.log(`${url} answered ${response.status()}`)
  for (const name of ['cross-origin-opener-policy', 'cross-origin-embedder-policy']) {
    console.log(`${name}: ${response.headers()[name] ?? '(absent)'}`)
  }
  console.log(`crossOriginIsolated: ${await page.evaluate(() => self.crossOriginIsolated)}`)

  const badge = page.locator('.bar-threads')
  await badge.waitFor()
  console.log(`the page says: ${(await badge.textContent()).trim()}`)
  console.log(`and explains: ${await badge.getAttribute('title')}`)

  // A trivial run, because the worker reports the pool on its first one, and
  // because a pool that started and then cannot deal is the same failure.
  await page.locator('button.run:not([disabled])').click()
  const started = Date.now()
  while (
    !consoleLines.some((line) => line.startsWith('dealer3: dealing on')) &&
    Date.now() - started < TIMEOUT_MS
  ) {
    await page.waitForTimeout(250)
  }

  const threads = Number.parseInt(await badge.textContent(), 10)
  const line = consoleLines.find((text) => text.startsWith('dealer3: dealing on'))
  if (!(threads > 1)) {
    failure = `the page reports ${threads} thread(s)`
  } else if (!line) {
    failure = 'a run reported nothing about the pool'
  } else if (line.includes('one thread')) {
    failure = `the page reports ${threads} threads but the worker said: ${line}`
  }
} catch (error) {
  failure = error.message
} finally {
  await browser.close()
}

console.log('console:')
for (const line of consoleLines) console.log(`  ${line}`)

if (failure) {
  console.error(`FAILED: ${failure}`)
  process.exit(1)
}
console.log('OK: the deployed page deals on the thread pool')
