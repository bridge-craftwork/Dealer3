import { describe, it, expect, vi } from 'vitest'
import { registerDlrLanguage, LANGUAGE_ID } from './dlrLanguage.js'

// A minimal stand-in for the parts of the Monaco API this module touches.
function fakeMonaco() {
  const calls = {}
  return {
    calls,
    languages: {
      register: (v) => (calls.register = v),
      setLanguageConfiguration: (id, v) => (calls.config = { id, ...v }),
      setMonarchTokensProvider: (id, v) => (calls.monarch = { id, ...v }),
      registerCompletionItemProvider: (id, p) => (calls.completion = { id, ...p }),
      CompletionItemKind: { Function: 1, Keyword: 2, Constant: 3 },
      CompletionItemInsertTextRule: { InsertAsSnippet: 4 },
    },
  }
}

// Shaped like the engine's language_info() output.
const info = {
  functions: ['hcp', 'shape', 'spade', 'spades', 'top2', 'pt0'],
  statement_keywords: ['condition', 'produce', 'csvrpt'],
  actions: ['printall', 'printoneline'],
  positions: ['north', 'south', 'n', 's'],
  vulnerabilities: ['none', 'all'],
  logical_words: ['and', 'or', 'not'],
  other_keywords: ['any', 'deal'],
  operators: ['==', '>=', '>', '='],
}

describe('registerDlrLanguage', () => {
  it('registers the language id', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    expect(m.calls.register.id).toBe(LANGUAGE_ID)
  })

  it('takes every function from the engine vocabulary', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    for (const f of info.functions) expect(m.calls.monarch.functions).toContain(f)
  })

  it('orders words longest-first so a prefix cannot shadow a longer word', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    const fns = m.calls.monarch.functions
    // `spade` must not be matched where `spades` was written.
    expect(fns.indexOf('spades')).toBeLessThan(fns.indexOf('spade'))
  })

  it('treats statement keywords and actions as keywords', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    expect(m.calls.monarch.keywords).toEqual(expect.arrayContaining(['condition', 'csvrpt', 'printall']))
  })

  it('excludes single-letter positions from constants', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    // `n` and `s` are valid positions but also common variable names; colouring
    // every one of them would be worse than leaving them plain.
    expect(m.calls.monarch.constants).toContain('north')
    expect(m.calls.monarch.constants).not.toContain('n')
  })

  it('offers completions for functions with their parentheses', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    const model = {
      getWordUntilPosition: () => ({ startColumn: 1, endColumn: 4 }),
    }
    const { suggestions } = m.calls.completion.provideCompletionItems(model, { lineNumber: 1 })
    const hcp = suggestions.find((s) => s.label === 'hcp')
    expect(hcp.insertText).toBe('hcp($0)')
    expect(hcp.detail).toBe('function')
  })

  it('is case-insensitive, matching the grammar', () => {
    const m = fakeMonaco()
    registerDlrLanguage(m, info)
    expect(m.calls.monarch.ignoreCase).toBe(true)
  })
})
