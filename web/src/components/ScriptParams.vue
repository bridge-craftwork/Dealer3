<template>
  <div v-if="params.length || error" class="params">
    <div v-if="error" class="params-error">{{ error }}</div>
    <template v-else>
      <div class="params-head">
        <span class="params-title">Parameters</span>
        <span class="params-hint">{{ hint }}</span>
      </div>
      <div class="params-fields">
        <!-- A declaration nothing uses gets no field: there is nothing for a
             value to reach, and an input that cannot affect the run is worse
             than a note saying so. -->
        <div v-for="p in params" :key="p.index" class="param" :class="{ needed: needed(p) }">
          <template v-if="p.usedOn">
            <label class="param-name" :for="`param-${p.index}`">${{ p.index }}</label>
            <input
              :id="`param-${p.index}`"
              class="param-input"
              type="text"
              :value="values[p.index] || ''"
              :placeholder="p.default == null ? 'needs a value' : p.default"
              :title="title(p)"
              spellcheck="false"
              @input="set(p.index, $event.target.value)"
            />
          </template>
          <template v-else>
            <span class="param-name">${{ p.index }}</span>
            <span class="param-stale" :title="title(p)">declared on line {{ p.declaredOn }}, unused</span>
          </template>
          <span v-if="note(p)" class="param-desc">{{ note(p) }}</span>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup>
// The fields for a script's `$0`-`$9`.
//
// Without these a parameterised scenario simply failed to parse here, with an
// error naming a line the reader could not act on and no way to act on it. The
// `$n` occurrences alone are not enough to build a form from: they give nowhere
// to put a label and no sensible starting value, which is what the script's own
// `# param 0 = west   # the seat that opens` lines supply.
//
// A field left empty is not an empty value — it means "use what the script
// says", so clearing a field returns to the declared default rather than
// blanking the parameter. Only a parameter with no default and no value stops
// the run, and that is the one this marks.
import { computed, ref, watch } from 'vue'
import { scriptParams } from '@/lib/engine.js'

const props = defineProps({
  script: { type: String, default: '' },
  /// `{ 0: 'west', 1: '15' }` — what has been typed, by parameter number.
  modelValue: { type: Object, default: () => ({}) },
  /// The engine has to have loaded before the script can be read.
  engineReady: { type: Boolean, default: false },
})
const emit = defineEmits(['update:modelValue', 'change'])

const params = ref([])
const error = ref('')

const values = computed(() => props.modelValue || {})

function analyse() {
  if (!props.engineReady) return
  const result = scriptParams(props.script || '')
  // A malformed `# param` line. Shown here rather than only as a parse failure,
  // because this is the pane it is about.
  error.value = result.ok ? '' : result.error || ''
  params.value = result.ok ? result.params : []
  announce()
}

/// What the run needs: `--param`'s own `N=TEXT` spelling, and whether anything
/// is still missing.
function announce() {
  const used = params.value.filter((p) => p.usedOn)
  const specs = used
    .filter((p) => (values.value[p.index] || '').trim() !== '')
    .map((p) => `${p.index}=${values.value[p.index].trim()}`)
  const missing = used
    .filter((p) => p.default == null && (values.value[p.index] || '').trim() === '')
    .map((p) => p.index)
  emit('change', { specs, missing })
}

watch(() => [props.script, props.engineReady], analyse, { immediate: true })
watch(() => props.modelValue, announce, { deep: true })

function set(index, text) {
  emit('update:modelValue', { ...values.value, [index]: text })
}

function needed(p) {
  return p.usedOn && p.default == null && (values.value[p.index] || '').trim() === ''
}

/// The description belongs under the field; a parameter with no description and
/// nothing declaring a default still needs a line saying why it is marked.
function note(p) {
  if (!p.usedOn) return ''
  if (needed(p)) return p.description || 'nothing declares a default for this one'
  return p.description || ''
}

function title(p) {
  if (!p.usedOn) return 'Nothing in the script uses this parameter.'
  if (p.default == null) {
    return `The script uses $${p.index} on line ${p.usedOn} and declares no default. ` +
      `Give it one here, or in the script: # param ${p.index} = <text>`
  }
  return `Declared on line ${p.declaredOn} as \`${p.default}\`. Leave empty to use that.`
}

const hint = computed(() =>
  params.value.some((p) => needed(p))
    ? 'The script cannot run until the marked fields have values.'
    : 'Leave a field empty to use the default the script declares.',
)
</script>

<style scoped>
.params {
  padding: 6px 10px;
  border-bottom: 1px solid var(--line);
  background: var(--bg-subtle);
}
.params-head { display: flex; align-items: baseline; gap: 8px; }
.params-title { font-size: 12px; font-weight: 600; }
.params-hint { font-size: 11px; color: var(--fg-muted); }
.params-error {
  font-size: 12px;
  color: var(--danger);
  font-family: var(--mono);
  white-space: pre-wrap;
}

.params-fields { display: flex; flex-wrap: wrap; gap: 8px 14px; margin-top: 5px; }

.param { display: grid; grid-template-columns: auto 1fr; gap: 0 6px; align-items: center; }
.param-name { font-family: var(--mono); font-size: 12px; color: var(--fg-muted); }
.param-input {
  font-family: var(--mono);
  font-size: 12px;
  padding: 2px 5px;
  width: 13ch;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: var(--bg);
  color: var(--fg);
}
.param-stale { font-size: 11px; color: var(--fg-muted); font-style: italic; }
/* Marked rather than merely empty: this is the one that stops the run, and the
   placeholder alone reads like any other unfilled field. */
.needed .param-input { border-color: var(--warn); background: var(--warn-subtle); }
.needed .param-desc { color: var(--warn-fg); }
.param-desc {
  grid-column: 2;
  font-size: 11px;
  color: var(--fg-muted);
  max-width: 30ch;
}
</style>
