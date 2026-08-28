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

      <!-- The script's HandType_* variables: what nature offered against what
           the run delivered. Above the averages because when a scenario is
           levelled this is the thing it was levelled for. -->
      <section v-if="handTypes.length" class="block">
        <h3>Hand types</h3>
        <p v-if="leveling" class="ht-note">
          Levelled over {{ leveling.measured.toLocaleString() }} measured deals ·
          {{ Math.round(leveling.cost).toLocaleString() }} dealt per deal kept ·
          keeps pinned down by <strong>{{ leveling.rarest }}</strong>, seen
          {{ leveling.rarest_seen.toLocaleString() }} times
        </p>
        <div class="ht">
          <div v-for="t in handTypes" :key="t.name" class="ht-row">
            <span class="ht-label" :style="{ color: palette.get(t.name).color }">{{ t.name }}</span>
            <!-- Two segments, and which comes first says which way the type
                 moved. A type levelling up shows nature first and the gain
                 beyond it; one levelling down shows what it delivers first and
                 what it gave up beyond that. Either way the blue ends at the
                 delivered share, so the blue edges line up down the column and
                 it is the orange that is ragged. -->
            <span class="ht-track">
              <template v-if="t.delivered >= t.natural">
                <span class="ht-bar natural" :style="{ width: pct(t.natural) }"></span>
                <span class="ht-bar delivered" :style="{ width: pct(t.delivered - t.natural) }"></span>
              </template>
              <template v-else>
                <span class="ht-bar delivered" :style="{ width: pct(t.delivered) }"></span>
                <span class="ht-bar natural" :style="{ width: pct(t.natural - t.delivered) }"></span>
              </template>
            </span>
            <span class="ht-value">{{ (100 * t.delivered).toFixed(1) }}%</span>
            <span v-if="leveling" class="ht-was">was {{ (100 * t.natural).toFixed(1) }}%</span>
            <span v-else class="ht-was">{{ t.produced }} of {{ t.out_of }}</span>
          </div>
        </div>
        <!-- The blue is what this run dealt, not the share it was aiming at.
             Over a short set that is lumpy however even the keeps are — 24
             boards across 5 bands carry a standard deviation of 8 points — and
             hiding that behind the target would be drawing the intention rather
             than the result. -->
        <p v-if="leveling" class="ht-key">
          <span class="ht-swatch natural"></span> natural
          <span class="ht-swatch delivered"></span> this run of {{ handTypes[0]?.out_of }},
          levelled toward {{ (100 * handTypes[0]?.planned).toFixed(0) }}% each
        </p>
      </section>

      <!-- average "label" expr, less the ones about hand types: those say the
           same thing as the table above and would only crowd this one. -->
      <section v-if="plainAverages.length" class="block">
        <h3>Averages</h3>
        <div class="avg">
          <div v-for="(a, i) in plainAverages" :key="i" class="avg-row">
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
        <DealGrid
          v-else-if="view === 'grid'"
          :deals="parsedDeals"
          :types="result.dealTypes || []"
          :palette="palette"
        />
        <template v-else>
          <!-- `printes` is the script's own output. It goes above the deals
               because that is what it is usually for: a line summarising each
               deal, which is easier to read as a block than interleaved. -->
          <pre v-if="result.printes" class="deals printes">{{ result.printes }}</pre>
          <!-- Tinted by hand type and not named: the colours alone show the run
               walking through the types, which naming them would only say
               again in words. -->
          <pre class="deals"><span
            v-for="(deal, i) in result.deals"
            :key="i"
            class="deal-line"
            :style="{ background: palette.get((result.dealTypes || [])[i]).tint }"
          >{{ deal }}\n</span></pre>
        </template>
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
import { handTypePalette } from '@/lib/handTypes.js'

const props = defineProps({
  result: { type: Object, default: null },
  error: { type: String, default: '' },
  requested: { type: Number, default: 0 },
  downloading: { type: Boolean, default: false },
})
defineEmits(['download', 'print'])

// Hands read far more easily than one-line strings, so that is the default.
const view = ref('grid')

// The script's hand types, in the order it declares them, which is the order
// their colours follow.
const handTypes = computed(() => props.result?.handTypes || [])

const leveling = computed(() => props.result?.leveling || null)

// One palette for the whole panel, so a type is the same colour in its row, on
// its board and behind its line — which is what shows the run walking through
// the types rather than meeting them as they fall.
const palette = computed(() => handTypePalette(handTypes.value.map((t) => t.name)))

// Averages about hand types are the hand-type table said twice, so they are
// left to it. The engine marks them, since a label is prose and cannot be asked.
const plainAverages = computed(() =>
  (props.result?.averages || []).filter((a) => !a.is_hand_type),
)

/// The longest bar in the table, which every row is drawn against.
///
/// A natural mix runs from a couple of percent to nearly sixty, so a 0-100
/// scale leaves the interesting end squashed into the first inch. Scaling to
/// the longest row spends the width on the rows rather than on the empty space
/// past them.
const htScale = computed(() => {
  const longest = Math.max(
    ...handTypes.value.map((t) => Math.max(t.natural, t.delivered)),
    0.01,
  )
  return longest
})

/// A share of the deals as a CSS width, against the longest row.
function pct(share) {
  return `${Math.max(0, share / htScale.value) * 100}%`
}

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
/* Hand types. Every column but the bar sizes to its own content, so the label
   sits against its bar rather than across a gap of reserved space, and the
   count on the right never has to wrap — the bar gives up the width instead,
   being the one thing here that can lose some and still say what it says. */
/* One grid for the whole table, the rows `display: contents`, so the four
   columns are shared tracks rather than each row sizing its own. A row-level
   grid let a shorter count on one line widen that row's bar by eight pixels,
   and bars that start and end in slightly different places are worse than
   bars that are slightly narrower. */
.ht {
  display: grid;
  grid-template-columns: auto minmax(4rem, 1fr) auto auto;
  align-items: center;
  gap: 6px 10px;
}
.ht-row { display: contents; }
.ht-label { font-family: var(--mono); font-size: 0.86rem; font-weight: 600; }
.ht-track {
  display: flex;
  background: var(--bg-subtle);
  border-radius: 2px;
  height: 14px;
  overflow: hidden;
}
.ht-bar { display: block; height: 100%; }
.ht-bar.natural { background: var(--natural); }
.ht-bar.delivered { background: var(--accent); }
.ht-value {
  font-variant-numeric: tabular-nums;
  text-align: right;
  font-size: 0.85rem;
  white-space: nowrap;
}
.ht-was {
  font-variant-numeric: tabular-nums;
  color: var(--fg-muted);
  font-size: 0.78rem;
  white-space: nowrap;
  text-align: right;
}
.ht-note { color: var(--fg-muted); font-size: 0.8rem; margin: 0 0 8px; }
.ht-key { color: var(--fg-muted); font-size: 0.78rem; margin: 8px 0 0; }
.ht-swatch {
  display: inline-block;
  width: 10px;
  height: 10px;
  border-radius: 2px;
  margin: 0 4px 0 10px;
  vertical-align: -1px;
}
.ht-key .ht-swatch:first-child { margin-left: 0; }
.ht-swatch.natural { background: var(--natural); }
.ht-swatch.delivered { background: var(--accent); }

/* One deal per span so each can carry its type's tint. `pre` keeps the
   whitespace; the spans are inline so a multi-line format still reads. */
.deal-line { display: block; border-radius: 2px; }

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

.printes {
  border-left: 2px solid var(--accent);
  padding-left: 10px;
  margin-bottom: 12px;
}

.deals {
  font-family: var(--mono); font-size: 12px; line-height: 1.5;
  background: var(--bg-subtle); padding: 10px; border-radius: 4px;
  overflow-x: auto; margin: 0; white-space: pre;
}
</style>
