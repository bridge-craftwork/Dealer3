<template>
  <div class="grid">
    <article v-for="(deal, i) in deals" :key="i" class="board">
      <header class="board-head">
        <span class="board-no">{{ i + 1 }}</span>
        <span class="board-hcp">
          NS {{ deal.north.hcp + deal.south.hcp }} · EW {{ deal.east.hcp + deal.west.hcp }}
        </span>
      </header>

      <!-- Compass layout: North on top, East/West flanking, South below —
           the arrangement a bridge player reads without thinking. -->
      <div class="compass">
        <div class="seat seat-n"><Hand label="N" :hand="deal.north" /></div>
        <div class="seat seat-w"><Hand label="W" :hand="deal.west" /></div>
        <div class="seat seat-e"><Hand label="E" :hand="deal.east" /></div>
        <div class="seat seat-s"><Hand label="S" :hand="deal.south" /></div>
      </div>
    </article>
  </div>
</template>

<script setup>
// A grid of deals laid out as bridge hands rather than one-line strings.
//
// Card primitives are vendored from Bridge-Classroom (lib/cardFormatting.js);
// its HandDisplay.vue was not, being built for an interactive table.
import { h } from 'vue'
import { SUIT_ORDER, SUIT_SYMBOLS, RED_SUITS } from '@/lib/cardFormatting.js'

defineProps({
  deals: { type: Array, required: true },
})

// Small enough to be a render function: four rows of symbol + ranks, plus the
// seat label and HCP. A separate SFC would be more indirection than it saves.
const Hand = (props) =>
  h('div', { class: 'hand' }, [
    h('div', { class: 'hand-head' }, [
      h('span', { class: 'hand-label' }, props.label),
      h('span', { class: 'hand-hcp' }, `${props.hand.hcp}`),
    ]),
    ...SUIT_ORDER.map((suit) =>
      h('div', { class: ['suit', RED_SUITS.has(suit) ? 'is-red' : 'is-black'] }, [
        h('span', { class: 'suit-sym' }, SUIT_SYMBOLS[suit]),
        // An en dash reads as "void" at a glance; an empty cell reads as a bug.
        h('span', { class: 'suit-cards' }, props.hand[suit].length ? props.hand[suit].join('') : '—'),
      ]),
    ),
  ])
Hand.props = ['label', 'hand']
</script>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 10px;
}

.board { border: 1px solid var(--line); border-radius: 4px; padding: 6px 8px 8px; }
.board-head {
  display: flex; justify-content: space-between; align-items: baseline;
  font-size: 11px; color: var(--fg-muted); margin-bottom: 4px;
}
.board-no { font-weight: 600; }
.board-hcp { font-family: var(--mono); }

.compass {
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-areas: 'n n' 'w e' 's s';
  gap: 4px 8px;
}
.seat-n { grid-area: n; justify-self: center; }
.seat-w { grid-area: w; }
.seat-e { grid-area: e; }
.seat-s { grid-area: s; justify-self: center; }

:deep(.hand) { min-width: 96px; }
:deep(.hand-head) {
  display: flex; gap: 6px; align-items: baseline;
  font-size: 10px; color: var(--fg-muted);
}
:deep(.hand-label) { font-weight: 700; }
:deep(.hand-hcp) { font-family: var(--mono); }
:deep(.suit) { display: flex; gap: 4px; font-family: var(--mono); font-size: 12px; line-height: 1.35; }
:deep(.suit-sym) { width: 0.9em; }
:deep(.suit.is-red .suit-sym) { color: #c0392b; }
:deep(.suit.is-black .suit-sym) { color: var(--fg); }
:deep(.suit-cards) { letter-spacing: 0.04em; }
</style>
