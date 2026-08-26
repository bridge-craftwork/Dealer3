<template>
  <div class="picker">
    <div class="picker-head">
      <input
        v-model="query"
        class="picker-search"
        type="search"
        placeholder="Search 340+ scenarios…"
        aria-label="Search scenarios"
      />
      <button class="picker-refresh" :disabled="loading" title="Reload the list" @click="load">↻</button>
    </div>

    <p v-if="loading" class="picker-muted">Loading scenarios…</p>
    <p v-else-if="error" class="picker-error">{{ error }}</p>
    <p v-else-if="!visible.length" class="picker-muted">
      No scenario matches “{{ query }}”.
    </p>

    <div v-else class="picker-tree">
      <div v-for="section in visible" :key="section.label" class="picker-section">
        <button class="picker-section-head" @click="toggle(section.label)">
          <span class="picker-caret">{{ isOpen(section.label) ? '▾' : '▸' }}</span>
          {{ section.label }}
          <span class="picker-count">{{ section.items.length }}</span>
        </button>

        <div v-if="isOpen(section.label)" class="picker-items">
          <button
            v-for="item in section.items"
            :key="item.file"
            class="picker-item"
            :class="{ 'is-selected': item.file === selected, 'is-busy': item.file === busyFile }"
            :title="item.description || item.file"
            @click="$emit('select', item)"
          >
            <span class="picker-item-label">{{ item.label }}</span>
            <span v-if="item.description" class="picker-item-desc">{{ item.description }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
// Browses the PBS scenario menu. Structure and metadata come from the manifest
// PBS CI builds; see lib/pbsScenarios.js.
//
// Sections start COLLAPSED, unlike Bridge-Classroom's DealLibraryPicker where
// everything starts open. A teacher's library is a handful of entries; this is
// 340+ across 20 sections, and an all-open tree is unusable. Searching expands
// automatically so matches are never hidden behind a closed section.
import { ref, computed, onMounted, watch } from 'vue'
import { fetchScenarioManifest } from '@/lib/pbsScenarios.js'

const props = defineProps({
  // Highlighted as current.
  selected: { type: String, default: '' },
  // Shown as loading while the parent fetches its script.
  busyFile: { type: String, default: '' },
})
defineEmits(['select'])

const sections = ref([])
const loading = ref(false)
const error = ref('')
const query = ref('')
const openSections = ref(new Set())

async function load() {
  loading.value = true
  error.value = ''
  try {
    const { sections: s } = await fetchScenarioManifest('release')
    sections.value = s
  } catch (e) {
    error.value = e.message || String(e)
  } finally {
    loading.value = false
  }
}
onMounted(load)

const visible = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return sections.value
  return sections.value
    .map((s) => ({
      ...s,
      items: s.items.filter(
        (i) =>
          i.label.toLowerCase().includes(q) ||
          i.file.toLowerCase().includes(q) ||
          (i.description || '').toLowerCase().includes(q),
      ),
    }))
    .filter((s) => s.items.length)
})

// While searching every matching section is open — a match hidden behind a
// collapsed heading reads as "no results".
const searching = computed(() => query.value.trim().length > 0)
function isOpen(label) {
  return searching.value || openSections.value.has(label)
}
function toggle(label) {
  const next = new Set(openSections.value)
  next.has(label) ? next.delete(label) : next.add(label)
  openSections.value = next
}

// Keep the section holding the current selection open when a search is cleared,
// so the selected scenario does not vanish.
watch(
  () => props.selected,
  (file) => {
    if (!file) return
    const owner = sections.value.find((s) => s.items.some((i) => i.file === file))
    if (owner) openSections.value = new Set(openSections.value).add(owner.label)
  },
)
</script>

<style scoped>
.picker { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.picker-head { display: flex; gap: 6px; padding: 8px; border-bottom: 1px solid var(--line); }
.picker-search {
  flex: 1; min-width: 0; padding: 6px 8px; font: inherit; font-size: 13px;
  border: 1px solid var(--line); border-radius: 4px;
  background: var(--bg); color: var(--fg);
}
.picker-refresh {
  padding: 4px 8px; border: 1px solid var(--line); border-radius: 4px;
  background: var(--bg-subtle); color: var(--fg); cursor: pointer;
}
.picker-refresh:disabled { opacity: 0.5; cursor: default; }
.picker-tree { overflow-y: auto; flex: 1; min-height: 0; }
.picker-muted { padding: 12px; color: var(--fg-muted); font-size: 13px; }
.picker-error { padding: 12px; color: var(--danger); font-size: 13px; }
.picker-section-head {
  display: flex; align-items: center; gap: 6px; width: 100%;
  padding: 6px 8px; border: 0; background: none; cursor: pointer;
  font: inherit; font-size: 13px; font-weight: 600; color: var(--fg); text-align: left;
}
.picker-section-head:hover { background: var(--bg-subtle); }
.picker-caret { width: 10px; color: var(--fg-muted); }
.picker-count { margin-left: auto; font-weight: 400; color: var(--fg-muted); font-size: 11px; }
.picker-item {
  display: block; width: 100%; padding: 5px 8px 5px 24px;
  border: 0; background: none; cursor: pointer; text-align: left;
  font: inherit; font-size: 13px; color: var(--fg);
}
.picker-item:hover { background: var(--bg-subtle); }
.picker-item.is-selected { background: var(--accent-subtle); }
.picker-item.is-busy { opacity: 0.6; }
.picker-item-label { display: block; }
.picker-item-desc {
  display: block; font-size: 11px; color: var(--fg-muted);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
</style>
