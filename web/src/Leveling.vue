<template>
  <div class="page">
    <header class="bar">
      <h1>dealer3</h1>
      <span class="bar-sub">levelling guide</span>
      <span class="bar-spacer"></span>
      <a class="bar-link" href="./reference.html">Language reference</a>
      <a class="bar-link" href="./index.html">Back to the app</a>
    </header>

    <main class="wrap">
      <!-- The document, rendered from the same `docs/leveling-guide.md` that
           GitHub shows and CI builds the PDF from. `v-html` on content that is
           part of this build, not anything a user supplied. -->
      <!-- eslint-disable-next-line vue/no-v-html -->
      <article class="doc" v-html="html"></article>
    </main>
  </div>
</template>

<script setup>
// `?raw` rather than a copy under src/: the guide belongs in docs/, where it is
// read alongside the strategy document and picked up by the PDF workflow. Vite
// inlines it at build time, so the page ships the text and fetches nothing.
import source from '../../docs/leveling-guide.md?raw'
import { renderGuide } from '@/lib/guide.js'

const html = renderGuide(source)
</script>

<style>
/* Unscoped: the document is `v-html`, so scoped rules would not reach it. */
:root {
  --ink: #1a1a1a;
  --ink-soft: #555;
  --rule: #e2e2e2;
  --bg: #fff;
  --bg-soft: #f7f7f7;
  --accent: #1d5f8a;
  --bar-bg: #22303a;
}

body {
  margin: 0;
  color: var(--ink);
  background: var(--bg);
  font: 16px/1.65 -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.bar {
  display: flex;
  align-items: baseline;
  gap: 0.75rem;
  padding: 0.6rem 1.25rem;
  background: var(--bar-bg);
  color: #fff;
}
.bar h1 {
  margin: 0;
  font-size: 1.05rem;
  font-weight: 600;
}
.bar-sub {
  font-size: 0.85rem;
  opacity: 0.75;
}
.bar-spacer {
  flex: 1;
}
.bar-link {
  color: #cfe4f2;
  font-size: 0.85rem;
  text-decoration: none;
}
.bar-link:hover {
  text-decoration: underline;
}

.wrap {
  max-width: 46rem;
  margin: 0 auto;
  padding: 2rem 1.25rem 5rem;
}

/* The document ------------------------------------------------------------ */

.doc h1 {
  font-size: 1.9rem;
  line-height: 1.2;
  margin: 0 0 1.5rem;
}
.doc h2 {
  font-size: 1.35rem;
  margin: 2.75rem 0 0.75rem;
  padding-bottom: 0.3rem;
  border-bottom: 1px solid var(--rule);
}
.doc h3 {
  font-size: 1.05rem;
  margin: 2rem 0 0.5rem;
}
.doc p,
.doc li {
  margin: 0.75rem 0;
}
.doc a {
  color: var(--accent);
}

/* Anchored headings land clear of nothing in particular here, but a little
   breathing room makes a jump from the contents list read as an arrival. */
.doc h2,
.doc h3 {
  scroll-margin-top: 1rem;
}

.doc code {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.87em;
  background: var(--bg-soft);
  border: 1px solid var(--rule);
  border-radius: 3px;
  padding: 0.08em 0.32em;
}
.doc pre {
  background: var(--bg-soft);
  border: 1px solid var(--rule);
  border-radius: 5px;
  padding: 0.8rem 1rem;
  overflow-x: auto;
  line-height: 1.5;
}
.doc pre code {
  background: none;
  border: 0;
  padding: 0;
  font-size: 0.85rem;
}

.doc blockquote {
  margin: 1rem 0;
  padding: 0.1rem 1rem;
  border-left: 3px solid var(--accent);
  background: var(--bg-soft);
  color: var(--ink-soft);
}
.doc blockquote strong {
  color: var(--ink);
}

.doc hr {
  border: 0;
  border-top: 1px solid var(--rule);
  margin: 3rem 0;
}

/* Screenshots of the app. Bordered because several have white grounds and
   would otherwise bleed into the page. */
.doc figure {
  margin: 1.4rem 0;
}
.doc figure img {
  display: block;
  max-width: 100%;
  height: auto;
  border: 1px solid var(--rule);
  border-radius: 5px;
  background: #fff;
}
.doc figcaption {
  margin-top: 0.5rem;
  font-size: 0.85rem;
  color: var(--ink-soft);
}

/* Tables carry most of the measurements, so they get the most attention. */
.table-scroll {
  overflow-x: auto;
  margin: 1.1rem 0;
}
.doc table {
  border-collapse: collapse;
  font-size: 0.9rem;
  min-width: 100%;
}
.doc th,
.doc td {
  border-bottom: 1px solid var(--rule);
  padding: 0.4rem 0.85rem 0.4rem 0;
  text-align: left;
  vertical-align: top;
  white-space: nowrap;
}
/* The refusals table is prose in its second column and has to wrap. */
.doc td:last-child {
  white-space: normal;
  min-width: 18rem;
}
.doc thead th {
  border-bottom: 2px solid #ccc;
  font-weight: 600;
  color: var(--ink-soft);
}
.doc tbody tr:hover {
  background: var(--bg-soft);
}

@media (prefers-color-scheme: dark) {
  :root {
    --ink: #e8e8e8;
    --ink-soft: #a8a8a8;
    --rule: #333;
    --bg: #16181a;
    --bg-soft: #1e2124;
    --accent: #6fb6e0;
  }
  .doc thead th {
    border-bottom-color: #444;
  }
}
</style>
