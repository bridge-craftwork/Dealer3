<template>
  <div class="app">
    <header class="bar">
      <h1>dealer3</h1>
      <span class="bar-sub">bridge hand generator — runs entirely in your browser</span>
      <span class="bar-spacer"></span>
      <!-- Opens in its own tab: looking a function up is something you do while
           part-way through writing a script, and losing the editor to do it
           would mean coming back to an empty page. -->
      <a
        class="bar-link"
        href="./reference.html"
        target="_blank"
        rel="noopener"
      >Language reference ↗</a>
      <!-- Beside the reference because it answers the other half of "what do I
           write": the reference says what the words mean, the guide says what
           to do with the Auto-level box below. -->
      <a
        class="bar-link"
        href="./leveling.html"
        target="_blank"
        rel="noopener"
      >Levelling guide ↗</a>
      <!-- Feedback needs somewhere durable to land. Without this the only route
           is email, and a report with no script attached is hard to act on. -->
      <a
        class="bar-link"
        href="https://github.com/bridge-craftwork/Dealer3/issues"
        target="_blank"
        rel="noopener noreferrer"
      >Feedback &amp; issues</a>
      <span v-if="engineVersion" class="bar-version">engine {{ engineVersion }}</span>
    </header>

    <main class="cols">
      <aside class="col col-picker">
        <ScenarioPicker :selected="selectedFile" :busy-file="loadingFile" @select="pickScenario" />
      </aside>

      <section class="col col-editor">
        <div class="controls">
          <label>Produce <input v-model.number="produce" class="narrow" type="number" min="1" /></label>
          <label>
            Max generate
            <!-- `min` must be a multiple of `step`, or the browser snaps to the
                 sequence min + n*step. With min=1 step=1000 the only valid
                 values were 1, 1001, 2001…, so 500000 stepped up to 500001 and
                 down to 499001. A zero is rejected at run time instead. -->
            <input v-model.number="maxGenerate" type="number" min="0" :step="generateStep" />
          </label>
          <!-- After the two limits, because on Random it is a field to read
               rather than set: it says which run this was, for quoting or
               coming back to. -->
          <label>
            Seed
            <input v-model.number="seed" type="number" min="0" max="4294967295" />
          </label>
          <!-- Rolled *before* the run and written into the field beside it, so
               the seed on screen is still the seed that produced what is shown.
               That was the reason Run never re-rolled on its own; doing it this
               way keeps the guarantee and saves the click — and saves needing a
               button to roll one by hand, since there is no other reason to. -->
          <label class="check" title="Roll a new seed each time you press Run, so every run is a fresh sample">
            <input v-model="newSeedEachRun" type="checkbox" />
            Random
          </label>
          <label>
            Format
            <select v-model="format">
              <option value="oneline">One line</option>
              <option value="printall">Print all</option>
              <option value="pbn">PBN</option>
            </select>
          </label>
        </div>

        <!-- Tabs, the levelling switch and Run share a row: three things that
             all decide what the pane below shows, and one row rather than two
             leaves that much more script on screen. -->
        <div class="run-row">
          <!-- Two views of the same run. The generated scenario is worth
               reading: the keeps, the header recording what they were measured
               over, and the chat text filled in from the same numbers. -->
          <div v-if="leveledScript" class="tabs" role="tablist">
            <button
              role="tab"
              :aria-selected="editorTab === 'script'"
              :class="{ on: editorTab === 'script' }"
              @click="editorTab = 'script'"
            >Script</button>
            <button
              role="tab"
              :aria-selected="editorTab === 'leveled'"
              :class="{ on: editorTab === 'leveled' }"
              @click="editorTab = 'leveled'"
            >Leveled</button>
          </div>
          <span v-else></span>

          <!-- Ticks itself when a script names hand types, since that is the
               only thing levelling needs and the reason to want it. Untouched
               after that: turning it back off is a choice, and re-ticking it on
               the next edit would take that away. Greyed while the levelled
               scenario is on screen, because that run has nothing left to
               decide — see `run`. -->
          <label class="check" :class="{ off: !levelBoxLive }" :title="levelHint">
            <input v-model="autoLevel" type="checkbox" :disabled="!levelBoxLive" />
            Auto-level
          </label>

          <button class="run" :disabled="!engineReady || running || !scriptValid" @click="run">
            {{ running ? 'Running…' : runLabel }}
          </button>
          <!-- Appears with the bars rather than the instant Run is pressed:
               a sub-second run would otherwise flash a button nobody could
               have used. -->
          <button v-if="showProgress" class="cancel" @click="cancel">Cancel</button>
        </div>

        <!-- Held back for a second, so the common short run does not flash a
             bar up and down. What it costs is that a run finishing at 1.1s
             shows one briefly — which is the right way round, since that run
             is long enough to wonder about. -->
        <div v-if="showProgress" class="progress" aria-live="polite">
          <div v-for="bar in progressBars" :key="bar.key" class="progress-row">
            <span class="progress-label">{{ bar.label }}</span>
            <span class="progress-track">
              <span
                class="progress-fill"
                :class="{ indeterminate: bar.fraction === null }"
                :style="bar.fraction === null ? null : { width: (100 * bar.fraction).toFixed(1) + '%' }"
              ></span>
            </span>
            <span class="progress-count">{{ bar.count }}</span>
          </div>
        </div>

        <ScriptEditor v-show="editorTab === 'script'" v-model="script" @validity="onValidity" />
        <ScriptViewer v-if="editorTab === 'leveled'" :script="leveledScript" />
      </section>

      <section class="col col-results">
        <ResultsPanel
          :result="result"
          :leveling="leveling"
          :error="error"
          :requested="produce"
          :downloading="downloading"
          @download="onDownload"
          @print="onPrint"
        />
      </section>
    </main>
  </div>

  <!-- A SIBLING of .app, not a child: the print stylesheet hides .app wholesale,
       which would take a nested print view down with it. -->
  <PrintView
    :script="script"
    :result="result"
    :scenario="selectedFile"
    :engine-ready="engineReady"
    :params="{ seed, produce, maxGenerate, format }"
  />
</template>

<script setup>
import { computed, ref, watch, onMounted, nextTick } from 'vue'
import ScenarioPicker from '@/components/ScenarioPicker.vue'
import ScriptEditor from '@/components/ScriptEditor.vue'
import ScriptViewer from '@/components/ScriptViewer.vue'
import ResultsPanel from '@/components/ResultsPanel.vue'
import PrintView from '@/components/PrintView.vue'
import { ready, generate, version } from '@/lib/engine.js'
import { fetchScenarioScript } from '@/lib/pbsScenarios.js'
import { downloadText, resultFilename, statisticsText } from '@/lib/download.js'
import { loadSession, saveSession } from '@/lib/session.js'
import { randomSeed } from '@/lib/format.js'

const STARTER = `# Write a dealer script, or pick a scenario on the left.
condition hcp(north) >= 15 && shape(north, any 4333 + any 4432 + any 5332)

action printoneline,
  average "N HCP" hcp(north),
  frequency "N HCP" (hcp(north), 15, 22)
`

// Pick up where the last visit left off. The starter script is only for a
// genuinely first visit — replacing someone's work with it would be worse than
// showing nothing.
const restored = loadSession()

const script = ref(restored?.script || STARTER)
// A random seed by default, matching the CLI, where `-s` defaults to the clock.
// Most of the time the question is "show me hands like this", not "show me
// these exact hands" — a fixed default quietly answers the second. A restored
// session keeps its seed, so reloading reproduces what was on screen.
const seed = ref(restored?.seed ?? randomSeed())
const produce = ref(restored?.produce ?? 20)
// Generation is the cheap half — ~1.3s for a million deals on a current
// machine — while a selective filter can easily need hundreds of thousands.
// 500,000 stopped short on real scripts (Lebensohl produced 17 of 20), and a
// truncated run is a worse outcome than a second of waiting.
const maxGenerate = ref(restored?.maxGenerate ?? 1000000)
const format = ref(restored?.format || 'oneline')

const engineReady = ref(false)
const engineVersion = ref('')
const running = ref(false)

// --- Progress -------------------------------------------------------------
//
// The engine reports from inside the worker; these hold the last report and
// decide whether it is worth showing yet.

/// The last report from each phase, keyed by phase name.
const phases = ref({})
/// Set a second into a run. Most runs finish first and never show a bar.
const showProgress = ref(false)
let progressTimer = null
let abort = null

/// A second's grace before any of it appears.
///
/// Most runs are well under that, and a bar that flashes up and down is worse
/// than none — it reads as a glitch rather than as information. A run that
/// crosses the second is one you have started to wonder about.
const PROGRESS_DELAY_MS = 1000

function startProgress() {
  phases.value = {}
  showProgress.value = false
  clearTimeout(progressTimer)
  progressTimer = setTimeout(() => {
    // Only if it is still going: the timer outlives a run that finished early.
    if (running.value) showProgress.value = true
  }, PROGRESS_DELAY_MS)
}

function stopProgress() {
  clearTimeout(progressTimer)
  progressTimer = null
  showProgress.value = false
  phases.value = {}
}

/// One bar per phase the run has reached, in the order they happen.
///
/// The measuring bar has no total until the probe has finished — how much
/// measuring a scenario needs depends on how rare its rarest hand type is, and
/// that is what the probe is for. Until then it runs indeterminate rather than
/// inventing a denominator.
const PHASE_LABELS = {
  probe: 'sampling',
  measuring: 'measuring',
  dealing: 'dealing',
}
const progressBars = computed(() =>
  ['probe', 'measuring', 'dealing']
    .filter((key) => phases.value[key])
    .map((key) => {
      const p = phases.value[key]
      const target = p.target > 0 ? p.target : 0
      return {
        key,
        label: PHASE_LABELS[key],
        fraction: target ? Math.min(1, p.produced / target) : null,
        count: target
          ? `${p.produced.toLocaleString()} / ${target.toLocaleString()}`
          : p.produced.toLocaleString(),
      }
    }),
)

/// Abandon the run in flight. The worker is terminated, so this stops work
/// already inside the wasm rather than merely ignoring its result.
function cancel() {
  abort?.abort()
}
const result = ref(null)
const error = ref('')
const scriptValid = ref(true)
const selectedFile = ref(restored?.scenario || '')
const loadingFile = ref('')
const downloading = ref(false)

// Levelling is off unless a script names hand types, which is the only thing it
// needs and the only reason to want it. `autoLevelTouched` records that someone
// has since had an opinion, so re-ticking the box on their next keystroke does
// not take it away from them.
const autoLevel = ref(restored?.autoLevel ?? false)
// Off by default: a run that changes its own seed cannot be repeated by
// pressing Run again, which is the first thing anyone tries.
const newSeedEachRun = ref(restored?.newSeedEachRun ?? false)
const autoLevelTouched = ref(restored?.autoLevel != null)
const editorTab = ref('script')

/// Whether the script declares any `HandType_*` variable.
///
/// Matched on the assignment rather than any mention, so a script that only
/// refers to one — a generated file, say — does not look like the source of it.
const hasHandTypes = computed(() =>
  /^[ \t]*HandType[A-Za-z0-9_]*[ \t]*=/m.test(script.value),
)

const levelHint = computed(() => {
  if (editorTab.value === 'leveled') {
    return 'The levelled scenario runs as it stands here — press Run for another sample of the same keeps.'
  }
  return hasHandTypes.value
    ? 'Measure how often each hand type comes up, then keep the common ones less often so the mix comes out even.'
    : 'Name some categories of hand with variables beginning HandType_ to level them.'
})

// Held rather than read off the last result, because running the levelled
// scenario on its own returns no levelling — that is the point of it. The
// report stays until the script changes or the box is unticked.
const leveling = ref(null)
const leveledScript = computed(() => leveling.value?.script || '')

/// Whether the box still has anything to decide.
///
/// On the Leveled tab it does not: that run takes the generated scenario as it
/// stands.
const levelBoxLive = computed(() => hasHandTypes.value && editorTab.value !== 'leveled')

const runLabel = computed(() => (editorTab.value === 'leveled' ? 'Run leveled' : 'Run'))

// Ticked for you the first time a script with hand types appears, and left
// alone afterwards.
watch(hasHandTypes, (has) => {
  if (has && !autoLevelTouched.value) autoLevel.value = true
  if (!has) autoLevel.value = false
}, { immediate: true })

watch(autoLevel, (on) => {
  autoLevelTouched.value = true
  if (!on) leveling.value = null
})

// The keeps belong to the script they were measured from; an edit makes them
// somebody else's numbers.
watch(script, () => {
  leveling.value = null
})

// A levelled run has a second tab; without one there is nothing to show there.
watch(leveledScript, (text) => {
  if (!text) editorTab.value = 'script'
})

onMounted(async () => {
  await ready()
  engineReady.value = true
  engineVersion.value = version()
})

// Persist the editor's contents and the parameters beside them. Debounced
// because this fires on every keystroke, and writing to localStorage is
// synchronous — it would otherwise sit on the typing path.
let saveTimer = null
watch(
  [script, seed, produce, maxGenerate, format, selectedFile, autoLevel, newSeedEachRun],
  () => {
    clearTimeout(saveTimer)
    saveTimer = setTimeout(() => {
      saveSession({
        script: script.value,
        seed: seed.value,
        produce: produce.value,
        maxGenerate: maxGenerate.value,
        format: format.value,
        scenario: selectedFile.value,
        autoLevel: autoLevel.value,
        newSeedEachRun: newSeedEachRun.value,
      })
    }, 400)
  },
  { deep: false },
)

// The arrows move the leading digit, not the last one. This field spans single
// digits to millions, and a fixed step is wrong at one end or the other: 1000
// is a fifth of a percent at 500,000, and larger than the whole value at 100.
const generateStep = computed(() => {
  const v = Math.floor(Math.abs(maxGenerate.value || 0))
  if (v < 10) return 1
  const magnitude = 10 ** (String(v).length - 1)
  // Exactly on a power of ten, a full-magnitude step down lands on zero. Drop a
  // decade so 100,000 goes to 90,000 rather than nothing.
  return v === magnitude ? magnitude / 10 : magnitude
})

function onValidity({ ok, empty }) {
  scriptValid.value = ok && !empty
}

async function pickScenario(item) {
  loadingFile.value = item.file
  error.value = ''
  try {
    script.value = await fetchScenarioScript(item.file)
    selectedFile.value = item.file
    result.value = null
    // Let the editor take the new buffer and re-validate before running.
    await nextTick()
  } catch (e) {
    error.value = e.message || String(e)
  } finally {
    loadingFile.value = ''
  }
}

// Saving re-runs rather than reformatting what is on screen: the displayed
// deals are capped, and PBN needs the engine's formatter anyway. The stats are
// identical either way, since generation is deterministic for a given seed.
async function onDownload(kind) {
  downloading.value = true
  try {
    const name = selectedFile.value || 'dealer3'
    if (kind === 'pbn') {
      const pbn = await generate(script.value, {
        seed: seed.value,
        produce: produce.value,
        maxGenerate: maxGenerate.value,
        format: 'pbn',
      })
      downloadText(
        resultFilename(name, seed.value, 'pbn'),
        pbn.deals.join('\n') + '\n',
        'application/x-pbn',
      )
    } else {
      const text = await generate(script.value, {
        seed: seed.value,
        produce: produce.value,
        maxGenerate: maxGenerate.value,
        format: format.value === 'pbn' ? 'oneline' : format.value,
      })
      // `printes` first, as it appears on screen: leaving it out would drop
      // what the script printed from the file the user saves.
      const printed = text.printes ? text.printes + '\n' : ''
      const body = printed + text.deals.join('\n') + '\n' + statisticsText(text)
      downloadText(resultFilename(name, seed.value, 'txt'), body)
    }
  } catch (e) {
    error.value = e?.message || String(e)
  } finally {
    downloading.value = false
  }
}

// The browser's print dialog, from which the user picks "Save as PDF". No PDF
// library: this keeps the text selectable and the footer link live, and adds
// nothing to the bundle.
function onPrint() {
  window.print()
}

async function run() {
  // Before the checks below, so the new seed is the one they validate and the
  // one the field shows beside the result.
  if (newSeedEachRun.value) seed.value = randomSeed()

  // The fields are free text, so they can hold 0, a negative, or nothing at all
  // if someone clears one. Catch that here rather than asking the engine to
  // generate zero deals and reporting an empty result as if it meant something.
  const limits = [
    ['Produce', produce.value],
    ['Max generate', maxGenerate.value],
  ]
  for (const [name, value] of limits) {
    if (!Number.isFinite(value) || value < 1) {
      error.value = `${name} must be at least 1.`
      result.value = null
      return
    }
  }
  if (!Number.isFinite(seed.value) || seed.value < 0) {
    error.value = 'Seed must be a whole number of 0 or more.'
    result.value = null
    return
  }

  running.value = true
  error.value = ''
  abort = new AbortController()
  startProgress()
  try {
    // Generation runs in a worker, so the tab stays responsive: the button
    // paints its disabled state at once, a second click cannot queue up behind
    // a frozen thread, and the engine can report how far along it is.
    // On the Leveled tab, run the generated scenario as it stands: no
    // measuring pass, no new keeps, the same script every time. So pressing Run
    // again is another sample of one levelling rather than a fresh levelling —
    // which is what you want when comparing runs, and what makes the script in
    // the pane worth reading rather than something that moves under you.
    const onLeveled = editorTab.value === 'leveled' && leveledScript.value
    result.value = await generate(onLeveled ? leveledScript.value : script.value, {
      seed: seed.value,
      produce: produce.value,
      maxGenerate: maxGenerate.value,
      format: format.value,
      autoLevel: !onLeveled && autoLevel.value && hasHandTypes.value,
      signal: abort.signal,
      onProgress: (report) => {
        phases.value = { ...phases.value, [report.phase]: report }
      },
    })
    if (result.value.leveling) leveling.value = result.value.leveling
  } catch (e) {
    result.value = null
    // Cancelling is a choice, not a fault: say what happened and leave the
    // previous result's absence unexplained by an error box.
    error.value = e?.cancelled ? '' : e?.message || String(e)
  } finally {
    stopProgress()
    abort = null
    running.value = false
  }
}
</script>

<style>
:root {
  --bg: #ffffff;
  --bg-subtle: #f4f5f7;
  --fg: #1b1d20;
  --fg-muted: #6b7280;
  --line: #d8dade;
  --accent: #2f6fb2;
  /* The share nature offers, against the accent's levelled share. Warm against
     cool, and far enough from both to be told apart by anyone who cannot. */
  --natural: #d98324;
  --accent-subtle: #e4eefa;
  --danger: #b3261e;
  --warn: #b8860b;
  --warn-fg: #8a6300;
  --warn-subtle: #fdf6e3;
  --mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  /* The editor is dark against the light page; these keep its chrome — border,
     status strip — matched to it rather than to the surrounding UI. */
  --editor-bg: #282c34;
  --editor-line: #3a4049;
}

* { box-sizing: border-box; }
html, body, #app { height: 100%; margin: 0; }
body {
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  color: var(--fg);
  background: var(--bg);
}
</style>

<style scoped>
.app { display: flex; flex-direction: column; height: 100%; }

.bar {
  display: flex; align-items: baseline; gap: 10px;
  padding: 8px 14px; border-bottom: 1px solid var(--line); background: var(--bg-subtle);
}
.bar h1 { font-size: 15px; margin: 0; }
.bar-sub { font-size: 12px; color: var(--fg-muted); }
.bar-spacer { flex: 1; }
.bar-link { font-size: 12px; color: var(--accent); text-decoration: none; }
.bar-link:hover { text-decoration: underline; }
.bar-version { font-size: 11px; color: var(--fg-muted); font-family: var(--mono); }

.cols { display: grid; grid-template-columns: 260px 1fr 1fr; flex: 1; min-height: 0; }
.col { min-width: 0; min-height: 0; }
.col-picker { border-right: 1px solid var(--line); }
.col-editor { display: flex; flex-direction: column; padding: 8px; gap: 8px; min-height: 0; }
.col-results { border-left: 1px solid var(--line); min-height: 0; }

.controls { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; font-size: 12px; }
.controls label { display: flex; align-items: center; gap: 4px; color: var(--fg-muted); }
.controls input, .controls select {
  font: inherit; font-size: 12px; padding: 3px 5px;
  border: 1px solid var(--line); border-radius: 3px; background: var(--bg); color: var(--fg);
}
/* Wide enough for a ten-digit seed, which is the longest thing any of them
   holds. Produce is a board count and never needs half of that. */
.controls input[type="number"] { width: 7em; }
/* Two digits narrower than the seed's seven ems, which is sized for a
   ten-figure seed. A board count is two or three digits nearly always, and the
   spinner takes a good part of a small box. */
.controls input.narrow { width: 4.5em; }
.check {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 0.82rem;
  white-space: nowrap;
}
.check.off { color: var(--fg-muted); }
.check input { margin: 0; }

/* Tabs left, the levelling switch centred, Run right. A grid rather than
   space-between so the middle is actually centred: with three flex items of
   unequal width it drifts, and it drifts differently depending on whether the
   tabs are there at all. */
.run-row {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: end;
  gap: 10px;
}
.run-row > .check { justify-self: center; }
.run-row > .run { justify-self: end; }

.tabs { display: flex; gap: 2px; margin-bottom: -1px; }
.tabs button {
  font: inherit;
  font-size: 0.8rem;
  padding: 4px 12px;
  border: 1px solid var(--line);
  border-bottom: none;
  border-radius: 4px 4px 0 0;
  background: var(--bg-subtle);
  color: var(--fg-muted);
  cursor: pointer;
}
.tabs button.on { background: var(--editor-bg); color: #fff; border-color: var(--editor-line); }

.editor-loading {
  flex: 1; display: grid; place-items: center;
  color: var(--fg-muted); font-size: 13px;
  border: 1px solid var(--line); border-radius: 4px;
}
.run {
  padding: 5px 16px; font: inherit; font-size: 13px; font-weight: 500;
  border: 0; border-radius: 4px; background: var(--accent); color: #fff; cursor: pointer;
}
.run:disabled { background: var(--line); color: var(--fg-muted); cursor: default; }

/* Quieter than Run: it is the way out, not the way on. */
.cancel {
  margin-left: 6px;
  padding: 5px 12px; font: inherit; font-size: 13px;
  border: 1px solid var(--line); border-radius: 4px;
  background: #fff; color: var(--fg-muted); cursor: pointer;
}
.cancel:hover { color: #b23b3b; border-color: #d8a9a9; }

/* Between the run row and the editor, so it sits where the wait is felt
   without pushing the script down permanently — it exists only while running. */
.progress {
  display: flex; flex-direction: column; gap: 3px;
  margin: 6px 0 2px;
}
.progress-row {
  display: grid;
  grid-template-columns: 5.5rem 1fr auto;
  align-items: center; gap: 8px;
  font-size: 11px; color: var(--fg-muted); font-family: var(--mono);
}
.progress-track {
  height: 5px; border-radius: 3px; background: var(--line); overflow: hidden;
}
.progress-fill {
  display: block; height: 100%; border-radius: 3px;
  background: var(--accent);
  transition: width 0.12s linear;
}
/* No total yet — the probe decides how much measuring this scenario needs, so
   until it finishes there is no honest denominator to draw against. */
.progress-fill.indeterminate {
  width: 35%;
  animation: progress-sweep 1.1s ease-in-out infinite;
}
@keyframes progress-sweep {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(300%); }
}
.progress-count { font-variant-numeric: tabular-nums; }

@media (prefers-reduced-motion: reduce) {
  .progress-fill.indeterminate { animation: none; width: 100%; opacity: 0.4; }
}

@media (max-width: 1000px) {
  .cols { grid-template-columns: 1fr; grid-template-rows: auto 1fr 1fr; }
  .col-picker { border-right: 0; border-bottom: 1px solid var(--line); max-height: 220px; }
  .col-results { border-left: 0; border-top: 1px solid var(--line); }
}
</style>
