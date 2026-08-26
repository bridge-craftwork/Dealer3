<template>
  <div class="results">
    <p v-if="error" class="results-error">{{ error }}</p>
    <p v-else-if="!result" class="results-muted">Run a script to see deals here.</p>

    <template v-else>
      <!-- The CLI's trailing stats block. -->
      <div class="stats">
        <span><strong>{{ result.generated.toLocaleString() }}</strong> generated</span>
        <span><strong>{{ result.produced.toLocaleString() }}</strong> produced</span>
        <span><strong>{{ result.seconds.toFixed(3) }}</strong> sec</span>
      </div>

      <p v-if="result.hitLimit" class="results-warn">
        Stopped at the generate limit before producing {{ requested }} deals. The filter may be
        very selective — raise the limit, or loosen the condition.
      </p>
      <p v-else-if="result.dealsTruncated" class="results-note">
        Showing the first {{ result.deals.length }} of {{ result.produced.toLocaleString() }}
        deals. Statistics below cover all of them.
      </p>

      <!-- average "label" expr -->
      <section v-if="result.averages.length" class="block">
        <h3>Averages</h3>
        <div class="avg">
          <div v-for="(a, i) in result.averages" :key="i" class="avg-row">
            <span class="avg-label">{{ (a.label || 'Average').trim() }}</span>
            <span class="avg-bar-track">
              <span class="avg-bar" :style="{ width: averageBarWidth(a.value) }"></span>
            </span>
            <span class="avg-value">{{ formatValue(a.value) }}</span>
          </div>
        </div>
      </section>

      <!-- frequency "label" (expr, min, max) -->
      <section v-for="(f, i) in result.frequencies" :key="'f' + i" class="block">
        <h3>{{ (f.label || 'Frequency').trim() }}</h3>

        <p v-if="!f.total" class="results-muted">No observations.</p>

        <template v-else>
          <!-- Values outside a declared range. Shown above the chart rather than
               tucked away: a script whose range is too narrow otherwise looks
               like it simply produced fewer deals. -->
          <p v-if="f.below || f.above" class="freq-outside">
            <span v-if="f.below">{{ f.below }} below {{ f.min }}</span>
            <span v-if="f.below && f.above"> · </span>
            <span v-if="f.above">{{ f.above }} above {{ f.max }}</span>
          </p>

          <div class="freq">
            <div v-for="bin in f.bins" :key="bin.value" class="freq-row">
              <span class="freq-value">{{ bin.value }}</span>
              <span class="freq-bar-track">
                <span
                  class="freq-bar"
                  :style="{ width: barWidth(bin.count, f) }"
                  :class="{ 'is-zero': !bin.count }"
                ></span>
              </span>
              <span class="freq-count">{{ bin.count }}</span>
              <span class="freq-pct">{{ percent(bin.count, f.total) }}</span>
            </div>
          </div>
        </template>
      </section>

      <section v-if="result.deals.length" class="block">
        <div class="deals-head">
          <h3>Deals</h3>
          <div class="deals-tools">
            <div class="toggle" role="group" aria-label="Deal view">
              <button :class="{ on: view === 'grid' }" @click="view = 'grid'">Hands</button>
              <button :class="{ on: view === 'text' }" @click="view = 'text'">Text</button>
            </div>
            <button class="dl" :disabled="downloading" @click="$emit('download', 'pbn')">
              Save PBN
            </button>
            <button class="dl" @click="$emit('download', 'text')">Save text</button>
            <!-- The browser's own print pipeline: a PDF with selectable text and
                 a live link back, which is the point — the script is meant to be
                 copied out of it. -->
            <button class="dl" @click="$emit('print')">Save PDF</button>
          </div>
        </div>

        <p v-if="view === 'grid' && !parsedDeals.length" class="results-muted">
          This output format cannot be shown as hands. Switch to Text, or generate with the
          one-line format.
        </p>
        <DealGrid v-else-if="view === 'grid'" :deals="parsedDeals" />
        <pre v-else class="deals">{{ result.deals.join('\n') }}</pre>
      </section>
      <p v-else-if="!result.hitLimit" class="results-muted">
        No deals matched the condition.
      </p>
    </template>
  </div>
</template>

<script setup>
// Renders everything a script produces: the stats block, `average` results,
// `frequency` histograms, and the deals themselves — as hands or as text.
//
// Frequencies arrive as data rather than the CLI's ASCII table, so they are
// drawn as bars. The numbers are kept alongside — this replaces the table's
// presentation, not its precision.
import { ref, computed } from 'vue'
import DealGrid from '@/components/DealGrid.vue'
import { parseOnelineDeals } from '@/lib/cardFormatting.js'
import { formatAverage } from '@/lib/format.js'

const props = defineProps({
  result: { type: Object, default: null },
  error: { type: String, default: '' },
  requested: { type: Number, default: 0 },
  downloading: { type: Boolean, default: false },
})
defineEmits(['download', 'print'])

// Hands read far more easily than one-line strings, so that is the default.
const view = ref('grid')

// Only the one-line format can be laid out as hands: printall is already a
// visual layout, and PBN is a record format. Returns [] for those, and the
// template offers Text instead rather than showing an empty grid.
const parsedDeals = computed(() => {
  if (!props.result?.deals?.length) return []
  return parseOnelineDeals(props.result.deals.join('\n'))
})

/**
 * Averages share one scale, set by the largest value shown, so the bars compare
 * against each other rather than each filling its own row.
 *
 * A negative average gets no bar rather than a misleading one: these are
 * arbitrary script expressions, and `100 * (x - y)` can legitimately go below
 * zero. The number is always shown, so nothing is hidden.
 */
const averageScale = computed(() => {
  const values = (props.result?.averages || []).map((a) => a.value)
  return Math.max(0, ...values)
})

function averageBarWidth(value) {
  const peak = averageScale.value
  if (!(peak > 0) || !(value > 0)) return '0%'
  return `${(value / peak) * 100}%`
}

/** Bars scale to the tallest bin, so a flat distribution still reads. */
function barWidth(count, freq) {
  const peak = Math.max(1, ...freq.bins.map((b) => b.count))
  return `${(count / peak) * 100}%`
}

function percent(count, total) {
  if (!total) return ''
  return `${((count / total) * 100).toFixed(1)}%`
}

const formatValue = formatAverage
</script>

<style scoped>
.results { padding: 12px; overflow-y: auto; height: 100%; min-height: 0; }
.results-muted { color: var(--fg-muted); font-size: 13px; }
.results-error {
  color: var(--danger); font-size: 13px; font-family: var(--mono);
  white-space: pre-wrap; margin: 0 0 12px;
}
.results-warn {
  background: var(--warn-subtle); border-left: 3px solid var(--warn);
  padding: 8px 10px; font-size: 13px; margin: 0 0 12px;
}
.results-note { color: var(--fg-muted); font-size: 12px; margin: 0 0 12px; }

.stats { display: flex; gap: 16px; font-size: 13px; margin-bottom: 12px; flex-wrap: wrap; }
.stats strong { font-family: var(--mono); }

.block { margin-bottom: 20px; }
.block h3 {
  font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em;
  color: var(--fg-muted); margin: 0 0 8px;
}

.avg { display: flex; flex-direction: column; gap: 3px; }
.avg-row { display: grid; grid-template-columns: minmax(6em, 14em) 1fr 5em; align-items: center; gap: 8px; font-size: 12px; }
.avg-label { white-space: pre; overflow: hidden; text-overflow: ellipsis; }
.avg-bar-track { background: var(--bg-subtle); border-radius: 2px; height: 14px; overflow: hidden; }
.avg-bar { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
.avg-value { font-family: var(--mono); text-align: right; }

.freq-outside { font-size: 12px; color: var(--warn-fg); margin: 0 0 6px; }
.freq { display: flex; flex-direction: column; gap: 2px; }
.freq-row { display: grid; grid-template-columns: 3em 1fr 4em 4em; align-items: center; gap: 8px; font-size: 12px; }
.freq-value { font-family: var(--mono); text-align: right; color: var(--fg-muted); }
.freq-bar-track { background: var(--bg-subtle); border-radius: 2px; height: 14px; overflow: hidden; }
.freq-bar { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
.freq-bar.is-zero { background: transparent; }
.freq-count { font-family: var(--mono); text-align: right; }
.freq-pct { font-family: var(--mono); text-align: right; color: var(--fg-muted); }

.deals-head { display: flex; align-items: center; gap: 12px; margin-bottom: 8px; }
.deals-head h3 { margin: 0; }
.deals-tools { display: flex; align-items: center; gap: 6px; margin-left: auto; }
.toggle { display: inline-flex; border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
.toggle button {
  border: 0; background: var(--bg); color: var(--fg-muted);
  font: inherit; font-size: 11px; padding: 3px 9px; cursor: pointer;
}
.toggle button.on { background: var(--accent); color: #fff; }
.dl {
  border: 1px solid var(--line); border-radius: 4px; background: var(--bg);
  color: var(--fg); font: inherit; font-size: 11px; padding: 3px 9px; cursor: pointer;
}
.dl:hover { background: var(--bg-subtle); }
.dl:disabled { opacity: 0.5; cursor: default; }

.deals {
  font-family: var(--mono); font-size: 12px; line-height: 1.5;
  background: var(--bg-subtle); padding: 10px; border-radius: 4px;
  overflow-x: auto; margin: 0; white-space: pre;
}
</style>
