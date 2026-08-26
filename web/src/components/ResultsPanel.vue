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
        <table class="avg">
          <tbody>
            <tr v-for="(a, i) in result.averages" :key="i">
              <th>{{ (a.label || 'Average').trim() }}</th>
              <td>{{ formatValue(a.value) }}</td>
              <td class="avg-n">over {{ a.count.toLocaleString() }} deals</td>
            </tr>
          </tbody>
        </table>
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
        <h3>Deals</h3>
        <pre class="deals">{{ result.deals.join('\n') }}</pre>
      </section>
      <p v-else-if="!result.hitLimit" class="results-muted">
        No deals matched the condition.
      </p>
    </template>
  </div>
</template>

<script setup>
// Renders everything a script produces: the stats block, `average` results, and
// `frequency` histograms.
//
// Frequencies arrive as data rather than the CLI's ASCII table, so they are
// drawn as bars. The numbers are kept alongside — this replaces the table's
// presentation, not its precision.
const props = defineProps({
  result: { type: Object, default: null },
  error: { type: String, default: '' },
  requested: { type: Number, default: 0 },
})

/** Bars scale to the tallest bin, so a flat distribution still reads. */
function barWidth(count, freq) {
  const peak = Math.max(1, ...freq.bins.map((b) => b.count))
  return `${(count / peak) * 100}%`
}

function percent(count, total) {
  if (!total) return ''
  return `${((count / total) * 100).toFixed(1)}%`
}

/** Averages come back as full f64; six significant digits is plenty to read. */
function formatValue(v) {
  if (Number.isInteger(v)) return String(v)
  return Number(v.toPrecision(6)).toString()
}
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

.avg { border-collapse: collapse; font-size: 13px; }
.avg th { text-align: left; font-weight: 500; padding: 2px 12px 2px 0; white-space: pre; }
.avg td { font-family: var(--mono); padding: 2px 12px 2px 0; }
.avg-n { color: var(--fg-muted); font-family: inherit !important; font-size: 12px; }

.freq-outside { font-size: 12px; color: var(--warn-fg); margin: 0 0 6px; }
.freq { display: flex; flex-direction: column; gap: 2px; }
.freq-row { display: grid; grid-template-columns: 3em 1fr 4em 4em; align-items: center; gap: 8px; font-size: 12px; }
.freq-value { font-family: var(--mono); text-align: right; color: var(--fg-muted); }
.freq-bar-track { background: var(--bg-subtle); border-radius: 2px; height: 14px; overflow: hidden; }
.freq-bar { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
.freq-bar.is-zero { background: transparent; }
.freq-count { font-family: var(--mono); text-align: right; }
.freq-pct { font-family: var(--mono); text-align: right; color: var(--fg-muted); }

.deals {
  font-family: var(--mono); font-size: 12px; line-height: 1.5;
  background: var(--bg-subtle); padding: 10px; border-radius: 4px;
  overflow-x: auto; margin: 0; white-space: pre;
}
</style>
