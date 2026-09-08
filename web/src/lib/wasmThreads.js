// Telling a threaded wasm build from a single-threaded one, by reading the
// binary rather than by trusting the command that produced it.
//
// This exists because the two builds are indistinguishable from the outside —
// same file name, same exports to a bundler, a page that loads and runs either
// way — and the failure they hide is silent: a deploy that ran the wrong build
// script ships a bundle that works, deals correctly, and uses one core of
// twelve. That is exactly how the first threaded build shipped serial without
// anyone noticing, and a console line was not enough to catch it.
//
// Two independent signals, both read out of the `.wasm` itself so that
// bundling, minifying and hashing cannot affect the answer:
//
//   1. **Shared memory.** A threads build's memory carries the `shared` flag —
//      that is what `SharedArrayBuffer` is, and what the workers deal into.
//      `wasm-bindgen` imports the memory rather than defining it, so both the
//      import section and the memory section are read.
//   2. **A `start_threads` export.** The pool is only exported when the crate
//      was built with the `parallel` feature, so this says the feature was on
//      as well as the target features.
//
// A build with one and not the other is not a build anybody meant to make, so
// both are required and each is reported separately.

/// Read a LEB128 unsigned integer, returning it with the offset after it.
function uleb(bytes, at) {
  let result = 0
  let shift = 0
  let i = at
  for (;;) {
    if (i >= bytes.length) throw new Error('truncated LEB128')
    const byte = bytes[i++]
    result += (byte & 0x7f) * 2 ** shift
    if ((byte & 0x80) === 0) return [result, i]
    shift += 7
    if (shift > 56) throw new Error('LEB128 too long')
  }
}

/// Skip a memory type's limits, saying whether they are shared.
///
/// The flags byte is a bitfield: 0x01 says a maximum follows, 0x02 says shared.
function readLimits(bytes, at) {
  const flags = bytes[at]
  let i = at + 1
  ;[, i] = uleb(bytes, i) // minimum
  if (flags & 0x01) [, i] = uleb(bytes, i) // maximum
  return { shared: (flags & 0x02) !== 0, next: i }
}

/// Skip one import's type, returning the offset after it.
///
/// Only a memory import is interesting; the rest are stepped over by kind.
function skipImportKind(bytes, at, onMemory) {
  const kind = bytes[at]
  let i = at + 1
  if (kind === 0x00) {
    ;[, i] = uleb(bytes, i) // function: a type index
  } else if (kind === 0x01) {
    i += 1 // table: element type
    const limits = readLimits(bytes, i)
    i = limits.next
  } else if (kind === 0x02) {
    const limits = readLimits(bytes, i)
    onMemory(limits.shared)
    i = limits.next
  } else if (kind === 0x03) {
    i += 2 // global: value type, mutability
  } else {
    throw new Error(`unknown import kind ${kind}`)
  }
  return i
}

function readName(bytes, at) {
  const [length, start] = uleb(bytes, at)
  const text = new TextDecoder().decode(bytes.subarray(start, start + length))
  return [text, start + length]
}

/**
 * What a `.wasm` binary says about threading.
 *
 * Returns `{ threaded, sharedMemory, startThreads, reasons }` — `threaded` only
 * when both signals are present, and `reasons` listing whichever is missing so a
 * failure names the fault rather than the conclusion.
 *
 * Throws on something that is not a wasm module at all, which is a different
 * failure and should read as one.
 */
export function inspectWasm(bytes) {
  const magic = [0x00, 0x61, 0x73, 0x6d]
  if (bytes.length < 8 || magic.some((b, i) => bytes[i] !== b)) {
    throw new Error('not a WebAssembly module (bad magic number)')
  }

  let sharedMemory = false
  let startThreads = false
  let i = 8
  while (i < bytes.length) {
    const id = bytes[i++]
    const [size, body] = uleb(bytes, i)
    i = body + size

    if (id === 2) {
      // Imports, which is where wasm-bindgen puts the memory.
      let [count, at] = uleb(bytes, body)
      for (let n = 0; n < count; n++) {
        ;[, at] = readName(bytes, at) // module
        ;[, at] = readName(bytes, at) // field
        at = skipImportKind(bytes, at, (shared) => {
          if (shared) sharedMemory = true
        })
      }
    } else if (id === 5) {
      // A memory the module defines itself, for a build that does not import it.
      let [count, at] = uleb(bytes, body)
      for (let n = 0; n < count; n++) {
        const limits = readLimits(bytes, at)
        if (limits.shared) sharedMemory = true
        at = limits.next
      }
    } else if (id === 7) {
      let [count, at] = uleb(bytes, body)
      for (let n = 0; n < count; n++) {
        let name
        ;[name, at] = readName(bytes, at)
        at += 1 // kind
        ;[, at] = uleb(bytes, at) // index
        if (name === 'start_threads') startThreads = true
      }
    }
  }

  const reasons = []
  if (!sharedMemory) reasons.push('its memory is not shared, so it was built without atomics')
  if (!startThreads) {
    reasons.push('it exports no start_threads, so it was built without the `parallel` feature')
  }
  return { threaded: sharedMemory && startThreads, sharedMemory, startThreads, reasons }
}
