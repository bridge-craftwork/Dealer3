<template>
  <div ref="host" class="viewer"></div>
</template>

<script setup>
// A script shown rather than edited: the generated levelled scenario.
//
// The same CodeMirror, language and theme as the editor, minus everything that
// exists for typing — no history, no completion, no linter. Colouring it by
// hand would have been a second highlighter to keep in step with the first, and
// the point of reading this file is the levelling block, which is ordinary
// script and should look like it.
import { ref, onMounted, onBeforeUnmount, watch, shallowRef } from 'vue'
import { EditorState } from '@codemirror/state'
import { EditorView, lineNumbers } from '@codemirror/view'
import { oneDark } from '@codemirror/theme-one-dark'
import { dlrLanguage } from '@/lib/dlrLanguage.js'
import { languageInfo, ready } from '@/lib/engine.js'

const props = defineProps({
  script: { type: String, default: '' },
})

const host = ref(null)
const view = shallowRef(null)

onMounted(async () => {
  // The vocabulary comes from the engine, so highlighting cannot disagree with
  // the parser. Awaiting here rather than relying on the parent's ordering.
  await ready()
  if (!host.value) return // unmounted while the engine was loading

  view.value = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.script,
      extensions: [
        lineNumbers(),
        oneDark,
        dlrLanguage(languageInfo()),
        // Read-only both ways: `readOnly` stops edits, `editable` also stops the
        // caret and the typing affordance, so it does not invite an edit that
        // the next run would overwrite anyway.
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        EditorView.theme({
          '&': { height: '100%', fontSize: '13px' },
          '.cm-scroller': { fontFamily: 'var(--mono)', overflow: 'auto' },
          '&.cm-focused': { outline: 'none' },
        }),
      ],
    }),
  })
})

// Each run generates a new scenario; the view holds whichever is current.
watch(
  () => props.script,
  (text) => {
    const v = view.value
    if (!v || text === v.state.doc.toString()) return
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } })
  },
)

onBeforeUnmount(() => {
  view.value?.destroy()
  view.value = null
})
</script>

<style scoped>
.viewer {
  height: 100%;
  min-height: 0;
  border: 1px solid var(--editor-line);
  border-radius: 0 4px 4px 4px;
  overflow: hidden;
}
</style>
