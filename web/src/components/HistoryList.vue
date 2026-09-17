<template>
  <div class="history">
    <p v-if="!entries.length" class="history-empty">
      Nothing here yet. Each script you run is kept — its last {{ revisionsKept }} versions,
      for the {{ scriptsKept }} scripts you used most recently — in this browser only.
    </p>

    <div v-for="entry in entries" :key="entry.id" class="entry" :class="{ 'is-current': entry.current }">
      <div class="entry-row">
        <button
          v-if="entry.revisions.length > 1"
          class="entry-caret"
          :aria-expanded="isOpen(entry.id)"
          :title="isOpen(entry.id) ? 'Hide earlier versions' : 'Show earlier versions'"
          @click="toggle(entry.id)"
        >{{ isOpen(entry.id) ? '▾' : '▸' }}</button>
        <span v-else class="entry-caret-space" />

        <button class="entry-open" :title="`Open the latest version of ${entry.label}`" @click="open(entry.id, 0)">
          <span class="entry-label">{{ entry.label }}</span>
          <span class="entry-meta">
            {{ ago(entry.revisions[0]) }}<template v-if="entry.revisions.length > 1">
              · {{ entry.revisions.length }} versions</template>
          </span>
        </button>

        <button class="entry-delete" :title="`Forget ${entry.label}`" :aria-label="`Forget ${entry.label}`" @click="$emit('delete', entry.id)">×</button>
      </div>

      <div v-if="isOpen(entry.id)" class="revisions">
        <button
          v-for="(at, index) in entry.revisions"
          :key="at + ':' + index"
          class="revision"
          @click="open(entry.id, index)"
        >{{ index === 0 ? 'Latest' : `Earlier, ${ago(at)}` }}<template v-if="index === 0"> · {{ ago(at) }}</template></button>
      </div>
    </div>
  </div>
</template>

<script setup>
// The History tab (#97). What counts as one script, and how many versions of it
// are kept, is lib/history.js; this only lists what the page hands it.
import { ref } from 'vue'
import { MAX_REVISIONS, MAX_SCRIPTS } from '@/lib/history.js'

defineProps({
  // `{ id, label, revisions: [at, ...], current }`, most recently used first.
  entries: { type: Array, required: true },
})
const emit = defineEmits(['open', 'delete'])

const revisionsKept = MAX_REVISIONS
const scriptsKept = MAX_SCRIPTS

const openEntries = ref(new Set())
const isOpen = (id) => openEntries.value.has(id)
function toggle(id) {
  const next = new Set(openEntries.value)
  next.has(id) ? next.delete(id) : next.add(id)
  openEntries.value = next
}

function open(id, index) {
  emit('open', { id, index })
}

/** "just now", "5 min ago", "3 h ago", "2 days ago", or a date. */
function ago(at) {
  const seconds = Math.max(0, (Date.now() - at) / 1000)
  if (seconds < 60) return 'just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} h ago`
  const days = Math.floor(seconds / 86400)
  if (days < 14) return days === 1 ? 'yesterday' : `${days} days ago`
  return new Date(at).toLocaleDateString()
}
</script>

<style scoped>
.history { overflow-y: auto; flex: 1; min-height: 0; padding: 4px 0; }
.history-empty { padding: 12px; margin: 0; color: var(--fg-muted); font-size: 13px; line-height: 1.5; }
.entry.is-current { background: var(--accent-subtle); }
.entry-row { display: flex; align-items: stretch; }
.entry-caret, .entry-caret-space { flex: none; width: 22px; }
.entry-caret {
  border: 0; background: none; cursor: pointer; color: var(--fg-muted);
  font: inherit; font-size: 12px; padding: 0;
}
.entry-open {
  flex: 1; min-width: 0; padding: 6px 4px;
  border: 0; background: none; cursor: pointer; text-align: left;
  font: inherit; font-size: 13px; color: var(--fg);
}
.entry-row:hover { background: var(--bg-subtle); }
.entry.is-current .entry-row:hover { background: transparent; }
.entry-label { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.entry-meta { display: block; font-size: 11px; color: var(--fg-muted); }
.entry-delete {
  flex: none; width: 28px; border: 0; background: none; cursor: pointer;
  color: var(--fg-muted); font: inherit; font-size: 15px; line-height: 1;
}
.entry-delete:hover { color: var(--danger); }
.revisions { padding: 0 0 4px 26px; }
.revision {
  display: block; width: 100%; padding: 3px 4px;
  border: 0; background: none; cursor: pointer; text-align: left;
  font: inherit; font-size: 12px; color: var(--fg-muted);
}
.revision:hover { background: var(--bg-subtle); color: var(--fg); }
</style>
