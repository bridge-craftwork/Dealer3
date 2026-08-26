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
// The script editor: Monaco, with highlighting and diagnostics driven by the
// engine itself.
//
// Diagnostics are the point. `check_script` is the real parser, so a squiggle
// here means the engine will reject the script — not that a regex guessed. The
// line and column come straight from pest.
import { ref, onMounted, onBeforeUnmount, watch, shallowRef } from 'vue'
// Core editor only. The `monaco-editor` barrel pulls in every language it
// ships — perl, abap, solidity, the lot — which added ~2.5 MB of bundle for a
// site that registers exactly one language of its own.
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api.js'
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import { registerDlrLanguage, LANGUAGE_ID } from '@/lib/dlrLanguage.js'
import { checkScript, languageInfo } from '@/lib/engine.js'

const props = defineProps({
  modelValue: { type: String, default: '' },
})
const emit = defineEmits(['update:modelValue', 'validity'])

// Monaco expects to find its workers here. Only the base worker is needed:
// no language services are loaded.
self.MonacoEnvironment = { getWorker: () => new EditorWorker() }

const host = ref(null)
const editor = shallowRef(null)
const diagnostic = ref(null)
let registered = false
let debounce = null

// pest errors are several lines: a location, the offending source, a caret, and
// the expectation. The status bar wants the last of those.
function summarise(message) {
  const expected = message.split('\n').find((l) => l.trim().startsWith('= '))
  if (expected) return expected.trim().slice(2)
  return message.split('\n')[0].replace(/^Parse error:\s*/, '').trim() || 'invalid syntax'
}

function validate() {
  const model = editor.value?.getModel()
  if (!model) return
  const text = model.getValue()

  if (!text.trim()) {
    diagnostic.value = null
    monaco.editor.setModelMarkers(model, 'dealer3', [])
    emit('validity', { ok: true, empty: true })
    return
  }

  const result = checkScript(text)
  if (result.ok) {
    diagnostic.value = null
    monaco.editor.setModelMarkers(model, 'dealer3', [])
    emit('validity', { ok: true, empty: false })
    return
  }

  const line = result.line ?? 1
  const column = result.column ?? 1
  diagnostic.value = { line, column, summary: summarise(result.error || '') }

  // pest reports a point, not a span. Underline to the end of the token that
  // starts there so the squiggle is visible rather than a zero-width tick.
  const lineContent = model.getLineContent(Math.min(line, model.getLineCount())) || ''
  const rest = lineContent.slice(column - 1)
  const tokenLength = (rest.match(/^\S+/) || [''])[0].length || 1

  monaco.editor.setModelMarkers(model, 'dealer3', [
    {
      severity: monaco.MarkerSeverity.Error,
      message: result.error,
      startLineNumber: line,
      startColumn: column,
      endLineNumber: line,
      endColumn: column + tokenLength,
    },
  ])
  emit('validity', { ok: false, empty: false })
}

onMounted(() => {
  if (!registered) {
    // The vocabulary comes from the engine, so highlighting and completion
    // cannot disagree with the parser.
    registerDlrLanguage(monaco, languageInfo())
    registered = true
  }

  editor.value = monaco.editor.create(host.value, {
    value: props.modelValue,
    language: LANGUAGE_ID,
    theme: 'vs',
    automaticLayout: true,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    fontSize: 13,
    lineNumbers: 'on',
    renderWhitespace: 'none',
    tabSize: 2,
  })

  editor.value.onDidChangeModelContent(() => {
    const text = editor.value.getValue()
    emit('update:modelValue', text)
    // Parsing is fast, but not worth doing on every keypress of a long script.
    clearTimeout(debounce)
    debounce = setTimeout(validate, 150)
  })

  validate()
})

// External changes (picking a scenario) replace the buffer wholesale.
watch(
  () => props.modelValue,
  (value) => {
    const ed = editor.value
    if (!ed || ed.getValue() === value) return
    ed.setValue(value)
    validate()
  },
)

onBeforeUnmount(() => {
  clearTimeout(debounce)
  editor.value?.dispose()
})

defineExpose({ focus: () => editor.value?.focus() })
</script>

<style scoped>
.editor { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.editor-host { flex: 1; min-height: 0; border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
.editor-status {
  padding: 4px 8px; font-size: 12px; color: var(--fg-muted);
  font-family: var(--mono);
}
.editor-status.is-error { color: var(--danger); }
</style>
