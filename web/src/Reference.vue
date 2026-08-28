<template>
  <div class="page">
    <header class="bar">
      <h1>dealer3</h1>
      <span class="bar-sub">language reference</span>
      <span class="bar-spacer"></span>
      <a class="bar-link" href="./leveling.html">Levelling guide</a>
      <a class="bar-link" href="./index.html">Back to the app</a>
      <span v-if="engineVersion" class="bar-version">engine {{ engineVersion }}</span>
    </header>

    <main v-if="info" class="wrap">
      <p class="lede">
        Every function, operator and statement dealer3 accepts. This page is generated from the
        engine on this site, so it cannot list something the parser rejects, or leave out something
        it accepts.
      </p>

      <div class="search">
        <label>
          <span class="visually-hidden">Search the reference</span>
          <input
            v-model="query"
            type="search"
            placeholder="Search — try shape, losers, or precedence"
            autocomplete="off"
          />
        </label>
        <span v-if="query.trim()" class="count">
          {{ resultCount }} {{ resultCount === 1 ? 'entry' : 'entries' }}
        </span>
      </div>

      <nav v-if="!query.trim()" class="toc">
        <a href="#statements">Statements</a>
        <a href="#functions">Functions</a>
        <a href="#operators">Operators</a>
        <a href="#actions">Actions</a>
        <a href="#words">Words</a>
        <a href="#unsupported">Not supported</a>
      </nav>

      <p v-if="query.trim() && resultCount === 0" class="empty">
        Nothing matches “{{ query.trim() }}”.
      </p>

      <!-- Statements ---------------------------------------------------- -->
      <section v-if="statements.keyword.length || statements.other.length" id="statements">
        <h2>Statements</h2>
        <p class="section-note">
          A script is a list of these, one per line. Order does not matter, apart from the
          condition: where there is more than one, the last wins.
        </p>

        <dl class="entries">
          <template v-for="doc in statements.keyword" :key="doc.form">
            <dt><code class="sig">{{ doc.form }}</code></dt>
            <dd>
              <p class="summary"><RichText :text="doc.summary" /></p>
              <pre class="example"><code>{{ doc.example }}</code></pre>
              <p v-if="doc.note" class="note"><RichText :text="doc.note" /></p>
            </dd>
          </template>
        </dl>

        <template v-if="statements.other.length">
          <h3>Without a keyword</h3>
          <dl class="entries">
            <template v-for="doc in statements.other" :key="doc.form">
              <dt><code class="sig">{{ doc.form }}</code></dt>
              <dd>
                <p class="summary"><RichText :text="doc.summary" /></p>
                <pre class="example"><code>{{ doc.example }}</code></pre>
                <p v-if="doc.note" class="note"><RichText :text="doc.note" /></p>
              </dd>
            </template>
          </dl>
        </template>
      </section>

      <!-- Functions ----------------------------------------------------- -->
      <section v-if="functions.length" id="functions">
        <h2>Functions</h2>
        <p class="section-note">
          <code>compass</code> is <code>north</code>, <code>east</code>, <code>south</code> or
          <code>west</code>. <code>suit</code> is <code>spades</code>, <code>hearts</code>,
          <code>diamonds</code> or <code>clubs</code>. Every function returns a whole number, and
          zero counts as false.
        </p>

        <template v-for="section in functions" :key="section.group">
          <h3>{{ section.group }}</h3>
          <dl class="entries">
            <template v-for="doc in section.entries" :key="doc.name">
              <dt :class="{ alias: doc.alias_of }">
                <code class="sig">{{ doc.signature }}</code>
              </dt>
              <dd :class="{ alias: doc.alias_of }">
                <p class="summary"><RichText :text="doc.summary" /></p>
                <template v-if="!doc.alias_of">
                  <pre class="example"><code>{{ doc.example }}</code></pre>
                  <p v-if="doc.note" class="note"><RichText :text="doc.note" /></p>
                </template>
              </dd>
            </template>
          </dl>
        </template>
      </section>

      <!-- Operators ----------------------------------------------------- -->
      <section v-if="operators.length" id="operators">
        <h2>Operators</h2>
        <p class="section-note">
          Tightest binding first. Operators on the same row bind equally and are applied left to
          right. Brackets override all of it.
        </p>

        <div v-for="(level, i) in operators" :key="level.precedence" class="level">
          <p class="level-label">
            <span class="level-num">{{ i + 1 }}</span>
            <span v-if="i === 0">binds tightest</span>
            <span v-else-if="i === operators.length - 1">binds loosest</span>
          </p>
          <dl class="entries">
            <template v-for="doc in level.entries" :key="doc.symbol">
              <dt>
                <code class="sig">{{ doc.symbol }}</code>
                <span v-if="doc.word" class="word-form">or <code>{{ doc.word }}</code></span>
              </dt>
              <dd>
                <p class="summary"><RichText :text="doc.summary" /></p>
                <pre class="example"><code>{{ doc.example }}</code></pre>
                <p v-if="doc.note" class="note"><RichText :text="doc.note" /></p>
              </dd>
            </template>
          </dl>
        </div>
      </section>

      <!-- Actions ------------------------------------------------------- -->
      <section v-if="actions.length" id="actions">
        <h2>Actions</h2>
        <p class="section-note">
          How matching deals are printed. Write one on its own line, or in an
          <code>action</code> list.
        </p>
        <dl class="entries">
          <template v-for="doc in actions" :key="doc.name">
            <dt><code class="sig">{{ doc.name }}</code></dt>
            <dd>
              <p class="summary"><RichText :text="doc.summary" /></p>
              <p v-if="doc.note" class="note"><RichText :text="doc.note" /></p>
            </dd>
          </template>
        </dl>
      </section>

      <!-- Words --------------------------------------------------------- -->
      <section v-if="!query.trim() && words.length" id="words">
        <h2>Words</h2>
        <dl class="entries">
          <template v-for="list in words" :key="list.title">
            <dt>{{ list.title }}</dt>
            <dd>
              <p class="summary">
                <code v-for="word in list.words" :key="word" class="chip">{{ word }}</code>
              </p>
              <p v-if="list.note" class="note"><RichText :text="list.note" /></p>
            </dd>
          </template>
        </dl>
      </section>

      <!-- Not supported ------------------------------------------------- -->
      <section v-if="notSupported.length" id="unsupported">
        <h2>Not supported</h2>
        <p class="section-note">
          Words the original dealer accepts that dealer3 does not. Each is reserved, so using one
          is a syntax error the editor will underline rather than something that quietly changes
          what your script means.
        </p>
        <dl class="entries">
          <template v-for="entry in notSupported" :key="entry.name">
            <dt><code class="sig">{{ entry.name }}</code></dt>
            <dd><p class="summary"><RichText :text="entry.instead" /></p></dd>
          </template>
        </dl>
      </section>

      <footer class="foot">
        <p>
          Generated from the engine's own vocabulary. Descriptions were written against
          <a href="https://www.bridgebase.com/tools/dealer/Manual/input.html">
            Henk Uijterwaal's manual
          </a>
          for the original dealer, and every example on this page is parsed and run by the test
          suite.
        </p>
        <p>
          <a href="./index.html">Back to the app</a>
          ·
          <a href="https://github.com/bridge-craftwork/Dealer3/issues">Feedback &amp; issues</a>
        </p>
      </footer>
    </main>

    <p v-else-if="error" class="loading error">{{ error }}</p>
    <p v-else class="loading">Loading the engine…</p>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { ready, languageInfo, version } from '@/lib/engine.js'
import RichText from '@/components/RichText.vue'
import {
  functionSections,
  operatorLevels,
  statementSections,
  filterInfo,
  countEntries,
  wordLists,
} from '@/lib/reference.js'

const info = ref(null)
const engineVersion = ref('')
const error = ref('')
const query = ref('')

// The engine has to finish loading before `language_info()` can be called at
// all — calling early fails inside the generated bindings with a message about
// `__wbindgen_free`, which says nothing about the actual mistake. This page is
// nothing but that call, so it renders a loading line until then.
onMounted(async () => {
  try {
    await ready()
    info.value = languageInfo()
    engineVersion.value = version()
  } catch (e) {
    error.value = `Could not load the engine: ${e?.message || e}`
  }
})

const filtered = computed(() => (info.value ? filterInfo(info.value, query.value) : null))
const resultCount = computed(() => (filtered.value ? countEntries(filtered.value) : 0))

const functions = computed(() => (filtered.value ? functionSections(filtered.value) : []))
const operators = computed(() => (filtered.value ? operatorLevels(filtered.value) : []))
const statements = computed(() =>
  filtered.value ? statementSections(filtered.value) : { keyword: [], other: [] },
)
const actions = computed(() => filtered.value?.action_docs ?? [])
const notSupported = computed(() => filtered.value?.not_supported ?? [])
const words = computed(() => (info.value ? wordLists(info.value) : []))
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
  --mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}

* { box-sizing: border-box; }
html, body, #app { margin: 0; }
body {
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  color: var(--fg);
  background: var(--bg);
}
</style>

<style scoped>
.bar {
  display: flex; align-items: baseline; gap: 10px;
  padding: 8px 14px; border-bottom: 1px solid var(--line); background: var(--bg-subtle);
  position: sticky; top: 0; z-index: 2;
}
.bar h1 { font-size: 15px; margin: 0; }
.bar-sub { font-size: 12px; color: var(--fg-muted); }
.bar-spacer { flex: 1; }
.bar-link { font-size: 12px; color: var(--accent); text-decoration: none; }
.bar-link:hover { text-decoration: underline; }
.bar-version { font-size: 11px; color: var(--fg-muted); font-family: var(--mono); }

.wrap { max-width: 820px; margin: 0 auto; padding: 20px 18px 60px; }

.lede { font-size: 14px; line-height: 1.6; color: var(--fg-muted); margin: 4px 0 18px; }

.search { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
.search label { flex: 1; }
.search input {
  width: 100%; font: inherit; font-size: 13px; padding: 6px 9px;
  border: 1px solid var(--line); border-radius: 4px; background: var(--bg); color: var(--fg);
}
.search input:focus { outline: 2px solid var(--accent-subtle); border-color: var(--accent); }
.count { font-size: 12px; color: var(--fg-muted); white-space: nowrap; }

.toc { display: flex; flex-wrap: wrap; gap: 12px; margin: 0 0 24px; font-size: 13px; }
.toc a { color: var(--accent); text-decoration: none; }
.toc a:hover { text-decoration: underline; }

.empty { font-size: 14px; color: var(--fg-muted); }

section { margin: 0 0 34px; scroll-margin-top: 48px; }
h2 {
  font-size: 17px; margin: 0 0 6px; padding-bottom: 5px;
  border-bottom: 1px solid var(--line);
}
h3 { font-size: 14px; margin: 22px 0 8px; color: var(--fg-muted); font-weight: 600; }
.section-note { font-size: 13px; line-height: 1.6; color: var(--fg-muted); margin: 0 0 14px; }
.section-note code { font-family: var(--mono); font-size: 12px; }

/* A definition list, not a table: the descriptions are prose of varying length
   and a two-column table makes every row as tall as its longest cell. */
.entries { margin: 0; }
.entries dt {
  font-family: var(--mono); font-size: 13px; margin: 16px 0 0;
  display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap;
}
.entries dd { margin: 3px 0 0 0; }

.sig { color: var(--fg); font-weight: 600; }
.word-form { font-family: system-ui, sans-serif; font-size: 12px; color: var(--fg-muted); }
.word-form code { font-family: var(--mono); }

.summary { font-size: 13.5px; line-height: 1.6; margin: 0; }
.note {
  font-size: 12.5px; line-height: 1.6; color: var(--fg-muted); margin: 5px 0 0;
  padding-left: 10px; border-left: 2px solid var(--line);
}
.level-label {
  display: flex; align-items: baseline; gap: 7px;
  margin: 14px 0 0; font-size: 11.5px; color: var(--fg-muted);
  text-transform: uppercase; letter-spacing: 0.04em;
}
.level-num {
  display: inline-grid; place-items: center;
  width: 18px; height: 18px; border-radius: 50%;
  background: var(--accent-subtle); color: var(--accent);
  font-size: 11px; font-weight: 600; letter-spacing: 0;
}

.example {
  font-family: var(--mono); font-size: 12.5px; margin: 6px 0 0;
  padding: 6px 9px; background: var(--bg-subtle); border-radius: 4px;
  overflow-x: auto; white-space: pre;
}
.example code { font: inherit; }

/* Alternative spellings are one line and dimmed: findable by search, but not
   competing with the function they point at. */
.entries dt.alias { margin-top: 8px; font-weight: normal; }
.entries dt.alias .sig { font-weight: normal; color: var(--fg-muted); }
.entries dd.alias .summary { font-size: 12.5px; color: var(--fg-muted); }

.chip {
  font-family: var(--mono); font-size: 12px;
  padding: 1px 6px; margin-right: 5px;
  background: var(--bg-subtle); border-radius: 3px;
}

.level { border-bottom: 1px dashed var(--line); padding-bottom: 10px; margin-bottom: 4px; }
.level:last-child { border-bottom: 0; }

.foot { margin-top: 40px; padding-top: 14px; border-top: 1px solid var(--line); }
.foot p { font-size: 12.5px; line-height: 1.6; color: var(--fg-muted); margin: 0 0 6px; }
.foot a { color: var(--accent); }

.loading { padding: 40px 18px; text-align: center; color: var(--fg-muted); font-size: 14px; }
.loading.error { color: var(--danger); }

.visually-hidden {
  position: absolute; width: 1px; height: 1px; overflow: hidden;
  clip: rect(0 0 0 0); white-space: nowrap;
}

@media (max-width: 600px) {
  .wrap { padding: 16px 12px 40px; }
  .bar-sub { display: none; }
}
</style>
