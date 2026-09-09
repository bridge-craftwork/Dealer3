import { describe, it, expect } from 'vitest'
import { inspectWasm } from '@/lib/wasmThreads.js'

// Modules built by hand, because the two real ones are 2 MB each and a test
// that needs a build to run is a test nobody runs. These carry exactly the
// bytes the check reads: an imported memory with its limits flags, a defined
// memory with its own, and an export section.

const HEADER = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]

/// A section: id, length, body.
function section(id, body) {
  return [id, body.length, ...body]
}

/// A name, as wasm writes one: length then bytes.
function name(text) {
  const bytes = [...new TextEncoder().encode(text)]
  return [bytes.length, ...bytes]
}

/// An import section holding one memory. `flags` is the limits bitfield:
/// 0x01 "a maximum follows", 0x02 "shared".
function importedMemory(flags) {
  const body = [
    1, // one import
    ...name('env'),
    ...name('memory'),
    0x02, // a memory
    flags,
    25, // minimum
    ...(flags & 0x01 ? [0x80, 0x80, 0x01] : []), // maximum, when there is one
  ]
  return section(2, body)
}

/// A memory section, for a module that defines its own rather than importing it.
function definedMemory(flags) {
  return section(5, [1, flags, 17, ...(flags & 0x01 ? [0x80, 0x80, 0x01] : [])])
}

/// An export section naming functions.
function exports(names) {
  const body = [names.length]
  names.forEach((text, index) => body.push(...name(text), 0x00, index))
  return section(7, body)
}

const wasm = (...parts) => new Uint8Array([...HEADER, ...parts.flat()])

describe('reading a wasm build for threads', () => {
  it('accepts the threaded build: shared memory and a start_threads export', () => {
    const report = inspectWasm(wasm(importedMemory(0x03), exports(['generate', 'start_threads'])))
    expect(report).toMatchObject({ threaded: true, sharedMemory: true, startThreads: true })
    expect(report.reasons).toEqual([])
  })

  // The failure this whole check exists for: a deploy that ran `npm run wasm`
  // instead of `npm run wasm:threaded`. It works, it deals the same deals, and
  // it uses one core.
  it('rejects the single-threaded build, and says which half is missing', () => {
    const report = inspectWasm(wasm(importedMemory(0x01), exports(['generate'])))
    expect(report.threaded).toBe(false)
    expect(report.sharedMemory).toBe(false)
    expect(report.startThreads).toBe(false)
    expect(report.reasons.join(' ')).toMatch(/atomics/)
    expect(report.reasons.join(' ')).toMatch(/parallel/)
  })

  // Built with atomics but without the feature that exports the pool: it would
  // instantiate, need the COOP/COEP headers, and still deal on one thread.
  it('rejects atomics without the parallel feature', () => {
    const report = inspectWasm(wasm(importedMemory(0x03), exports(['generate'])))
    expect(report.threaded).toBe(false)
    expect(report.sharedMemory).toBe(true)
    expect(report.reasons).toHaveLength(1)
  })

  // wasm-bindgen imports the memory, but nothing says it must; a module that
  // defines its own has to be read the same way.
  it('reads a memory the module defines itself', () => {
    const report = inspectWasm(wasm(definedMemory(0x03), exports(['start_threads'])))
    expect(report.threaded).toBe(true)
  })

  it('refuses something that is not wasm at all', () => {
    expect(() => inspectWasm(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]))).toThrow(/WebAssembly/)
  })
})
