// Monaco language support for dealer scripts.
//
// The tokenizer is BUILT AT RUNTIME from the engine's own `language_info()`,
// rather than shipping a second copy of the word lists. That export comes from
// `dealer_parser::vocabulary`, which is itself checked against `grammar.pest` by
// two tests in dealer-parser. So highlighting cannot advertise a function the
// parser does not accept, or miss one it does — the failure mode that left 19
// functions unhighlighted in the VS Code extension for years.
//
// This is why the editor does not load `dlr.tmLanguage.json` directly. That file
// still exists and is still the source for VS Code, but both it and this are
// generated from the same vocabulary, so they agree by construction. Deriving
// the tokenizer here avoids shipping vscode-textmate and an Oniguruma wasm build
// (~300 kB) to say the same thing.

export const LANGUAGE_ID = 'dlr'

/** Escape a word for use inside a RegExp alternation. */
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** Longest first, so `spades` wins over `spade` and `>=` over `>`. */
const byLengthDesc = (a, b) => b.length - a.length || a.localeCompare(b)

export function registerDlrLanguage(monaco, info) {
  monaco.languages.register({ id: LANGUAGE_ID, extensions: ['.dlr'], aliases: ['DLR', 'dlr'] })

  const keywords = [...info.statement_keywords, ...info.actions].sort(byLengthDesc)
  const constants = [
    ...info.positions.filter((p) => p.length > 1),
    ...info.vulnerabilities,
    ...info.other_keywords,
  ].sort(byLengthDesc)

  monaco.languages.setLanguageConfiguration(LANGUAGE_ID, {
    comments: { lineComment: '#', blockComment: ['/*', '*/'] },
    brackets: [['(', ')']],
    autoClosingPairs: [
      { open: '(', close: ')' },
      { open: '"', close: '"' },
    ],
    surroundingPairs: [
      { open: '(', close: ')' },
      { open: '"', close: '"' },
    ],
  })

  monaco.languages.setMonarchTokensProvider(LANGUAGE_ID, {
    defaultToken: '',
    ignoreCase: true,
    keywords,
    functions: [...info.functions].sort(byLengthDesc),
    constants,
    logical: info.logical_words,

    tokenizer: {
      root: [
        // `# key: value` metadata headers PBS scripts carry, then plain comments.
        [/^\s*#\s*[a-zA-Z-]+:.*$/, 'comment.doc'],
        [/#.*$/, 'comment'],
        [/\/\/.*$/, 'comment'],
        [/\/\*/, 'comment', '@blockComment'],

        [/"/, 'string', '@string'],

        // Shape patterns: 4333, 5xxx, %s4432. Before numbers, which would
        // otherwise eat the leading digits.
        [/%s?\d{4}\b/, 'number.hex'],
        [/\b[0-9xX]{4}\b/, 'number.hex'],

        // Card and holding literals: AS, TC (hascard); SAKQ, HT62 (predeal).
        [/\b[AKQJT2-9][SHDC]\b/, 'constant'],
        [/\b[SHDC][AKQJT2-9]+\b/, 'constant'],

        [
          /[a-zA-Z_][a-zA-Z0-9_]*/,
          {
            cases: {
              '@keywords': 'keyword',
              '@functions': 'type.identifier',
              '@constants': 'constant',
              '@logical': 'keyword.operator',
              '@default': 'identifier',
            },
          },
        ],

        [/\d+/, 'number'],
        [/[=!<>]=|&&|\|\||[-+*/%<>?:!=]/, 'operator'],
        [/[()]/, '@brackets'],
        [/[,;]/, 'delimiter'],
      ],

      blockComment: [
        [/[^*/]+/, 'comment'],
        [/\*\//, 'comment', '@pop'],
        [/[*/]/, 'comment'],
      ],

      string: [
        [/[^"]+/, 'string'],
        [/"/, 'string', '@pop'],
      ],
    },
  })

  registerCompletion(monaco, info)
}

function registerCompletion(monaco, info) {
  monaco.languages.registerCompletionItemProvider(LANGUAGE_ID, {
    provideCompletionItems(model, position) {
      const word = model.getWordUntilPosition(position)
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      }
      const { Function, Keyword, Constant } = monaco.languages.CompletionItemKind
      const item = (label, kind, detail, insertText) => ({
        label,
        kind,
        detail,
        range,
        insertText: insertText ?? label,
        ...(insertText
          ? { insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet }
          : {}),
      })

      return {
        suggestions: [
          // Functions all take arguments, so complete the parentheses too and
          // drop the cursor inside them.
          ...info.functions.map((f) => item(f, Function, 'function', `${f}($0)`)),
          ...info.statement_keywords.map((k) => item(k, Keyword, 'statement')),
          ...info.actions.map((a) => item(a, Keyword, 'action')),
          ...info.positions.filter((p) => p.length > 1).map((p) => item(p, Constant, 'position')),
          ...info.vulnerabilities.map((v) => item(v, Constant, 'vulnerability')),
          ...info.logical_words.map((w) => item(w, Keyword, 'operator')),
        ],
      }
    },
  })
}
