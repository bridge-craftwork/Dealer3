<template>
  <span
    ><template v-for="(part, i) in parts" :key="i"
      ><code v-if="part.code">{{ part.text }}</code><template v-else>{{ part.text }}</template
      ></template
    ></span
  >
</template>

<script setup>
// Descriptions come from the Rust vocabulary tables, where a function name is
// written in backticks — the natural thing to do in source. This renders those
// spans as code rather than putting literal backticks on the page.
//
// The template is written without whitespace between tags on purpose: this is
// inline prose, and Vue would otherwise insert spaces around every code span.
import { computed } from 'vue'
import { codeSpans } from '@/lib/reference.js'

const props = defineProps({ text: { type: String, default: '' } })
const parts = computed(() => codeSpans(props.text))
</script>

<style scoped>
code {
  font-family: var(--mono);
  font-size: 0.92em;
  background: var(--bg-subtle);
  padding: 0 3px;
  border-radius: 3px;
}
</style>
