<template>
  <div class="editor">
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
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from '@codemirror/language'
import { autocompletion, closeBrackets } from '@codemirror/autocomplete'
import { linter, lintGutter } from '@codemirror/lint'
import { dlrLanguage, dlrCompletion } from '@/lib/dlrLanguage.js'
import { checkScript, languageInfo, ready } from '@/lib/engine.js'

const props = defineProps({
  modelValue: { type: String, default: '' },
})
const emit = defineEmits(['update:modelValue', 'validity'])

const host = ref(null)
const view = shallowRef(null)
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
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
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
.editor { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.editor-host {
  flex: 1; min-height: 0; overflow: hidden;
  border: 1px solid var(--line); border-radius: 4px;
}
.editor-status {
  padding: 4px 8px; font-size: 12px; color: var(--fg-muted); font-family: var(--mono);
}
.editor-status.is-error { color: var(--danger); }
</style>
