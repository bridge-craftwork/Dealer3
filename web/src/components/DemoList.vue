<template>
  <div class="demos">
    <button
      v-for="demo in demos"
      :key="demo.id"
      class="demo"
      :class="{ 'is-selected': demo.id === selected }"
      @click="$emit('select', demo)"
    >
      <span class="demo-title">{{ demo.title }}</span>
      <span class="demo-desc">{{ demo.description }}</span>
    </button>
  </div>
</template>

<script setup>
// The Demos tab (#104). A handful of entries, so every description is shown in
// full rather than truncated to a line the way the PBS list has to be.
defineProps({
  // From lib/demos.js.
  demos: { type: Array, required: true },
  // The id of the demo the editor came from, highlighted as current.
  selected: { type: String, default: '' },
})
defineEmits(['select'])
</script>

<style scoped>
.demos { overflow-y: auto; flex: 1; min-height: 0; padding: 4px 0; }
.demo {
  display: block; width: 100%; padding: 8px 10px;
  border: 0; background: none; cursor: pointer; text-align: left;
  font: inherit; font-size: 13px; color: var(--fg);
}
.demo:hover { background: var(--bg-subtle); }
.demo.is-selected { background: var(--accent-subtle); }
.demo-title { display: block; font-weight: 600; }
.demo-desc { display: block; margin-top: 2px; font-size: 12px; line-height: 1.4; color: var(--fg-muted); }
</style>
