<template>
  <div class="picker">
    <!-- Three places a script comes from, one panel. The close button stays on
         this row whichever tab is showing: it belongs to the panel, not to a list. -->
    <div class="picker-tabs" role="tablist" aria-label="Where scripts come from">
      <button
        v-for="t in TABS"
        :key="t.id"
        role="tab"
        class="picker-tab"
        :class="{ 'is-active': tab === t.id }"
        :aria-selected="tab === t.id"
        :title="t.title"
        @click="$emit('update:tab', t.id)"
      >{{ t.label }}</button>
      <button
        class="picker-close"
        title="Hide this panel, and give the width to the editor"
        aria-label="Hide the script list"
        aria-expanded="true"
        @click="$emit('close')"
      >‹</button>
    </div>

    <DemoList
      v-if="tab === 'demos'"
      :demos="demos"
      :selected="selectedDemo"
      @select="$emit('select-demo', $event)"
    />

    <HistoryList
      v-else-if="tab === 'history'"
      :entries="history"
      @open="$emit('open-revision', $event)"
      @delete="$emit('delete-entry', $event)"
    />

    <template v-else>
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

        <!-- Inside the scrolling tree, after the last scenario, so it reads as
             the end of the list rather than as chrome pinned to the panel. -->
        <p class="picker-credit">
          Scenarios courtesy of
          <a
            href="https://github.com/ADavidBailey/Practice-Bidding-Scenarios"
            target="_blank"
            rel="noopener noreferrer"
          >David Bailey</a>
        </p>
      </div>
    </template>
  </div>
</template>

<script setup>
// The panel scripts are picked from: PBS scenarios, Demos (#104) and History
// (#97).
//
// The PBS tree's structure and metadata come from the manifest PBS CI builds;
// see lib/pbsScenarios.js. Its sections start COLLAPSED, unlike
// Bridge-Classroom's DealLibraryPicker where everything starts open. A
// teacher's library is a handful of entries; this is 340+ across 20 sections,
// and an all-open tree is unusable. Searching expands automatically so matches
// are never hidden behind a closed section.
import { ref, computed, watch } from 'vue'
import { fetchScenarioManifest } from '@/lib/pbsScenarios.js'
import DemoList from '@/components/DemoList.vue'
import HistoryList from '@/components/HistoryList.vue'

const TABS = [
  { id: 'pbs', label: 'PBS', title: 'Practice Bidding Scenarios' },
  { id: 'demos', label: 'Demos', title: 'Scripts that show what dealer3 can do' },
  { id: 'history', label: 'History', title: 'Scripts you have run, kept in this browser' },
]

const props = defineProps({
  // Which tab is showing: 'pbs', 'demos' or 'history'.
  tab: { type: String, default: 'pbs' },
  // The PBS scenario highlighted as current.
  selected: { type: String, default: '' },
  // Shown as loading while the parent fetches its script.
  busyFile: { type: String, default: '' },
  // From lib/demos.js, and the id of the one the editor came from.
  demos: { type: Array, default: () => [] },
  selectedDemo: { type: String, default: '' },
  // History entries, as HistoryList takes them.
  history: { type: Array, default: () => [] },
})
defineEmits(['select', 'select-demo', 'open-revision', 'delete-entry', 'update:tab', 'close'])

const sections = ref([])
const loading = ref(false)
const error = ref('')
const query = ref('')
const openSections = ref(new Set())
let loaded = false

async function load() {
  loaded = true
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

// Fetched the first time the PBS tab shows rather than on mount: someone who
// opens the page on Demos or History has no use for the manifest yet.
watch(
  () => props.tab,
  (tab) => {
    if (tab === 'pbs' && !loaded) load()
  },
  { immediate: true },
)

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

// Open the section holding the current selection, so it does not vanish when a
// search is cleared — and so a session restored from a previous visit shows
// which scenario is loaded rather than an all-collapsed tree.
//
// Watches `sections` too: on a restore the selection is set before the manifest
// has been fetched, so there is nothing to find on the first run.
watch(
  [() => props.selected, sections],
  ([file]) => {
    if (!file) return
    const owner = sections.value.find((s) => s.items.some((i) => i.file === file))
    if (owner) openSections.value = new Set(openSections.value).add(owner.label)
  },
  { immediate: true },
)
</script>

<style scoped>
.picker { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.picker-tabs {
  display: flex; align-items: stretch; gap: 0; padding: 6px 6px 0;
  border-bottom: 1px solid var(--line);
}
/* Padding sized so three tabs and the close button fit the 260px column. */
.picker-tab {
  flex: none; padding: 5px 8px; margin-bottom: -1px;
  border: 1px solid transparent; border-radius: 4px 4px 0 0;
  background: none; cursor: pointer; font: inherit; font-size: 13px; color: var(--fg-muted);
}
.picker-tab:hover { color: var(--fg); }
.picker-tab.is-active {
  color: var(--fg); font-weight: 600;
  border-color: var(--line); border-bottom-color: var(--bg); background: var(--bg);
}
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
/* Pushed to the far end of the tab row: it acts on the panel, not on a tab. */
.picker-close {
  flex: none; margin: 0 0 5px auto;
  padding: 4px 8px; border: 1px solid var(--line); border-radius: 4px;
  background: var(--bg-subtle); color: var(--fg-muted); cursor: pointer;
  font: inherit; line-height: 1;
}
.picker-close:hover { color: var(--fg); }
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
.picker-credit {
  margin: 4px 0 0; padding: 10px 8px 14px;
  border-top: 1px solid var(--line);
  font-size: 11px; line-height: 1.5; color: var(--fg-muted);
}
.picker-credit a { color: var(--accent); }
</style>
