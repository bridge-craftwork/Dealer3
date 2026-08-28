<template>
  <button
    class="copy"
    type="button"
    :class="{ done: state === 'done', failed: state === 'failed' }"
    :title="title"
    @click="copy"
  >
    {{ label }}
  </button>
</template>

<script setup>
// Copy the text of the pane it sits in.
//
// Selecting it by hand is worse than it sounds: the panes are CodeMirror, which
// draws only the lines on screen, so Ctrl-A takes the whole page and dragging
// takes only what has been rendered. The generated scenario is the thing most
// worth copying out — it is what you paste into BBO — and it is read-only, so
// there is no caret to select from either.
import { ref } from 'vue'

const props = defineProps({
  /// Called for the text, rather than passing it: the editor holds the current
  /// document and reading it at click time avoids keeping a second copy in
  /// sync with every keystroke.
  text: { type: Function, required: true },
  title: { type: String, default: 'Copy to the clipboard' },
})

const state = ref('idle')
const label = ref('Copy')

/// Show an outcome for a moment, then go back to offering the action.
function flash(next, text) {
  state.value = next
  label.value = text
  setTimeout(() => {
    state.value = 'idle'
    label.value = 'Copy'
  }, 1400)
}

async function copy() {
  const value = props.text() ?? ''
  try {
    // `navigator.clipboard` is absent on an insecure origin and can be refused
    // outright, so the older path is a real fallback rather than politeness.
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value)
    } else {
      const area = document.createElement('textarea')
      area.value = value
      area.setAttribute('readonly', '')
      area.style.position = 'fixed'
      area.style.opacity = '0'
      document.body.appendChild(area)
      area.select()
      const ok = document.execCommand('copy')
      document.body.removeChild(area)
      if (!ok) throw new Error('execCommand refused')
    }
    flash('done', 'Copied')
  } catch {
    // Saying so beats a button that looks as though it worked.
    flash('failed', 'Press ⌘C')
  }
}
</script>

<style scoped>
/* Inset in the pane's top-right, over the editor's own dark ground. Absolute
   against the pane rather than in the layout, so it costs no height — these
   panes are already the tallest thing on the page. */
.copy {
  position: absolute;
  top: 6px;
  right: 14px;
  z-index: 3;
  padding: 2px 8px;
  font: 11px/1.6 var(--mono);
  color: #9aa3b2;
  background: rgba(40, 44, 52, 0.85);
  border: 1px solid var(--editor-line);
  border-radius: 3px;
  cursor: pointer;
  /* Out of the way until wanted: the script is what the pane is for. */
  opacity: 0.55;
  transition: opacity 0.12s ease, color 0.12s ease;
}
.copy:hover,
.copy:focus-visible {
  opacity: 1;
  color: #e6e6e6;
}
.copy.done {
  opacity: 1;
  color: #9ecb8f;
  border-color: #4a6b45;
}
.copy.failed {
  opacity: 1;
  color: #ff8a80;
  border-color: #6b4545;
}
</style>
