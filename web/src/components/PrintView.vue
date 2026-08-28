<template>
  <!-- Screen-hidden; the print stylesheet reveals this and hides the app. -->
  <div class="print-only print-doc">
    <header class="p-head">
      <h1>{{ title }}</h1>
      <!-- Same order as the controls on screen, and for the same reason: the
           seed is the one to read rather than set, and it is what identifies
           this sheet if anyone wants to deal it again. -->
      <dl class="p-params">
        <div><dt>Produce</dt><dd>{{ params.produce }}</dd></div>
        <div><dt>Max generate</dt><dd>{{ params.maxGenerate.toLocaleString() }}</dd></div>
        <div><dt>Format</dt><dd>{{ params.format }}</dd></div>
        <div class="p-seed"><dt>Seed</dt><dd>{{ params.seed }}</dd></div>
      </dl>
    </header>

    <section class="p-block p-script">
      <h2>Script</h2>
      <!-- Coloured with the engine's own tokenizer rather than CodeMirror's
           generated classes, which are not stable to target. Palette is tuned
           for paper: the editor's dark theme inverts badly when printed. -->
      <pre><code><span
        v-for="(tok, i) in scriptTokens"
        :key="i"
        :class="tok[0] ? 'tk-' + tok[0] : null"
      >{{ tok[1] }}</span></code></pre>
    </section>

    <section v-if="result" class="p-block">
      <h2>Results</h2>
      <p class="p-stats">
        <strong>{{ result.generated.toLocaleString() }}</strong> generated ·
        <strong>{{ result.produced.toLocaleString() }}</strong> produced ·
        {{ result.seconds.toFixed(3) }} sec
      </p>

      <template v-if="result.averages.length">
        <h3>Averages</h3>
        <table class="p-table">
          <tbody>
            <tr v-for="(a, i) in result.averages" :key="i">
              <th>{{ (a.label || 'Average').trim() }}</th>
              <td>{{ formatValue(a.value) }}</td>
            </tr>
          </tbody>
        </table>
      </template>

      <template v-for="(f, i) in result.frequencies" :key="'f' + i">
        <h3>{{ (f.label || 'Frequency').trim() }}</h3>
        <p v-if="f.below || f.above" class="p-outside">
          <span v-if="f.below">{{ f.below }} below {{ f.min }}</span>
          <span v-if="f.below && f.above"> · </span>
          <span v-if="f.above">{{ f.above }} above {{ f.max }}</span>
        </p>
        <table class="p-table p-freq">
          <tbody>
            <tr v-for="bin in f.bins" :key="bin.value">
              <th>{{ bin.value }}</th>
              <td class="p-bar-cell">
                <!-- A filled div rather than a chart: prints as vector, needs
                     print-color-adjust so the fill is not dropped. -->
                <span class="p-bar" :style="{ width: barWidth(bin.count, f) }"></span>
              </td>
              <td class="p-count">{{ bin.count }}</td>
            </tr>
          </tbody>
        </table>
      </template>
    </section>

    <section v-if="deals.length" class="p-block">
      <h2>
        Deals
        <span v-if="truncated" class="p-note">
          (first {{ deals.length }} of {{ result.produced.toLocaleString() }})
        </span>
      </h2>
      <div class="p-grid">
        <article v-for="(deal, i) in deals" :key="i" class="p-board">
          <div class="p-board-no">{{ i + 1 }}</div>
          <div class="p-compass">
            <div class="p-seat p-n"><PrintHand label="N" :hand="deal.north" /></div>
            <div class="p-seat p-w"><PrintHand label="W" :hand="deal.west" /></div>
            <div class="p-seat p-e"><PrintHand label="E" :hand="deal.east" /></div>
            <div class="p-seat p-s"><PrintHand label="S" :hand="deal.south" /></div>
          </div>
        </article>
      </div>
    </section>

    <footer class="p-foot">
      Generated with dealer3 — <a :href="siteUrl">{{ siteUrl }}</a>.
      Paste the script above into the editor to reproduce this.
    </footer>
  </div>
</template>

<script setup>
// The printed document. Hidden on screen; `@media print` swaps it in.
//
// A separate component rather than print rules over the live UI: what belongs
// on paper is genuinely different — no picker, no controls, the script as a
// static listing, and only the first few boards. Trying to coerce the running
// app into that shape with CSS alone would be more fragile and less legible.
import { computed, h } from 'vue'
import { SUIT_ORDER, SUIT_SYMBOLS, RED_SUITS, parseOnelineDeals } from '@/lib/cardFormatting.js'
import { dlrStreamParser, tokenizeLine } from '@/lib/dlrLanguage.js'
import { languageInfo, isReady } from '@/lib/engine.js'
import { formatAverage } from '@/lib/format.js'

const props = defineProps({
  script: { type: String, default: '' },
  result: { type: Object, default: null },
  params: { type: Object, required: true },
  scenario: { type: String, default: '' },
  // Only so the token colouring recomputes once the engine has loaded.
  engineReady: { type: Boolean, default: false },
})

/** A run of 500 deals is not a document. Enough to show the shape of the set. */
const MAX_PRINTED_BOARDS = 12

const siteUrl = 'https://dealer.bridge-classroom.org'

const title = computed(() =>
  props.scenario ? props.scenario.replace(/_/g, ' ') : 'Dealer script',
)

const scriptTokens = computed(() => {
  // Colouring needs the engine's vocabulary, and this component renders before
  // the wasm module has finished loading. The script matters more than its
  // colours, so print it uncoloured rather than not at all.
  // `engineReady` is a prop purely so this recomputes when the engine arrives.
  if (!props.engineReady || !isReady()) return [[null, props.script]]

  const parser = dlrStreamParser(languageInfo())
  const state = parser.startState()
  const out = []
  for (const line of props.script.split('\n')) {
    out.push(...tokenizeLine(parser, line, state))
    out.push([null, '\n'])
  }
  return out
})

const deals = computed(() => {
  if (!props.result?.deals?.length) return []
  return parseOnelineDeals(props.result.deals.join('\n')).slice(0, MAX_PRINTED_BOARDS)
})

const truncated = computed(
  () => !!props.result && props.result.produced > deals.value.length && deals.value.length > 0,
)

function barWidth(count, freq) {
  const peak = Math.max(1, ...freq.bins.map((b) => b.count))
  return `${(count / peak) * 100}%`
}

const formatValue = formatAverage

const PrintHand = (p) =>
  h('div', { class: 'p-hand' }, [
    h('div', { class: 'p-hand-head' }, `${p.label} · ${p.hand.hcp}`),
    ...SUIT_ORDER.map((suit) =>
      h('div', { class: ['p-suit', RED_SUITS.has(suit) ? 'is-red' : ''] }, [
        h('span', { class: 'p-sym' }, SUIT_SYMBOLS[suit]),
        h('span', {}, p.hand[suit].length ? p.hand[suit].join('') : '—'),
      ]),
    ),
  ])
PrintHand.props = ['label', 'hand']
</script>
