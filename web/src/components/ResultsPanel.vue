<template>
  <div class="results">
    <p v-if="error" class="results-error">{{ error }}</p>
    <p v-else-if="!result" class="results-muted">Run a script to see deals here.</p>

    <template v-else>
      <!-- The CLI's trailing stats block. -->
      <div class="stats">
        <!-- Every deal the run looked at, both passes. It counted only the
             second until 2026-08-29, which read as 1,701 beside "levelled over
             52,771 measured deals" — the same deals, counted honestly. -->
        <span :title="generatedHint">
          <strong>{{ result.generated.toLocaleString() }}</strong> generated
          <span v-if="measuredThisRun" class="stats-split">
            = {{ measuredThisRun.characterized.toLocaleString() }} characterizing
            + {{ additionalGenerated.toLocaleString() }} additional
          </span>
        </span>
        <span><strong>{{ result.produced.toLocaleString() }}</strong> produced</span>
        <!-- Split when levelling, because the run then deals the scenario
             twice — once to find out what it does, once to do it — and a
             single total makes the second look slow when most of the wait was
             the first. -->
        <span v-if="measuredThisRun" :title="timingHint">
          <strong>{{ result.seconds.toFixed(2) }}</strong> sec
          <span class="stats-split">
            = {{ measuredThisRun.measure_seconds.toFixed(2) }} characterizing
            + {{ producedSeconds.toFixed(2) }} additional dealing
          </span>
        </span>
        <span v-else><strong>{{ result.seconds.toFixed(3) }}</strong> sec</span>
      </div>

      <!-- Not while a round was being dealt: the hand types below say which
           ones ran out, which is the same news with the part that matters in
           it. Two warnings saying it once each read as two problems. -->
      <p v-if="result.hitLimit && !dealingRounds" class="results-warn">
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
          {{ leveling.rarest_seen.toLocaleString() }} times<template v-if="precision">,
          <strong :class="precisionClass">±{{ precision }}</strong> on that rate</template>
        </p>
        <!-- A thin measurement is the one error levelling cannot recover from:
             the keep is `mix / natural`, so an error in a rate measured on too
             little is baked in for good rather than averaging out. It does not
             make the run wrong, so it warns rather than refusing — but it has
             to be visible, because nothing else on the page looks amiss. -->
        <p v-for="(w, i) in leveling?.warnings || []" :key="i" class="ht-warn">
          {{ w.replace(/\s+/g, ' ') }}
        </p>
        <!-- One chart when there is one thing to show, two when a levelling
             gives nature and the run something to be read against each other.
             Both are drawn to the same scale — see `htScale` — so a bar in one
             means what a bar of that length means in the other. -->
        <div class="ht-charts">
          <div v-for="chart in charts" :key="chart.key" class="ht-chart">
            <h4 v-if="chart.title" class="ht-chart-title">{{ chart.title }}</h4>
            <div class="ht" :class="{ counted: chart.counts }">
              <div v-for="t in handTypes" :key="t.name" class="ht-row">
                <span class="ht-label" :style="{ color: palette.get(t.name).color }">
                  {{ t.name }}
                </span>
                <span class="ht-track">
                  <span
                    class="ht-bar"
                    :class="chart.tone"
                    :style="{ width: pct(chart.share(t)) }"
                  ></span>
                </span>
                <span class="ht-value">{{ (100 * chart.share(t)).toFixed(1) }}%</span>
                <!-- Against what the round owed it rather than against the run:
                     "3 of 4" says a board is missing, where "3 of 11" says
                     nothing at all. A type holding the remainder shows one more,
                     which is the partial round and not a shortfall. -->
                <template v-if="chart.counts">
                  <span
                    v-if="t.wanted != null"
                    class="ht-was"
                    :class="{ 'ht-short': t.produced < t.wanted }"
                  >{{ t.produced }} of {{ t.wanted }}</span>
                  <span v-else class="ht-was">{{ t.produced }} of {{ t.out_of }}</span>
                </template>
              </div>
            </div>
          </div>
        </div>
        <!-- A round that could not be filled. Not an error — a short set is
             still a set — but nothing else on the page would say which types
             ran out, and that is what decides whether to raise the limit or to
             widen a category that is rarer than its author thought. -->
        <p v-if="shortOfRound.length" class="ht-warn">
          Ran out of deals before filling {{ shortOfRound.join(', ') }}. Raise Max generate, or
          widen the categories that came up short.
        </p>
        <!-- The deal count is the run's whole cost, which for a levelled run is
             nearly all measuring — the note above already accounts for that, so
             saying it here again would read as the round having been expensive. -->
        <p v-else-if="roundFilled" class="ht-key">
          <template v-if="rounds.even">Exactly {{ rounds.rounds }} of each</template>
          <template v-else>{{ rounds.rounds }} complete rounds</template><template
            v-if="rounds.remainder"
          >, and a partial round of {{ rounds.remainder }}</template><template
            v-if="!leveling"
          >, dealt from {{ result.generated.toLocaleString() }} deals</template>.
        </p>
        <!-- What the run dealt is not the share it was aiming at. Over a short
             set that is lumpy however even the keeps are — 24 boards across 5
             bands carry a standard deviation of 8 points — so the target is
             said in words here rather than drawn as a third bar nobody asked
             for. -->
        <p v-if="leveling" class="ht-key">
          Levelled toward
          <template v-if="evenTarget">{{ (100 * handTypes[0].planned).toFixed(0) }}% each</template>
          <template v-else>the shares the scenario declares</template>.<template
            v-if="!dealingRounds"
          >
            A run of {{ handTypes[0]?.out_of }} is lumpy around that however even the keeps
            are.</template>
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
          <p v-if="!f.grid && (f.below || f.above)" class="freq-outside">
            <span v-if="f.below">{{ f.below }} below {{ f.min }}</span>
            <span v-if="f.below && f.above"> · </span>
            <span v-if="f.above">{{ f.above }} above {{ f.max }}</span>
          </p>

          <!-- Two-dimensional: a grid of counts shaded by magnitude, which is
               what a cross-tabulation is. Drawn as a table because it is one —
               the row and column headers are the two expressions' values, and
               a screen reader gets them for free. -->
          <div v-if="f.grid" class="heat-scroll">
            <table class="heat">
              <caption class="sr-only">
                Counts by {{ (f.label || 'the two expressions').trim() }}
              </caption>
              <thead>
                <tr>
                  <th scope="col"><span class="sr-only">Value</span></th>
                  <th v-for="label in gridColumnLabels(f.grid)" :key="'c' + label" scope="col">
                    {{ label }}
                  </th>
                  <th scope="col" class="heat-sum">Sum</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(row, r) in f.grid.counts" :key="'r' + r">
                  <th scope="row">{{ gridRowLabels(f.grid)[r] }}</th>
                  <!-- An empty cell is left blank rather than printed as a
                       zero. On a sparse grid the zeros are the majority, and
                       setting them in ink makes the reader subtract them back
                       out; blanking them lets the counts that exist carry the
                       shape on their own, without leaning on the colour. The
                       value stays for a screen reader, which cannot see the
                       blank and would otherwise hear a gap it must interpret. -->
                  <td
                    v-for="(count, c) in row"
                    :key="'c' + c"
                    :style="heatCell(count, gridPeak(f.grid))"
                    :title="heatTitle(f, r, c, count)"
                    :class="{ 'is-zero': !count }"
                  >
                    <template v-if="count">{{ count }}</template>
                    <span v-else class="sr-only">0</span>
                  </td>
                  <td class="heat-sum" :class="{ 'is-zero': !gridRowSum(row) }">
                    <template v-if="gridRowSum(row)">{{ gridRowSum(row) }}</template>
                    <span v-else class="sr-only">0</span>
                  </td>
                </tr>
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row">Sum</th>
                  <td
                    v-for="(sum, c) in gridColumnSums(f.grid)"
                    :key="'s' + c"
                    class="heat-sum"
                    :class="{ 'is-zero': !sum }"
                  >
                    <template v-if="sum">{{ sum }}</template>
                    <span v-else class="sr-only">0</span>
                  </td>
                  <td class="heat-sum heat-total">{{ gridTotal(f.grid) }}</td>
                </tr>
              </tfoot>
            </table>
          </div>

          <div v-else class="freq">
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
import {
  rowLabels as heatRowLabels,
  columnLabels as heatColumnLabels,
  peak as heatPeak,
  rowSum as heatRowSum,
  columnSums as heatColumnSums,
  total as heatTotal,
  cellStyle as heatCellStyle,
} from '@/lib/heatmap.js'

const props = defineProps({
  result: { type: Object, default: null },
  /// The levelling behind the run, held by the page rather than read off the
  /// result: running the levelled scenario on its own returns none, and the
  /// natural rates it measured are still what the orange bars mean.
  leveling: { type: Object, default: null },
  error: { type: String, default: '' },
  requested: { type: Number, default: 0 },
  downloading: { type: Boolean, default: false },
})
defineEmits(['download', 'print'])

// Hands read far more easily than one-line strings, so that is the default.
const view = ref('grid')

// The script's hand types, in the order it declares them, which is the order
// their colours follow.
// This run's counts, with the natural rates taken from the levelling that
// produced the script. A re-run of the levelled scenario measures nothing, so
// its own idea of "natural" is just what it delivered.
const handTypes = computed(() => {
  const run = props.result?.handTypes || []
  const measured = new Map((props.leveling?.shares || []).map((s) => [s.name, s.natural]))
  return run.map((t) => (measured.has(t.name) ? { ...t, natural: measured.get(t.name) } : t))
})

const leveling = computed(() => props.leveling)

/// The levelling *this* run did, which is not the one the bars are drawn from.
///
/// `leveling` is held by the page across runs, so the natural rates survive a
/// re-run of the levelled scenario — that run measures nothing and would
/// otherwise have no idea what "natural" meant. Timings cannot be held the same
/// way: they belong to the run on the clock. Reading the held one gave
/// `0.99 sec = 6.01 measuring + 0.00 dealing` on a Leveled-tab re-run, the 6.01
/// being left over from the levelling that produced the script.
const measuredThisRun = computed(() => props.result?.leveling || null)

/// Deals the producing pass had to deal for itself.
///
/// Usually none, and that is the point: a levelled scenario is the
/// characterizing pass's scenario with the keeps added, so every deal it can
/// produce is one that pass already dealt. It deals its own only when more were
/// wanted than were kept.
const additionalGenerated = computed(() =>
  Math.max(0, (props.result?.generated || 0) - (measuredThisRun.value?.characterized || 0)),
)

/// What the run itself cost, once the characterizing pass is taken off.
///
/// Subtracted rather than timed separately: the two together are the number on
/// the clock, and a reader comparing this against a re-run of the levelled
/// scenario on its own should find the second figure, not the total.
const producedSeconds = computed(() =>
  Math.max(0, (props.result?.seconds || 0) - (measuredThisRun.value?.measure_seconds || 0)),
)

/// How well the rarest category's rate is known, which is the number that says
/// whether a levelling is worth trusting.
///
/// The command line has always printed this; the page printed the sighting
/// count alone, which asks a reader to know that 61 sightings is ±13% and 2,000
/// is ±2.2%. A keep is `mix / natural`, so this error is baked into the
/// delivered mix and does not average out with a longer run.
const precision = computed(() => {
  const e = props.leveling?.rarest_error
  if (typeof e !== 'number' || !isFinite(e)) return null
  // A decimal where the number is small enough for one to mean something, as
  // the command line prints it: `2.2%` reads as a measurement, `2%` as a
  // rounding. Past ten percent the decimal is noise.
  const percent = e * 100
  return percent < 10 ? `${percent.toFixed(1)}%` : `${percent.toFixed(0)}%`
})

/// Coloured once it is worth noticing, and not before.
///
/// One threshold rather than a scale: at a tenth or worse the mix this delivers
/// can be a couple of points off its target and stay there, which is worth an
/// eye; above that the number speaks for itself. An earlier version dimmed the
/// middle band, which had it backwards — a measurement that is merely adequate
/// should not be quieter than a good one.
const precisionClass = computed(() => {
  const e = props.leveling?.rarest_error
  return typeof e === 'number' && isFinite(e) && e >= 0.1 ? 'ht-thin' : ''
})

const timingHint = computed(
  () =>
    'Levelling deals the scenario twice: once to characterize it — how often each hand type ' +
    'comes up — then again to produce the deals. Running the levelled scenario on its own ' +
    'costs only the second.',
)

const generatedHint = computed(() =>
  measuredThisRun.value
    ? 'Every deal the run looked at. Characterizing is nearly all of it: the deals it produces ' +
      'are a filter over the deals that pass already dealt, so they are re-used rather than ' +
      'dealt again, and the second figure is only what was still wanted after that.'
    : 'Deals examined, including those the condition rejected.',
)

// One palette for the whole panel, so a type is the same colour in its row, on
// its board and behind its line — which is what shows the run walking through
// the types rather than meeting them as they fall.
/// Whether this run was dealing a round robin at all.
const dealingRounds = computed(() => handTypes.value.some((t) => t.wanted != null))

/// Hand types the round could not fill.
const shortOfRound = computed(() =>
  handTypes.value.filter((t) => t.wanted != null && t.produced < t.wanted).map((t) => t.name),
)

/// Whether every type got what the round owed it, which is the ordinary outcome
/// and the one worth confirming: a set that is exact does not look any
/// different from one that is nearly exact.
const roundFilled = computed(() => dealingRounds.value && shortOfRound.value.length === 0)

/// The round robin's shape, from the engine rather than inferred: how many
/// complete rounds, how many deals were left over, and whether the rounds were
/// even or weighted by `HandType_X_Share`. A weighted run has no "N of each" to
/// report, so the wording turns on it.
const rounds = computed(() => props.result?.roundRobin ?? null)

const palette = computed(() => handTypePalette(handTypes.value.map((t) => t.name)))

// Averages about hand types are the hand-type table said twice, so they are
// left to it. The engine marks them, since a label is prose and cannot be asked.
const plainAverages = computed(() =>
  (props.result?.averages || []).filter((a) => !a.is_hand_type),
)

/// The charts to draw: one, or two when a levelling gives nature and the run
/// something to be read against each other.
///
/// They were one chart with two segments stacked in it, and which segment came
/// first said which way a type had moved. Readable once explained, which is the
/// problem — a reader who has not had it explained sees two bars of different
/// colours and no reason for the order. Two charts say the same thing by being
/// two charts.
const charts = computed(() => {
  if (!leveling.value) {
    return [{ key: 'run', title: null, tone: 'delivered', counts: true, share: (t) => t.delivered }]
  }
  return [
    { key: 'natural', title: 'Natural', tone: 'natural', counts: false, share: (t) => t.natural },
    {
      key: 'run',
      title: `This run of ${props.result?.produced ?? 0}`,
      tone: 'delivered',
      counts: false,
      share: (t) => t.delivered,
    },
  ]
})

/// Whether the levelling was aiming at an even mix, which is nearly always. A
/// scenario declaring `HandType_X_Share` is not, and "20% each" would be a lie
/// about it.
const evenTarget = computed(() => {
  const planned = handTypes.value.map((t) => t.planned)
  return planned.every((p) => Math.abs(p - planned[0]) < 1e-9)
})

/// The longest bar in either chart, which every row of both is drawn against.
///
/// Shared deliberately: two charts side by side are only worth having if a bar
/// in one means what a bar of that length means in the other. Scaling each to
/// its own longest row would draw a type at 43% and a type at 23% the same
/// width and say nothing about the difference.
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

// The grid's arithmetic and its colour ramp, kept in lib so they can be
// tested without a DOM — see `heatmap.test.js`.
const gridRowLabels = heatRowLabels
const gridColumnLabels = heatColumnLabels
const gridPeak = heatPeak
const gridRowSum = heatRowSum
const gridColumnSums = heatColumnSums
const gridTotal = heatTotal
const heatCell = heatCellStyle

function heatTitle(f, r, c, count) {
  const rows = gridRowLabels(f.grid)
  const columns = gridColumnLabels(f.grid)
  const of = f.total ? ` — ${percent(count, f.total)} of ${f.total}` : ''
  return `${rows[r]} × ${columns[c]}: ${count}${of}`
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
/* Side by side while there is room for both, stacked when there is not — and
   at the width the deals below need for three hands, so the panel does not
   change its mind about how wide it is halfway down. That grid is
   `minmax(240px, 1fr)` with a 10px gap, so three hands want 740px; two charts
   of 359 plus this gap want the same. `auto-fit` measures the panel rather
   than the window, which is what matters when the panel is a column. */
.ht-charts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(359px, 1fr));
  gap: 10px 22px;
  align-items: start;
}
.ht-chart-title {
  margin: 0 0 6px;
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--fg-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.ht {
  display: grid;
  grid-template-columns: auto minmax(4rem, 1fr) auto;
  align-items: center;
  gap: 6px 10px;
}
/* The count column, which only the chart of what was dealt carries. */
.ht.counted { grid-template-columns: auto minmax(4rem, 1fr) auto auto; }
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
/* A type that came up short. The count is already there; this only stops it
   reading as muted furniture like the ones that filled. */
.ht-short { color: var(--warn, #b45309); font-weight: 600; }

.ht-was {
  font-variant-numeric: tabular-nums;
  color: var(--fg-muted);
  font-size: 0.78rem;
  white-space: nowrap;
  text-align: right;
}
.ht-note { color: var(--fg-muted); font-size: 0.8rem; margin: 0 0 8px; }
.ht-key { color: var(--fg-muted); font-size: 0.78rem; margin: 8px 0 0; }
/* Amber rather than red: the levelling ran and its numbers are on screen —
   this is about how much to trust them, not about a failure. */
/* Quieter than the numbers it explains: the total is the figure, the split is
   the reason. */
.stats-split {
  color: var(--muted, #777);
  font-size: 0.86em;
}

/* A rate known to a tenth or worse: the mix this delivers can be a couple of
   points off its target, permanently. Not an error, so it is coloured rather
   than boxed. */
.ht-thin { color: var(--warn, #b45309); }
.ht-warn {
  margin: 0.35rem 0 0.6rem;
  padding: 0.45rem 0.6rem;
  border-left: 3px solid #c8860d;
  background: #fdf6e7;
  color: #6b4e08;
  font-size: 0.82rem;
  line-height: 1.45;
  border-radius: 0 3px 3px 0;
}


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

/* A two-dimensional frequency: counts in a grid, shaded by magnitude. */
.heat-scroll { overflow-x: auto; }
.heat {
  border-collapse: separate;
  /* A 2px gap of surface between fills, so adjacent cells of similar weight
     stay distinguishable rather than merging into a block. */
  border-spacing: 2px;
  font-size: 12px;
  font-family: var(--mono);
}
.heat th {
  font-weight: 500;
  color: var(--fg-muted);
  text-align: right;
  padding: 2px 6px;
  white-space: nowrap;
}
.heat thead th { text-align: center; }
.heat td {
  min-width: 2.6em;
  padding: 4px 6px;
  text-align: right;
  border-radius: 3px;
  background: var(--bg-subtle);
  color: var(--fg);
  font-variant-numeric: tabular-nums;
}
/* An empty cell recedes to the surface and is left blank: on a sparse grid,
   nothing should look like nothing rather than like a faint something, and a
   printed zero is neither. */
.heat td.is-zero { background: transparent; }
/* The margins are totals, not observations. Keeping them off the colour ramp
   stops the largest numbers in the table from owning the darkest end of it and
   flattening the cells the grid is actually about. */
.heat .heat-sum {
  background: transparent;
  color: var(--fg-muted);
  border-left: 1px solid var(--line);
}
.heat tfoot .heat-sum { border-left: none; border-top: 1px solid var(--line); }
.heat tfoot .heat-total { border-left: 1px solid var(--line); color: var(--fg); }

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

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
