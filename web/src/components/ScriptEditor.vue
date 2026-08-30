<template>
  <div class="editor">
    <CopyButton :text="currentText" title="Copy the script" />
    <div ref="host" class="editor-host"></div>
    <div class="editor-status" :class="{ 'is-error': !!diagnostic }">
      <span v-if="diagnostic">
        Line {{ diagnostic.line }}, column {{ diagnostic.column }} — {{ diagnostic.summary }}
      </span>
      <span v-else>No syntax errors</span>
    </div>
  </div>
</template>

<script setup>
// The script editor: CodeMirror 6, with highlighting and diagnostics driven by
// the engine itself.
//
// Diagnostics are the point. `check_script` is the real parser, so a squiggle
// here means the engine will reject the script — not that a regex guessed. The
// line and column come straight from pest.
import { ref, onMounted, onBeforeUnmount, watch, shallowRef } from 'vue'
import { EditorState } from '@codemirror/state'
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { bracketMatching } from '@codemirror/language'
import { autocompletion, closeBrackets } from '@codemirror/autocomplete'
import { linter, lintGutter } from '@codemirror/lint'
// A dark editor on a light page. Syntax palettes are built for dark grounds —
// the same colours that read clearly there are washed out on white, which is
// what the default highlight style looked like here.
import { oneDark } from '@codemirror/theme-one-dark'
import { dlrLanguage, dlrCompletion } from '@/lib/dlrLanguage.js'
import { checkScript, languageInfo, ready } from '@/lib/engine.js'
import CopyButton from './CopyButton.vue'

const props = defineProps({
  modelValue: { type: String, default: '' },
})
const emit = defineEmits(['update:modelValue', 'validity'])

const host = ref(null)
const view = shallowRef(null)

/// The document as it stands, read at click time by the copy button.
///
/// From the view rather than the `modelValue` prop, so what is copied is what
/// is on screen even between a keystroke and the update it emits.
const currentText = () => view.value?.state.doc.toString() ?? props.modelValue ?? ''

const diagnostic = ref(null)

// pest errors are several lines: a location, the offending source, a caret, and
// the expectation. The status bar wants the last of those.
function summarise(message) {
  const expected = message.split('\n').find((l) => l.trim().startsWith('= '))
  if (expected) return expected.trim().slice(2)
  return message.split('\n')[0].replace(/^Parse error:\s*/, '').trim() || 'invalid syntax'
}

/** Translate the engine's line/column into a document offset. */
function offsetOf(doc, line, column) {
  const l = Math.min(Math.max(line, 1), doc.lines)
  const info = doc.line(l)
  return Math.min(info.from + Math.max(column - 1, 0), info.to)
}

// The linter runs on a debounce CodeMirror manages, and is the single place that
// decides validity — the status bar and the Run button both read from it.
const dlrLinter = linter((v) => {
  const text = v.state.doc.toString()

  if (!text.trim()) {
    diagnostic.value = null
    emit('validity', { ok: true, empty: true })
    return []
  }

  const result = checkScript(text)
  if (result.ok) {
    diagnostic.value = null
    emit('validity', { ok: true, empty: false })
    return []
  }

  const line = result.line ?? 1
  const column = result.column ?? 1
  diagnostic.value = { line, column, summary: summarise(result.error || '') }
  emit('validity', { ok: false, empty: false })

  // pest reports a point, not a span. Underline to the end of the token that
  // starts there, so the squiggle is visible rather than a zero-width tick.
  const from = offsetOf(v.state.doc, line, column)
  const rest = v.state.doc.sliceString(from, v.state.doc.line(Math.min(line, v.state.doc.lines)).to)
  const tokenLength = (rest.match(/^\S+/) || [''])[0].length || 1

  return [
    {
      from,
      to: Math.min(from + tokenLength, v.state.doc.length),
      severity: 'error',
      message: result.error,
    },
  ]
}, { delay: 150 })

onMounted(async () => {
  // The vocabulary and the diagnostics both come from the engine, so the editor
  // cannot be built until the wasm module has initialised. Awaiting here rather
  // than relying on the parent's ordering keeps that dependency local: this used
  // to work only because the component happened to be lazy-loaded, and broke the
  // moment it was imported directly.
  await ready()
  if (!host.value) return // unmounted while the engine was loading

  // The vocabulary comes from the engine, so highlighting and completion cannot
  // disagree with the parser.
  const info = languageInfo()

  view.value = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        lineNumbers(),
        highlightActiveLine(),
        history(),
        bracketMatching(),
        closeBrackets(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        oneDark,
        dlrLanguage(info),
        autocompletion({ override: [dlrCompletion(info)] }),
        lintGutter(),
        dlrLinter,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) emit('update:modelValue', u.state.doc.toString())
        }),
        EditorView.theme({
          '&': { height: '100%', fontSize: '13px' },
          '.cm-scroller': { fontFamily: 'var(--mono)', overflow: 'auto' },
          '&.cm-focused': { outline: 'none' },
          // one-dark ships no rule for the lint gutter marker; without this it
          // renders near-invisible against the dark gutter.
          '.cm-lint-marker-error': { filter: 'brightness(1.4)' },

          // The names the levelling machinery reads, which are ordinary
          // variables to the grammar and so coral like any other without this.
          // One-dark's eight hues are all spoken for, and a ninth close enough
          // to fit would be close enough to confuse — so these are marked by a
          // dotted rule under a brighter ivory instead, which no other token
          // wears. The underline's colour is the only thing separating a hand
          // type from the share that weights it.
          '.dlr-leveling-name': {
            color: '#dfe4ec',
            textDecoration: 'underline dotted #61afef',
            textUnderlineOffset: '3px',
          },
          '.dlr-leveling-share': {
            color: '#dfe4ec',
            textDecoration: 'underline dotted #e5c07b',
            textUnderlineOffset: '3px',
          },
          // The generated block's markers and stamp. Comments, and left the
          // colour of comments — they have to be comments for a levelled
          // scenario to run on BBO — but weighted, so the region you must not
          // edit by hand is bracketed visibly.
          '.dlr-leveling-marker': { color: '#93a1b5', fontWeight: '700' },
          // A `# key: value` header PBS reads. Both halves are coloured, and in
          // one-dark's own hues — the key in the whiskey it paints constants,
          // which cannot appear in a header, the value in the green it paints
          // text. The effect that matters is on the line this does *not* match:
          // a mistyped key leaves the whole header the flat grey of an ordinary
          // comment, which is what it has become.
          '.dlr-meta-key': { color: '#d19a66', fontWeight: '700' },
          '.dlr-meta-value': { color: '#98c379' },
        }),
      ],
    }),
  })
})

// External changes (picking a scenario) replace the buffer wholesale.
watch(
  () => props.modelValue,
  (value) => {
    const v = view.value
    if (!v || v.state.doc.toString() === value) return
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } })
  },
)

onBeforeUnmount(() => view.value?.destroy())

defineExpose({ focus: () => view.value?.focus() })
</script>

<style scoped>
/* `position: relative` so the copy button can sit inset in the top-right. */
.editor { position: relative; display: flex; flex-direction: column; height: 100%; min-height: 0; }
.editor-host {
  flex: 1; min-height: 0; overflow: hidden;
  border: 1px solid var(--editor-line); border-radius: 4px 4px 0 0;
}
/* Reads as part of the dark editor rather than the light page. */
.editor-status {
  padding: 4px 8px; font-size: 12px; font-family: var(--mono);
  color: #9aa3b2; background: var(--editor-bg);
  border: 1px solid var(--editor-line); border-top: 0;
  border-radius: 0 0 4px 4px; margin-top: -5px;
}
.editor-status.is-error { color: #ff8a80; }
</style>
