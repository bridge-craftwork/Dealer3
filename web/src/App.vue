<template>
  <div class="app">
    <header class="bar">
      <h1>dealer3</h1>
      <span class="bar-sub">bridge hand generator — runs entirely in your browser</span>
      <span class="bar-spacer"></span>
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
          <label>Seed <input v-model.number="seed" type="number" min="0" /></label>
          <label>Produce <input v-model.number="produce" type="number" min="1" /></label>
          <label>Max generate <input v-model.number="maxGenerate" type="number" min="1" step="1000" /></label>
          <label>
            Format
            <select v-model="format">
              <option value="oneline">One line</option>
              <option value="printall">Print all</option>
              <option value="pbn">PBN</option>
            </select>
          </label>
          <button class="run" :disabled="!engineReady || running || !scriptValid" @click="run">
            {{ running ? 'Running…' : 'Run' }}
          </button>
        </div>

        <ScriptEditor v-model="script" @validity="onValidity" />
      </section>

      <section class="col col-results">
        <ResultsPanel
          :result="result"
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
import { ref, onMounted, nextTick } from 'vue'
import ScenarioPicker from '@/components/ScenarioPicker.vue'
import ScriptEditor from '@/components/ScriptEditor.vue'
import ResultsPanel from '@/components/ResultsPanel.vue'
import PrintView from '@/components/PrintView.vue'
import { ready, generate, version } from '@/lib/engine.js'
import { fetchScenarioScript } from '@/lib/pbsScenarios.js'
import { downloadText, resultFilename, statisticsText } from '@/lib/download.js'

const STARTER = `# Write a dealer script, or pick a scenario on the left.
condition hcp(north) >= 15 && shape(north, any 4333 + any 4432 + any 5332)

action printoneline,
  average "N HCP" hcp(north),
  frequency "N HCP" (hcp(north), 15, 22)
`

const script = ref(STARTER)
const seed = ref(1)
const produce = ref(20)
const maxGenerate = ref(500000)
const format = ref('oneline')

const engineReady = ref(false)
const engineVersion = ref('')
const running = ref(false)
const result = ref(null)
const error = ref('')
const scriptValid = ref(true)
const selectedFile = ref('')
const loadingFile = ref('')
const downloading = ref(false)

onMounted(async () => {
  await ready()
  engineReady.value = true
  engineVersion.value = version()
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
      const pbn = generate(script.value, {
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
      const text = generate(script.value, {
        seed: seed.value,
        produce: produce.value,
        maxGenerate: maxGenerate.value,
        format: format.value === 'pbn' ? 'oneline' : format.value,
      })
      const body = text.deals.join('\n') + '\n' + statisticsText(text)
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
  running.value = true
  error.value = ''
  try {
    // Generation is synchronous inside the wasm module and will block the tab.
    // Yield a frame first so the button can paint its running state; the
    // max-generate bound is what actually keeps the block short.
    await new Promise((r) => requestAnimationFrame(r))
    result.value = generate(script.value, {
      seed: seed.value,
      produce: produce.value,
      maxGenerate: maxGenerate.value,
      format: format.value,
    })
  } catch (e) {
    result.value = null
    error.value = e?.message || String(e)
  } finally {
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
.controls input[type="number"] { width: 7em; }
.editor-loading {
  flex: 1; display: grid; place-items: center;
  color: var(--fg-muted); font-size: 13px;
  border: 1px solid var(--line); border-radius: 4px;
}
.run {
  margin-left: auto; padding: 5px 16px; font: inherit; font-size: 13px; font-weight: 500;
  border: 0; border-radius: 4px; background: var(--accent); color: #fff; cursor: pointer;
}
.run:disabled { background: var(--line); color: var(--fg-muted); cursor: default; }

@media (max-width: 1000px) {
  .cols { grid-template-columns: 1fr; grid-template-rows: auto 1fr 1fr; }
  .col-picker { border-right: 0; border-bottom: 1px solid var(--line); max-height: 220px; }
  .col-results { border-left: 0; border-top: 1px solid var(--line); }
}
</style>
