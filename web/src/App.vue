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
      <!-- The pool the engine brought up, which is what every run deals on.
           Shown rather than logged: a page that fell back to one thread looks
           exactly like a slow scenario, and that is how the first threaded
           build shipped serial without anyone noticing. `one thread` is
           marked, because on this site it means the headers are not arriving
           rather than that the browser cannot. -->
      <span
        v-if="threads"
        class="bar-version bar-threads"
        :class="{ lonely: threads.threads < 2 }"
        :title="threadsHint"
      >{{ threads.threads }} {{ threads.threads === 1 ? 'thread' : 'threads' }}</span>
    </header>

    <main class="cols" :class="{ 'picker-closed': !pickerOpen }">
      <!-- The scenario list is how you find a starting point, and then it is
           260px of nothing while you edit. Closing it hands that width to the
           editor and the results, which share the rest of the row. -->
      <aside class="col col-picker" :class="{ 'is-closed': !pickerOpen }">
        <ScenarioPicker
          v-if="pickerOpen"
          :selected="selectedFile"
          :busy-file="loadingFile"
          @select="pickScenario"
          @close="pickerOpen = false"
        />
        <!-- What is left when it is closed: a labelled rail, not a bare edge.
             An icon alone would leave the list findable only by whoever hid
             it, and this is the only way back. -->
        <button
          v-else
          class="picker-open"
          title="Show the scenario list"
          aria-expanded="false"
          @click="pickerOpen = true"
        >
          <span aria-hidden="true">›</span>
          <span class="picker-open-label">Scenarios</span>
        </button>
      </aside>

      <section class="col col-editor">
        <!-- What a link did, said where the script it replaced is. A link that
             quietly swapped somebody's work for a stranger's would be the one
             unforgivable thing this feature could do, so the way back is
             offered here rather than left to the back button — which would not
             help, the page having never navigated. -->
        <p v-if="shareError" class="shared-notice bad">{{ shareError }}</p>
        <p v-else-if="sharedNotice" class="shared-notice">
          <span>{{ sharedNotice }}</span>
          <button
            v-if="canRestore"
            class="shared-restore"
            title="Put back the script and settings that were here before this link was opened"
            @click="restorePrevious"
          >Bring back what I had</button>
          <button class="shared-dismiss" title="Hide this" @click="sharedNotice = ''">Dismiss</button>
        </p>

        <!-- Said where it is chosen rather than in the results, because it is
             a reason to choose differently before pressing Run: a script with
             no double-dummy question in it downloads 640 KiB and reads none of
             the answers. Asked of the engine, off the parsed script. -->
        <p v-if="dealSource === 'library' && pointlessLibrary" class="source-warn">
          {{ pointlessLibrary }}
        </p>
        <!-- The same question read the other way, and the expensive direction:
             a script that does ask for double-dummy work, told to shuffle its
             own deals, solves every one of them here. -->
        <p v-else-if="dealSource === 'random' && slowRandom" class="source-warn">
          {{ slowRandom }}
        </p>

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
          <!-- The number touched most while writing a script — "give me five of
               these and let me look" — so it is the one that earns a permanent
               place beside Run. -->
          <label>Produce <input v-model.number="produce" class="num-produce" type="number" min="1" /></label>

          <!-- Beside the gear rather than in it: sharing is something you go
               looking for, and a control behind a disclosure is one nobody
               finds. It shares what is in the editor and the settings around
               it — on the Leveled tab that is still the script that produced
               the levelling, which is the thing worth sending. -->
          <button
            class="share"
            :disabled="!script.trim()"
            title="Copy a link to this script and its settings. The link holds them itself — nothing is uploaded."
            @click="onShare"
          >Share</button>

          <button
            class="settings-toggle"
            :class="{ on: settingsOpen }"
            :title="settingsOpen ? 'Hide settings' : 'Deal source, seed, format and limits'"
            aria-label="Settings"
            aria-controls="settings-panel"
            :aria-expanded="settingsOpen ? 'true' : 'false'"
            @click="settingsOpen = !settingsOpen"
          >
            <!-- U+FE0E, the text variation selector. U+2699 has no default
                 presentation, so a platform chooses: macOS draws the glyph,
                 iOS draws a colour emoji, and the button showed a shaded 3D
                 cog on a phone and a flat one on a desktop. This asks for the
                 text form, which also keeps it in `currentColor` with the
                 rest of the button. -->
            <span aria-hidden="true">⚙︎</span>
          </button>

          <button
            class="run"
            :disabled="!engineReady || running || !scriptValid || paramsMissing.length > 0"
            :title="runHint"
            @click="run"
          >
            {{ running ? 'Running…' : runLabel }}
          </button>
          <!-- Appears with the bars rather than the instant Run is pressed:
               a sub-second run would otherwise flash a button nobody could
               have used. -->
          <button v-if="showProgress" class="cancel" @click="cancel">Cancel</button>
        </div>

        <!-- The link itself, and not only the clipboard: a copy can be refused
             outright by the browser, and a link nobody can see would then be a
             button that appeared to work. Selectable, so it can be taken by
             hand. -->
        <div v-if="shareLink" class="share-panel">
          <input
            ref="shareField"
            class="share-input"
            type="text"
            readonly
            aria-label="Link to this script"
            :value="shareLink"
            @focus="$event.target.select()"
          />
          <button class="shared-dismiss" @click="shareLink = ''">Done</button>
          <p class="share-note settings-note">{{ shareNote }}</p>
        </div>

        <!-- Everything that is set once and then left alone. It used to be a
             row of its own, always on screen, and it is the reason the page did
             not fit a phone: the number fields are sized in `em` for the digits
             they hold, so the row had a floor it could not shrink below. See
             #95.

             What stays out here is what someone touches while writing a script:
             Produce, and Run. Everything else — including Auto-level, which most
             people never reach for — is in here.

             Below the Run row, not above it: a disclosure opens downward from
             the control that owns it, and one that pushed the row it belongs to
             further down the page read as a separate thing that had appeared. -->
        <div v-if="settingsOpen" id="settings-panel" class="settings-panel">
          <!-- Where the deals come from. The one setting here that is about the
               deals rather than the run: everything beside it means exactly what
               it meant, the seed included — on Pre-solved it picks a starting
               position in the library instead of driving a shuffle, so the same
               seed still gives the same deals. -->
          <span class="source" role="radiogroup" aria-label="Deal source">
            <label class="check" title="Shuffle deals here, from the seed. What this page has always done.">
              <input v-model="dealSource" type="radio" name="deal-source" value="random" />
              Random deals
            </label>
            <label class="check" :title="libraryHint">
              <input v-model="dealSource" type="radio" name="deal-source" value="library" />
              Pre-solved deals
            </label>
          </span>
          <label>
            Max generate
            <!-- `min` must be a multiple of `step`, or the browser snaps to the
                 sequence min + n*step. With min=1 step=1000 the only valid
                 values were 1, 1001, 2001…, so 500000 stepped up to 500001 and
                 down to 499001. A zero is rejected at run time instead. -->
            <input
              v-model.number="maxGenerate"
              class="num-generate"
              type="number"
              min="0"
              :step="generateStep"
            />
          </label>
          <!-- After the two limits, because on Random it is a field to read
               rather than set: it says which run this was, for quoting or
               coming back to. -->
          <label>
            Seed
            <input v-model.number="seed" class="num-seed" type="number" min="0" max="4294967295" />
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
          <!-- `None` is not a way of hiding the deals: the engine stops
               collecting them, so a statistics run does not build, ship and
               lay out hands nobody is going to look at. -->
          <label :title="formatHint">
            Format
            <select v-model="format">
              <option value="oneline">One line</option>
              <option value="printall">Print all</option>
              <option value="pbn">PBN</option>
              <option value="none">None — statistics only</option>
            </select>
          </label>
          <!-- Only beside PBN, because that is the only format with anywhere to
               put a table: one line for the tag, or twenty-two for the section
               PBN 2.1 specifies. Filled from what a deal already knows and
               never solved for, so this changes what the file holds and never
               how long the run takes — which is why it is a plain checkbox and
               not a warning about cost. -->
          <label v-if="format === 'pbn'" class="check" :title="ddTagsHint">
            <input v-model="ddTables" type="checkbox" />
            Double-dummy tables
          </label>

          <!-- Divides Produce among the hand types instead of taking deals as
               they come. It answers the same question as Auto-level below —
               what mix comes out — and only one of them can, which is why the
               two sit together. -->
          <label class="check" :class="{ off: !roundRobinLive }" :title="roundRobinHint">
            <input v-model="roundRobin" type="checkbox" :disabled="!roundRobinLive" />
            Round robin
          </label>
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

          <!-- Levelling's own limit, and only shown while levelling is on:
               characterizing is a pass nobody asked for, and how long to spend
               on it is the one thing worth saying about it. Seconds rather than
               deals, because how many deals a sighting costs is exactly what
               the pass is there to find out — and because Max generate, which
               used to stop this pass as well, is about the run instead. -->
          <label v-if="autoLevel && levelBoxLive" :title="measureHint">
            Characterize
            <input
              v-model.number="measureSeconds"
              class="num-measure"
              type="number"
              min="1"
              max="300"
              step="5"
            />s
          </label>

          <!-- Beside Run rather than at the top of the pane: this is where the
               eye already is when a run is being set up. `aria-expanded` and
               `aria-controls` because the panel is elsewhere in the document
               and a screen reader has no other way to learn the two are
               related. -->

          <!-- The neutral half of what this page says about the library. The
               warnings stay outside: they exist to change a decision before Run
               is pressed, and one behind a closed panel would not. -->
          <p v-if="dealSource === 'library'" class="source-note settings-note">
            {{ libraryHint }}
          </p>
        </div>


        <!-- Not held back the way the bars below are. Fetching is the one part
             of a run that waits on something outside the tab, and a page that
             sits still for a second with nothing said reads as broken. -->
        <p v-if="libraryStatus" class="library-status" aria-live="polite">{{ libraryStatus }}</p>

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
              <!-- Where this pass is expected to end, when that is short of the
                   goal. Without it a bar that stops at 3% looks broken, when in
                   fact it is doing all it can. -->
              <span
                v-if="bar.expected !== null"
                class="progress-mark"
                :style="{ left: (100 * bar.expected).toFixed(1) + '%' }"
                :title="bar.expectedHint"
              ></span>
            </span>
            <span class="progress-count" :title="bar.expectedHint || undefined">{{ bar.count }}</span>
          </div>
        </div>

        <!-- Above the editor rather than among the run controls: these belong
             to the script, not to the run — a different script wants different
             ones, and the same script wants the same ones every time. -->
        <ScriptParams
          v-show="editorTab === 'script'"
          v-model="paramValues"
          :script="script"
          :engine-ready="engineReady"
          @change="onParams"
        />
        <ScriptEditor
          v-show="editorTab === 'script'"
          v-model="script"
          :params="paramSpecs"
          @validity="onValidity"
        />
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
    :params="{ seed, produce, maxGenerate, format, dealSource }"
  />
</template>

<script setup>
import { computed, ref, watch, onMounted, nextTick } from 'vue'
import ScenarioPicker from '@/components/ScenarioPicker.vue'
import ScriptEditor from '@/components/ScriptEditor.vue'
import ScriptParams from '@/components/ScriptParams.vue'
import ScriptViewer from '@/components/ScriptViewer.vue'
import ResultsPanel from '@/components/ResultsPanel.vue'
import PrintView from '@/components/PrintView.vue'
import {
  ready,
  generate,
  usesDoubleDummy,
  version,
  defaultMeasureSeconds,
  poolInfo,
} from '@/lib/engine.js'
import { libraryStatusText, pointlessLibraryWarning, slowRandomWarning } from '@/lib/library.js'
import { fetchScenarioScript, prettifyLabel } from '@/lib/pbsScenarios.js'
import { downloadText, resultFilename, statisticsText } from '@/lib/download.js'
import { loadSession, saveSession } from '@/lib/session.js'
import { randomSeed } from '@/lib/format.js'
import { makeDocument, paramValuesFrom } from '@/lib/envelope.js'
import { parseFragment, resolveFragment, shareFragment, shareUrl } from '@/lib/share.js'
import { copyText } from '@/lib/clipboard.js'

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
// Divide Produce among the script's hand types — one of each per round, any
// remainder going to whichever types turn up next — rather than taking deals as
// they come.
//
// Exact where levelling is exact on average, which for a set generated once and
// handed to a class is the difference that matters: twenty boards drawn from a
// perfectly level distribution are four of each *on average* and 6/1/5/4/4
// without anything having gone wrong. Nothing is measured, so nothing can be
// measured wrong.
const roundRobin = ref(restored?.roundRobin ?? false)
// Generation is the cheap half — ~1.3s for a million deals on a current
// machine — while a selective filter can easily need hundreds of thousands.
// 500,000 stopped short on real scripts (Lebensohl produced 17 of 20), and a
// truncated run is a worse outcome than a second of waiting.
const maxGenerate = ref(restored?.maxGenerate ?? 1000000)
const format = ref(restored?.format || 'oneline')

// Whether a PBN export carries the double-dummy tables its deals arrived with.
//
// A boolean here and one of the engine's four words on the wire: the page
// offers the standard `[OptimumResultTable]` or nothing at all, where the
// command line can also ask for the Bridge Composer extension. Two controls for
// an encoding almost nobody chooses between would be two controls to explain.
//
// On by default, matching `--dd-tags`: a deal drawn from the pre-solved library
// knows all twenty cells, and dropping them on the way out would throw away the
// whole reason for reading from it.
const ddTables = ref((restored?.ddTags ?? 'optimum') !== 'none')
const ddTags = computed(() => (ddTables.value ? 'optimum' : 'none'))

const ddTagsHint = computed(() =>
  ddTables.value
    ? 'Each deal that already knows its double-dummy table writes it into the file, as the ' +
      '[OptimumResultTable] section PBN 2.1 specifies. Deals from the pre-solved library all ' +
      'know theirs; a shuffled deal only knows what the script asked about. Nothing is solved ' +
      'to fill one in.'
    : 'No analysis tags, even for deals that know their table. Worth it for a large set nobody ' +
      'will read the analysis in: the table is twenty-two lines a board, where a whole board is ' +
      'a few hundred bytes.',
)

// Where the deals come from: 'random' shuffles them here, as this page always
// has; 'library' draws them from the published solved-deal library, where every
// deal arrives with its double-dummy table and the seed picks a starting
// position rather than driving a shuffle.
//
// Random by default. Pre-solved costs a download and pays for it only when a
// script asks a double-dummy question, which almost none do.
const dealSource = ref(restored?.dealSource || 'random')

/// The one line shown while pieces of the library are being fetched, and empty
/// the rest of the time.
const libraryStatus = ref('')

// What has been typed into the parameter fields, by parameter number. Empty
// means "use whatever the script declares", so clearing a field returns to the
// default rather than blanking the parameter.
const paramValues = ref(restored?.paramValues || {})
/// The same thing in `--param`'s spelling, ready for the engine.
const paramSpecs = ref([])
/// Parameters the script uses that nothing supplies. The run would fail on the
/// first of them, so Run is held rather than offering it.
const paramsMissing = ref([])

function onParams({ specs, missing }) {
  paramSpecs.value = specs
  paramsMissing.value = missing
}

const engineReady = ref(false)
const engineVersion = ref('')
/// `{ threads, why, supported }` from the worker, once it has brought the pool
/// up. Null until then.
const threads = ref(null)

const threadsHint = computed(() => {
  const info = threads.value
  if (!info) return ''
  if (info.threads > 1) {
    return (
      `Every run deals on all ${info.threads} threads. On a machine this size that is about ` +
      'four times faster on a bare filter and six on a real scenario. The deals are not ' +
      'affected: a thread count changes how long a run takes and nothing else.'
    )
  }
  if (!info.supported) {
    return 'This engine was built without a thread pool, so it deals on one thread.'
  }
  return (
    `No thread pool: ${info.why}. Every run deals on one thread, which on this machine is ` +
    'about four to six times slower than it needs to be. The deals themselves are the same.'
  )
})
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
/// The characterizing bar counts sightings of the scarcest category against the
/// number needed to divide by, not deals: a run that has dealt a million is no
/// further along than its scarcest category says it is, and that is what the
/// keeps are computed from.
const PHASE_LABELS = {
  characterizing: 'characterizing',
  dealing: 'dealing',
  'additional dealing': 'additional dealing',
}
const progressBars = computed(() =>
  ['characterizing', 'dealing', 'additional dealing']
    .filter((key) => phases.value[key])
    .map((key) => {
      const p = phases.value[key]
      const target = p.target > 0 ? p.target : 0
      // Only worth drawing when it says something the bar does not: a pass on
      // course for its goal has its mark at the far end, which is just the end.
      const short = target && p.expected > 0 && p.expected < target * 0.98
      return {
        key,
        label: PHASE_LABELS[key],
        fraction: target ? Math.min(1, p.produced / target) : null,
        expected: short ? Math.min(1, p.expected / target) : null,
        expectedHint: short
          ? `On course for about ${p.expected.toLocaleString()} of ${target.toLocaleString()} — ` +
            'the deal limit or the time budget will stop it first. ' +
            'The levelling still works; its rarest rate is just less well known.'
          : '',
        // Deals looked at, beside deals kept. The bar counts what came out,
        // and for most scripts that climbs steadily enough to be the whole
        // story. For one that solves a double-dummy table per deal it does
        // not: a batch is dealt and tested before any of it is handed over, so
        // the bar can sit at zero for a long time while the run is working
        // hard. This is the number that moves meanwhile, and it is what `-m`
        // prints in the terminal for the same reason.
        count:
          (target
            ? `${p.produced.toLocaleString()} / ${target.toLocaleString()}`
            : p.produced.toLocaleString()) +
          (p.generated > 0 ? ` · ${p.generated.toLocaleString()} dealt` : ''),
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
// Whether the scenario list is showing. Open on a first visit — it is how you
// find something to run — and remembered from then on, because someone who has
// closed it is editing a script and would have to close it again on every
// reload otherwise.
const pickerOpen = ref(restored?.pickerOpen ?? true)

// Closed by default: the panel exists because these were on screen all the
// time and did not need to be. Remembered, though — someone who opens it is
// usually setting up a session, not answering one question.
const settingsOpen = ref(restored?.settingsOpen ?? false)
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
// Seconds to spend characterizing a scenario before levelling it. `null` until
// the engine is loaded and says what its own default is — kept in one place
// rather than written down twice, since a field disagreeing with the engine it
// drives is worse than no field.
const measureSeconds = ref(restored?.measureSeconds ?? null)
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
    ? 'Measure how often each hand type comes up, then keep the common ones less often so the mix comes out even, and write the levelled scenario.'
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

// The same conditions, for the same reasons: it needs categories to deal round,
// and the Leveled tab runs its scenario as it stands.
//
// Independent of Auto-level, not an alternative to it. They are two capabilities
// resting on the same `HandType_` declarations: one measures the scenario and
// writes the levelled copy, the other decides which of its deals reach the
// file. Together you get a script to publish and an exact set to hand out.
const roundRobinLive = computed(() => hasHandTypes.value && editorTab.value !== 'leveled')

/// What to send the engine. On the Leveled tab the generated scenario runs as
/// it stands, and dealing that round would be both modes at once.
const roundRobinAsked = computed(
  () => roundRobin.value && hasHandTypes.value && editorTab.value !== 'leveled',
)

const roundRobinHint = computed(() => {
  if (!hasHandTypes.value) {
    return 'Name some categories of hand with variables beginning HandType_ to deal them round robin.'
  }
  return 'Divide Produce among the hand types: one of each per round, and a partial round at the end if it does not divide. Works with Auto-level or without it.'
})

// It deals the hand types, so a script naming none has nothing to deal round.
// Enforced by the engine as well — this only keeps the page from offering a run
// it knows will be refused.
watch(hasHandTypes, (has) => {
  if (!has) roundRobin.value = false
})


const runLabel = computed(() => (editorTab.value === 'leveled' ? 'Run leveled' : 'Run'))

// A disabled button with no explanation is the worst of both: says the run
// cannot happen and not what would let it.
const runHint = computed(() => {
  if (!paramsMissing.value.length) return ''
  const names = paramsMissing.value.map((i) => `$${i}`).join(', ')
  return `Fill in ${names} above — the script uses ${
    paramsMissing.value.length > 1 ? 'them' : 'it'
  } and declares no default.`
})

const measureHint =
  'How long to spend measuring how often each hand type comes up, before dealing the levelled ' +
  'scenario. Longer pins the keeps down better; the panel says what the measurement was worth. ' +
  'Max generate does not bound this — it is the budget for the run you asked for.'

const formatHint = computed(() =>
  format.value === 'none'
    ? 'Statistics only: the engine collects no deals, so nothing is rendered, shipped or laid out. Averages, frequencies and the counts are all still measured over every deal produced.'
    : 'How each produced deal is written out. None keeps the statistics and collects no deals at all, which is what a run gathering numbers wants.',
)

// Said once. This was the tooltip while a near-identical sentence sat on the
// page — two wordings of one fact, which had already drifted apart once — and
// it is now both the tooltip and the note inside the settings panel.
//
// The sentence about a piece being 640 KiB and kept has gone. It explained a
// first-run wait that was two and a half seconds when it was written and is
// now under a second, and implementation detail that no longer explains
// anything a user notices is just something else to read.
const libraryHint =
  'Deals come from Pavlicek\u2019s 10,485,760 solved deals, so tricks(), dds() and par() are ' +
  'lookups rather than searches. The seed picks where in the library to start.'

/// Whether this script asks a double-dummy question at all, as the engine reads
/// it off the parsed program: true, false, or undefined while the engine is
/// still loading or the script does not parse.
///
/// Not a search for the words. `t = tricks(north, notrump)` mentions one and
/// `x = t` does not, and a comment mentioning `par` is not a call — which is
/// why this is asked of the engine rather than answered with a regular
/// expression here.
const scriptUsesDoubleDummy = computed(() => {
  if (!engineReady.value) return undefined
  try {
    return usesDoubleDummy(script.value, paramSpecs.value)
  } catch {
    // A script the engine cannot even look at is not a script asking for
    // nothing; say nothing rather than the wrong thing.
    return undefined
  }
})

/// Why pre-solved deals are the wrong choice for this script, if they are.
const pointlessLibrary = computed(() => pointlessLibraryWarning(scriptUsesDoubleDummy.value))

/// And the other way: why random deals are the expensive choice for a script
/// that does ask a double-dummy question. The same answer read the other way,
/// and the costlier of the two mistakes — the first wastes a download, this one
/// turns a run that would have taken a moment into one that takes minutes.
const slowRandom = computed(() => slowRandomWarning(scriptUsesDoubleDummy.value, produce.value))

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
  // First, and not awaited with the rest: a shared script should be on screen
  // while the engine is still loading, exactly as a restored one is.
  openSharedLink()
  // A link pasted into the address bar of a tab that is already here changes
  // the fragment without reloading anything, and without this the page would
  // sit there having visibly done nothing.
  window.addEventListener('hashchange', openSharedLink)
  await ready()
  engineReady.value = true
  engineVersion.value = version()
  // Only when nothing was restored and nothing typed: whoever set it meant it.
  if (measureSeconds.value == null) measureSeconds.value = defaultMeasureSeconds()
  // Brings the generating worker up as a side effect, so the first Run does not
  // also pay for loading the engine a second time.
  threads.value = await poolInfo()
})

// Persist the editor's contents and the parameters beside them. Debounced
// because this fires on every keystroke, and writing to localStorage is
// synchronous — it would otherwise sit on the typing path.
let saveTimer = null
watch(
  [
    script,
    seed,
    produce,
    roundRobin,
    maxGenerate,
    format,
    ddTables,
    dealSource,
    selectedFile,
    pickerOpen,
    settingsOpen,
    autoLevel,
    measureSeconds,
    newSeedEachRun,
    paramValues,
  ],
  () => {
    clearTimeout(saveTimer)
    saveTimer = setTimeout(() => {
      saveSession({
        script: script.value,
        seed: seed.value,
        produce: produce.value,
        roundRobin: roundRobin.value,
        maxGenerate: maxGenerate.value,
        format: format.value,
        ddTags: ddTags.value,
        dealSource: dealSource.value,
        scenario: selectedFile.value,
        pickerOpen: pickerOpen.value,
        settingsOpen: settingsOpen.value,
        autoLevel: autoLevel.value,
        measureSeconds: measureSeconds.value,
        newSeedEachRun: newSeedEachRun.value,
        paramValues: paramValues.value,
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
    const text = await fetchScenarioScript(item.file)
    script.value = text
    // What the list served, kept so Share can tell an untouched scenario from
    // an edited one. An untouched one travels as its name; one changed by a
    // character cannot, or the recipient would open a different script under
    // the right name.
    pristine.value = { file: item.file, text }
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
        roundRobin: roundRobinAsked.value,
        maxGenerate: maxGenerate.value,
        format: 'pbn',
        ddTags: ddTags.value,
        params: paramSpecs.value,
        // Saving re-runs the script, so it has to read from the same place the
        // run on screen did — otherwise the file would hold different deals
        // from the ones it was saved from.
        source: dealSource.value,
      })
      downloadText(
        resultFilename(name, seed.value, 'pbn'),
        pbn.deals.join('\n') + '\n',
        'application/x-pbn',
      )
    } else if (format.value === 'none') {
      // Nothing was collected, so there is nothing to re-run FOR: the
      // statistics on screen are the whole of what a text file would hold.
      // Re-running would deal the hundred thousand again to arrive at numbers
      // already in hand, which is the expensive half of what None was picked to
      // avoid. Written from the result instead.
      const shown = result.value
      if (!shown) return
      const printed = shown.printes ? shown.printes + '\n' : ''
      downloadText(resultFilename(name, seed.value, 'txt'), printed + statisticsText(shown))
    } else {
      const text = await generate(script.value, {
        seed: seed.value,
        produce: produce.value,
        roundRobin: roundRobinAsked.value,
        maxGenerate: maxGenerate.value,
        format: format.value === 'pbn' ? 'oneline' : format.value,
        params: paramSpecs.value,
        source: dealSource.value,
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

// --- Sharing, and opening what somebody shared ---------------------------
//
// A link carries the script and the settings in its fragment, so it reaches no
// server: there is nothing to store, nothing to expire, and nothing logged.
// `share.js` holds the encoding and the three forms; here is only what the page
// does with them.

/// The script as the scenario list served it. Share compares against this to
/// decide whether the script itself has to travel.
const pristine = ref({ file: '', text: '' })

/// The link most recently made, shown until it is dismissed.
const shareLink = ref('')
const shareNote = ref('')
const shareField = ref(null)

/// What opening a link did, and what went wrong if it did not open.
const sharedNotice = ref('')
const shareError = ref('')

/// What the editor held before a link replaced it.
///
/// Kept here rather than read back from the session: the autosave overwrites
/// the stored copy within half a second of the shared script arriving, and this
/// also covers work done since the page was opened, which the stored copy would
/// not have.
const displaced = ref(null)
const canRestore = computed(() => !!displaced.value)

/// Everything a link can change, as it stands.
function currentState() {
  return {
    script: script.value,
    seed: seed.value,
    produce: produce.value,
    maxGenerate: maxGenerate.value,
    format: format.value,
    ddTables: ddTables.value,
    roundRobin: roundRobin.value,
    dealSource: dealSource.value,
    scenario: selectedFile.value,
    paramValues: paramValues.value,
    newSeedEachRun: newSeedEachRun.value,
    measureSeconds: measureSeconds.value,
    autoLevel: autoLevel.value,
    autoLevelTouched: autoLevelTouched.value,
    pristine: pristine.value,
  }
}

function sharedMeasureSeconds() {
  if (!autoLevel.value || !hasHandTypes.value || measureSeconds.value == null) return undefined
  const engineDefault = engineReady.value ? defaultMeasureSeconds() : null
  return measureSeconds.value === engineDefault ? undefined : measureSeconds.value
}

async function onShare() {
  shareError.value = ''
  const doc = makeDocument(script.value, {
    seed: seed.value,
    produce: produce.value,
    maxGenerate: maxGenerate.value,
    format: format.value,
    autoLevel: autoLevel.value && hasHandTypes.value,
    roundRobin: roundRobinAsked.value,
    ddTags: ddTags.value,
    // What the run would send, rather than what has been typed: a value for a
    // parameter the script does not use would not change the run, and should
    // not change the link either.
    params: paramSpecs.value,
    // Only when the run would use it, and only when it is not the engine's own
    // number. A link saying twenty seconds when twenty is the default says
    // nothing — and would go on saying it after the default had changed.
    measureSeconds: sharedMeasureSeconds(),
    dealSource: dealSource.value,
    newSeedEachRun: newSeedEachRun.value,
    scenario: selectedFile.value,
  })
  let fragment
  try {
    fragment = await shareFragment(doc, {
      pristineScript: pristine.value.file === selectedFile.value ? pristine.value.text : null,
    })
  } catch (e) {
    shareLink.value = ''
    shareError.value = e?.message || String(e)
    return
  }
  shareLink.value = shareUrl(window.location, fragment)
  const copied = await copyText(shareLink.value)
  // Short, because the panel it sits in is squeezed on a phone \u2014 which is
  // where a link is most likely to be read.
  shareNote.value =
    (copied ? 'Copied. ' : 'The browser refused to copy \u2014 take it from here. ') +
    (fragment.startsWith('#s=')
      ? 'It names the scenario and your settings; the script comes from the same list.'
      : `The whole script is in the link \u2014 ${shareLink.value.length} characters, and ` +
        'nothing is uploaded.')
  // Selected, so it can be taken by hand on the platforms where a copy is
  // refused, which are the platforms this matters on.
  await nextTick()
  shareField.value?.select?.()
}

/// Open whatever the fragment names, and stop. Nothing runs: an unknown script
/// can be expensive, the reader should see what they are about to run, and a
/// page that starts work on open makes the back button surprising.
async function openSharedLink() {
  if (!parseFragment(window.location.hash)) return
  // A second link, pasted after a first one failed, must not open underneath
  // the first one's complaint.
  shareError.value = ''
  try {
    const opened = await resolveFragment(window.location.hash, {
      fetchScenario: fetchScenarioScript,
    })
    if (opened) applySharedDocument(opened)
  } catch (e) {
    shareError.value = e?.message || String(e)
  }
}

function applySharedDocument({ source, doc }) {
  const s = doc.settings
  // Only worth keeping when there is something to lose. This is the only way
  // back to it: the page has not navigated, so the back button would leave the
  // site rather than undo this.
  const before = currentState()
  displaced.value = before.script.trim() && before.script !== doc.script ? before : null

  script.value = doc.script
  // A link that names no seed is about a script rather than about particular
  // hands, so every visitor gets their own sample.
  seed.value = s.seed ?? randomSeed()
  produce.value = s.produce
  maxGenerate.value = s.maxGenerate
  format.value = s.format
  ddTables.value = s.ddTags !== 'none'
  roundRobin.value = s.roundRobin
  dealSource.value = s.dealSource
  newSeedEachRun.value = s.newSeedEachRun
  selectedFile.value = s.scenario || ''
  // The conversion this is easiest to miss: the link carries the engine's
  // `N=TEXT`, the fields hold values by number. Without it a shared
  // parameterised scenario arrives blank and runs on its declared defaults.
  paramValues.value = paramValuesFrom(s.params)
  if (s.measureSeconds != null) measureSeconds.value = s.measureSeconds
  autoLevel.value = s.autoLevel
  // The link had an opinion, so the box must not be re-ticked underneath it by
  // the watcher that ticks it for a script naming hand types.
  autoLevelTouched.value = true
  // A shared scenario's script is the list's own, so Share can send it back the
  // short way.
  pristine.value = source === 'scenario' ? { file: s.scenario, text: doc.script } : { file: '', text: '' }

  result.value = null
  leveling.value = null
  error.value = ''
  editorTab.value = 'script'

  const named = source === 'scenario' && s.scenario ? ` \u201c${prettifyLabel(s.scenario)}\u201d` : ''
  // That nothing has run is the part worth saying: it is the difference between
  // this and every other link, and the reason the page looks idle.
  sharedNotice.value = `Opened a shared script${named}. Nothing has run yet.`
}

/// Put back what the link replaced.
function restorePrevious() {
  const was = displaced.value
  sharedNotice.value = ''
  displaced.value = null
  // The link has been declined, so it must not open again on the next reload.
  // The fragment goes; the page does not navigate.
  try {
    window.history.replaceState(null, '', window.location.pathname + window.location.search)
  } catch {
    // Some embeddings refuse it. Losing the fragment is not worth an error.
  }
  if (!was) return
  script.value = was.script
  seed.value = was.seed
  produce.value = was.produce
  maxGenerate.value = was.maxGenerate
  format.value = was.format
  ddTables.value = was.ddTables
  roundRobin.value = was.roundRobin
  dealSource.value = was.dealSource
  selectedFile.value = was.scenario || ''
  paramValues.value = was.paramValues || {}
  newSeedEachRun.value = was.newSeedEachRun ?? false
  if (was.measureSeconds != null) measureSeconds.value = was.measureSeconds
  // `scenario` is the picker's highlight, and it came back with the script.
  autoLevel.value = was.autoLevel ?? false
  autoLevelTouched.value = was.autoLevelTouched
  pristine.value = was.pristine
  result.value = null
  leveling.value = null
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
  // Only when it is going to be used. A cleared field on a run that levels
  // nothing is not a mistake worth stopping for.
  if (autoLevel.value && levelBoxLive.value) limits.push(['Characterize', measureSeconds.value])
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
    // characterizing pass, no new keeps, the same script every time. So Run
    // again is another sample of one levelling rather than a fresh levelling —
    // which is what you want when comparing runs, and what makes the script in
    // the pane worth reading rather than something that moves under you.
    const onLeveled = editorTab.value === 'leveled' && leveledScript.value
    result.value = await generate(onLeveled ? leveledScript.value : script.value, {
      seed: seed.value,
      produce: produce.value,
      roundRobin: roundRobinAsked.value,
      maxGenerate: maxGenerate.value,
      format: format.value,
      ddTags: ddTags.value,
      params: paramSpecs.value,
      autoLevel: !onLeveled && autoLevel.value && hasHandTypes.value,
      // Left out while the field is still empty, so the engine's own default
      // applies rather than a zero.
      measureSeconds: measureSeconds.value ?? undefined,
      // The deals themselves. Everything above is the same either way, which is
      // the point of it being a choice of source rather than a second mode.
      source: dealSource.value,
      signal: abort.signal,
      onProgress: (report) => {
        phases.value = { ...phases.value, [report.phase]: report }
      },
      onLibrary: (status) => {
        libraryStatus.value = libraryStatusText(status)
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
    libraryStatus.value = ''
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
  /* A statistic whose label indents itself is a breakdown of the one above.
     A different hue rather than a lighter accent: a tint pale enough to read
     as subordinate came to 2.3:1 against the track, under the 3:1 a graphical
     object wants, and darkening it until it passed left it indistinguishable
     from the accent. This is 4.6:1 on the track and a different family from
     both the accent and `--natural`.

     The colour is reinforcement, not the signal — the indented label already
     says which line these belong to, so nothing is lost by a reader who
     cannot tell the two hues apart. */
  --accent-sub: #7a5cc4;
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

/* Wraps, which is the whole of why this page did not fit a phone.
   Eight items at `nowrap` — title, subtitle, three links, version, threads —
   needed 446px in a 390px viewport, and the page scrolled sideways by exactly
   that 56px. Measured rather than guessed: setting this one property took
   `documentElement.scrollWidth` from 446 to 390. The editor and the results
   were never the problem; both already clip inside their own scrollers. */
.bar {
  display: flex; flex-wrap: wrap; align-items: baseline; gap: 10px;
  padding: 8px 14px; border-bottom: 1px solid var(--line); background: var(--bg-subtle);
}
.bar h1 { font-size: 15px; margin: 0; }
.bar-sub { font-size: 12px; color: var(--fg-muted); }
.bar-spacer { flex: 1; }
.bar-link { font-size: 12px; color: var(--accent); text-decoration: none; }
.bar-link:hover { text-decoration: underline; }
.bar-version { font-size: 11px; color: var(--fg-muted); font-family: var(--mono); }
/* Beside the version, and marked when it says one: on this site one thread
   means the headers are not arriving, not that the browser cannot. */
.bar-threads { border-left: 1px solid var(--line); padding-left: 10px; }
.bar-threads.lonely { color: var(--warn-fg); }

.cols { display: grid; grid-template-columns: 260px 1fr 1fr; flex: 1; min-height: 0; }
/* The first column shrinks to the rail; the two `1fr` columns take the 232px
   it gave up between them, which is the point of closing it. */
.cols.picker-closed { grid-template-columns: 28px 1fr 1fr; }
.col { min-width: 0; min-height: 0; }
.col-picker { border-right: 1px solid var(--line); }
.col-editor { display: flex; flex-direction: column; padding: 8px; gap: 8px; min-height: 0; }
.col-results { border-left: 1px solid var(--line); min-height: 0; }

/* Shared by the settings panel and the Run row: both hold small labelled
   fields, and a field should not change size for having moved between them.

   These were written for `.controls`, the row the panel replaced. Renaming the
   container orphaned every rule below — the number fields lost the widths at
   the end of this block and fell back to the browser's default, which is how
   Produce came to be wider than Seed. Nothing failed; it just looked wrong. */
.settings-panel label, .run-row label {
  display: flex; align-items: center; gap: 4px; color: var(--fg-muted);
}
.settings-panel input, .settings-panel select, .run-row input {
  font: inherit; font-size: 12px; padding: 3px 5px;
  border: 1px solid var(--line); border-radius: 3px; background: var(--bg); color: var(--fg);
  /* The widths below are in `ch` and `em`, so they follow the font. If anything
     ever inflates it again — a platform, a user's setting — a field grows and
     its row does not. These two stop it from taking the page with it: a label
     that may shrink, and a field that may not exceed what holds it. */
  max-width: 100%; min-width: 0;
}
.settings-panel label, .run-row label { min-width: 0; }
/* Numeric fields sized for the values they actually hold.
   A number input draws its spinner INSIDE its own box, and the box also carries
   the field's padding and border, so the digits are clipped well before the
   border is reached. The old widths were set by eye against that and both came
   up short: `Produce` at 4.5em showed `2000(` for 20000, and the 7em the other
   two shared runs out around eight digits, two short of a ten-figure seed.
   So the width is stated as what it has to hold rather than as a round number:
   `--num-digits` of text, plus `--num-chrome` for everything the browser draws
   around it. */
.settings-panel, .run-row { --num-chrome: 2.6em; }
.settings-panel input[type="number"], .run-row input[type="number"] {
  width: calc(var(--num-digits, 8) * 1ch + var(--num-chrome));
}
/* A board count. Seven digits is a million boards — far past anything anyone
   asks a browser for, and still the narrowest of the three. */
input.num-produce { --num-digits: 7; }
/* Up to 10,000,000, which the field's own arrows will walk it to. */
input.num-generate { --num-digits: 8; }
/* A u32: 4294967295, and the widest thing on the row. */
input.num-seed { --num-digits: 10; }
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
/* Flex rather than three fixed columns. It was `1fr auto 1fr` — tabs, one
   centred checkbox, Run at the end — and a second checkbox made four items for
   three columns, so Run wrapped onto a line of its own and took a line of
   script with it. Wrapping is now the row's own decision, so everything shares
   a line while there is room for it. */
.run-row {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 6px 14px;
}
/* Run to the far right, whichever line it ends up on. */
.run-row > .run { margin-left: auto; }
/* The gear sits with Run, not adrift at the far edge: `margin-left: auto` on
   Run pushes everything before it left, so placing this immediately before Run
   keeps the pair together however wide the row gets. */
/* Sized and bordered like Cancel, because it is that kind of control: a
   secondary button on the Run row. It first shipped with `var(--border)` and
   `var(--bg-raised)`, neither of which this file defines — and an undefined
   `var()` inside the `border` shorthand makes the whole declaration invalid,
   which resolves to `border-style: none`. So it had no border, no background
   and no box at all, and nothing anywhere reported a problem.

   The row is `align-items: end`, so what makes this look level with its
   neighbours is its box height matching theirs rather than any alignment
   property. 4 + 4 padding, 16px glyph and 2px border is 26px, against Run's
   ~26 and Cancel's ~28. */
.settings-toggle {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 32px;
  padding: 4px 9px; font-size: 16px; line-height: 1; cursor: pointer;
  border: 1px solid var(--line); border-radius: 4px;
  background: #fff; color: var(--fg-muted);
}
.settings-toggle:hover { color: var(--fg); border-color: var(--fg-muted); }
/* Open is a state, not a hover: with the panel below, the button has to say
   which of the two it is, or the only way to tell is to look away from it. */
.settings-toggle.on { color: var(--accent); border-color: var(--accent); }

.settings-panel {
  display: flex; flex-wrap: wrap; align-items: center; gap: 6px 14px;
  padding: 8px 10px; margin: 0 0 6px;
  border: 1px solid var(--line); border-radius: 6px; background: var(--bg-subtle);
}
.settings-panel label { font-size: 0.82rem; white-space: nowrap; }
/* Wraps onto its own line rather than sitting in the flow of controls: it is a
   sentence, and a sentence between two number fields reads as a label. */
.settings-note { flex-basis: 100%; margin: 2px 0 0; }


/* The characterizing budget sits on this row rather than among the controls
   above, so it inherits none of their sizing. Three digits: 300s is the most
   the engine accepts. */
.run-row label { font-size: 0.82rem; white-space: nowrap; }
.run-row input.num-measure { width: calc(3ch + 2.6em); }

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

/* Quieter than Run, like Cancel, and the same height as both: the row is
   `align-items: end`, so what makes these look level is their boxes matching
   rather than any alignment property. */
.share {
  padding: 5px 12px; font: inherit; font-size: 13px;
  border: 1px solid var(--line); border-radius: 4px;
  background: #fff; color: var(--fg-muted); cursor: pointer;
}
.share:hover:not(:disabled) { color: var(--accent); border-color: var(--accent); }
.share:disabled { color: var(--line); cursor: default; }

/* Opens under the row that made it, like the settings panel and for the same
   reason: a disclosure belongs below the control that owns it. */
.share-panel {
  display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px;
  padding: 8px 10px; margin: 0 0 6px;
  border: 1px solid var(--line); border-radius: 6px; background: var(--bg-subtle);
}
/* Takes the row: a link that is cut off is one that gets pasted cut off.
   `min-width: 0` because a flex item will otherwise refuse to shrink below the
   width of its content, and this content is several hundred characters. */
.share-input {
  flex: 1; min-width: 0;
  font: 12px/1.5 var(--mono);
  padding: 4px 6px;
  border: 1px solid var(--line); border-radius: 3px;
  background: var(--bg); color: var(--fg);
}
/* The same size as the notes under the run controls, and for the same reason:
   this is a line about a control, not body text. At 14px it was three lines on
   a phone, and the panel is squeezed there already. */
.share-note { color: var(--fg-muted); font-size: 11.5px; line-height: 1.45; }

/* What a link did to the page, above the script it did it to. Not styled as a
   warning: opening a shared script is the feature working. */
.shared-notice {
  display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px;
  margin: 0 0 6px; padding: 6px 9px;
  font-size: 11.5px; line-height: 1.45;
  border-left: 3px solid var(--accent);
  background: var(--accent-subtle); color: var(--fg);
}
/* A link that could not be opened at all. */
.shared-notice.bad {
  border-left-color: var(--warn);
  background: var(--warn-subtle);
  color: var(--warn-fg);
}
.shared-restore, .shared-dismiss {
  font: inherit; font-size: 11px;
  padding: 2px 8px;
  border: 1px solid var(--line); border-radius: 3px;
  background: var(--bg); color: var(--fg-muted); cursor: pointer;
}
.shared-restore { color: var(--accent); border-color: var(--accent); }
.shared-restore:hover, .shared-dismiss:hover { background: var(--bg-subtle); }

/* The deal source, set apart from the numeric fields beside it: it is the one
   control on the row that changes what a run reads rather than how much of it.
   The rule after it does that without a second row. */
.source {
  display: inline-flex; align-items: center; gap: 10px;
  padding-right: 10px; border-right: 1px solid var(--line);
}

/* Both sit under the controls in the space a script would otherwise start in,
   so they are read before Run rather than after it. */
.source-note, .source-warn {
  margin: 6px 0 0; font-size: 11.5px; line-height: 1.45;
}
.source-note { color: var(--fg-muted); }
.source-warn {
  color: var(--warn-fg);
  background: var(--warn-subtle);
  border-left: 3px solid var(--warn);
  padding: 5px 8px;
}

/* Shown the moment a fetch starts, where the progress bars are held back for a
   second: a network wait has nothing else to show for itself. */
.library-status {
  margin: 6px 0 2px;
  font-size: 11px; font-family: var(--mono); color: var(--fg-muted);
}

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
  height: 5px; border-radius: 3px; background: var(--line);
  /* `relative` for the expected mark, which is positioned within it, and not
     `overflow: hidden` because that mark stands slightly proud of a 5px bar to
     be visible at all. The fill rounds its own corners. */
  position: relative;
}
.progress-fill {
  display: block; height: 100%; border-radius: 3px;
  background: var(--accent);
  transition: width 0.12s linear;
}
/* No total to draw against — a plain run is bounded by the deals asked for, but
   nothing says how many it will have to look at to find them. */
.progress-fill.indeterminate {
  width: 35%;
  animation: progress-sweep 1.1s ease-in-out infinite;
}
@keyframes progress-sweep {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(300%); }
}
.progress-count { font-variant-numeric: tabular-nums; }
/* Where the pass is expected to end. A hairline rather than a block: it marks a
   position, and the bar filling up to it is the thing to watch. */
.progress-mark {
  position: absolute; top: -2px; bottom: -2px; width: 2px;
  background: var(--warn, #b45309); border-radius: 1px;
  transition: left 0.3s ease-out;
}

@media (prefers-reduced-motion: reduce) {
  .progress-fill.indeterminate { animation: none; width: 100%; opacity: 0.4; }
}

/* The rail the closed list leaves behind: the whole column is the way back, so
   it cannot be missed and does not need aiming at. Vertical, because 28px of
   width is what was freed and a horizontal label would not fit in it. */
.picker-open {
  width: 100%; height: 100%;
  display: flex; flex-direction: column; align-items: center; gap: 8px;
  padding: 10px 0;
  border: 0; background: var(--bg-subtle); color: var(--fg-muted);
  font: inherit; font-size: 12px; cursor: pointer;
}
.picker-open:hover { background: var(--accent-subtle); color: var(--fg); }
.picker-open-label { writing-mode: vertical-rl; letter-spacing: 0.04em; }

/* An iPad in landscape already gets the three columns; what it does not have is
   a desktop's width to spend on them. The scenario list takes 260 of 1180 —
   more than a fifth — leaving 460 each for the script and the results, which
   are the two things being read against each other. Narrower here, wider there.

   `.cols.picker-closed` still wins on specificity, so closing the list still
   collapses it to the rail. */
@media (min-width: 1001px) and (max-width: 1250px) {
  .cols { grid-template-columns: 200px 1fr 1fr; }
}

@media (max-width: 1000px) {
  .cols { grid-template-columns: 1fr; grid-template-rows: auto 1fr 1fr; }
  /* Stacked, the list is a row rather than a column, so the closed rail is a
     strip across the top and its label reads the ordinary way round. */
  .cols.picker-closed { grid-template-columns: 1fr; }
  /* A share of the height rather than a fixed slice of it. 220px is a fifth of
     an iPad held upright and well over half a phone held sideways, where it left
     57px each for the editor and the results — the two things actually being
     used. The list is a means; it gives up the room. */
  .col-picker {
    border-right: 0; border-bottom: 1px solid var(--line);
    max-height: min(220px, 30vh);
  }
  .col-picker.is-closed { max-height: none; }
  .picker-open { height: auto; flex-direction: row; padding: 6px 10px; }
  .picker-open-label { writing-mode: horizontal-tb; }
  .col-results { border-left: 0; border-top: 1px solid var(--line); }

  /* Stacked, the editor column has a fixed share of the height, and opening a
     panel in it — settings, the share link — used to take every pixel of the
     deficit out of the script pane. Its root is `height: 100%; min-height: 0`
     with no flex of its own, so nothing stopped it at zero: the reader saw the
     Run row, the panel, and the status strip with no script between them, and
     the column's overflow was painted over the results below (#119).

     Two rules, and they need each other. The column scrolls, so what does not
     fit is scrolled to rather than drawn over the next row. And the panes have
     a floor, so shrinking stops somewhere that still shows a few lines — a
     scroll container alone was tried first, and the pane collapsed inside it
     just the same, since `height: 100%` resolves against the scroller's own
     height and flex still shrinks it to fit.

     The floor is chosen against what the pane gets with nothing open, measured
     at 390x844 before this change: 152px plain, 82px on the page a shared link
     lands on (the notice above it), 78px with the Pre-solved warning showing.
     4rem is 64px — the status strip and about two lines of script — which sits
     below all three with room to spare. It first went in at 5rem, 80px, which
     cleared the link's page by two pixels and fell inside the 78: every one of
     those states would then have scrolled that did not before, and a real
     phone's font metrics are not a desktop's at phone width. So none of them
     move; only a panel pushing past the floor starts the column scrolling.
     Both panes, because the Leveled tab's viewer is built the same way and
     collapses the same way. */
  .col-editor { overflow-y: auto; }
  .col-editor > .editor,
  .col-editor > .viewer-wrap { min-height: 4rem; }
}

/* A phone is where someone reads a run, not where they write one — the script
   language wants a keyboard. So the results take the room, and the editor keeps
   enough to see what is about to run and to reach Run itself. The subtitle goes:
   it is decorative, and on a header that now wraps it costs a whole line.

   This is the layout a shared link lands in (#98, #99): the reader taps, sees
   what came out, and scrolls up to the script only if they want it. */
@media (max-width: 620px) {
  .bar-sub { display: none; }
  /* The spacer pushes the links to the right of the title, which is what a
     single-line header wants and a wrapped one does not: it grows only on the
     line it lands on, so the first line ends up right-aligned against a second
     that starts at the left. Without it the whole header simply packs. */
  .bar-spacer { display: none; }
  .cols { grid-template-rows: auto minmax(0, 0.8fr) minmax(0, 1.7fr); }
  .col-picker { max-height: 150px; }
}
</style>
