<script setup>
/**
 * A deal's double-dummy table: tricks available to each seat in each strain,
 * with both sides playing perfectly.
 *
 * VENDORED from bridge-solver's `DoubleDummyTable.vue`, which vendored it from
 * Bridge-Classroom. Two things came with it and are the reason to share rather
 * than rewrite. Rows read `N, S, E, W` — partners adjacent, which is how a
 * double-dummy table is always drawn, and the ordering the merge depends on.
 * And identical partnership rows merge into one `NS` / `EW` row, which is
 * lossless (a pair only merges when every cell already matches) and is the
 * common case, since the two hands of a partnership usually take the same
 * tricks.
 *
 * The **columns** did not come with it. Both of those apps draw
 * `♣ ♦ ♥ ♠ NT`, which came from the BBO helper extension; this draws the
 * standard `NT ♠ ♥ ♦ ♣`, the order PBN's own encodings use. See
 * `lib/ddTable.js`.
 *
 * What did not come with it, because dealer3 deals hands rather than plays
 * them: the contract highlight, and the `compact`, `rotated`, `par` and
 * `diverged` props. A generated deal has no auction, no declarer and no bidding
 * engine to disagree with. See `lib/ddTable.js` for the rest of the divergence.
 *
 * Drawn small on purpose. This sits inside a board in a grid of forty of them,
 * where the hands are the point and the table is a footnote to them.
 *
 * Draws nothing at all unless the deal's table is complete (#129) — see
 * `lib/ddTable.js` for why half a table is not worth drawing and not worth
 * solving for.
 */
import { computed } from 'vue'
import { ddRows, DISPLAY_STRAINS } from '@/lib/ddTable.js'
import { SUIT_SYMBOLS, RED_SUITS } from '@/lib/cardFormatting.js'

const props = defineProps({
  /** The engine's `tricks[seat][strain]`, rows `N,E,S,W`, columns `C,D,H,S,NT`. */
  tricks: { type: Array, default: null },
})

/** The strain a display column stands for, as `cardFormatting` names its suits. */
const SUIT_OF_STRAIN = { C: 'clubs', D: 'diamonds', H: 'hearts', S: 'spades' }

const SEAT_LABELS = {
  N: 'North',
  S: 'South',
  E: 'East',
  W: 'West',
  NS: 'North and South',
  EW: 'East and West',
}

const STRAIN_NAMES = {
  C: 'clubs',
  D: 'diamonds',
  H: 'hearts',
  S: 'spades',
  NT: 'notrump',
}

const columns = computed(() =>
  DISPLAY_STRAINS.map((strain) => ({
    strain,
    label: strain === 'NT' ? 'NT' : SUIT_SYMBOLS[SUIT_OF_STRAIN[strain]],
    red: RED_SUITS.has(SUIT_OF_STRAIN[strain]),
  })),
)

const rows = computed(() => ddRows(props.tricks))

const TABLE_TITLE =
  'Tricks available to each seat in each strain, with both sides playing perfectly.'

function cellTitle(seat, strain, tricks) {
  const who = SEAT_LABELS[seat] || seat
  const takes = seat.length > 1 ? 'take' : 'takes'
  return `${who} ${takes} ${tricks} tricks in ${STRAIN_NAMES[strain]}`
}
</script>

<template>
  <table v-if="rows" class="dd" :title="TABLE_TITLE">
    <caption class="dd-sr">
      Double-dummy tricks. A partnership shows as one row when both hands take
      the same tricks.
    </caption>
    <thead>
      <tr>
        <th scope="col" class="dd-corner"><span class="dd-sr">Seat</span></th>
        <th
          v-for="col in columns"
          :key="col.strain"
          scope="col"
          :class="{ 'dd-red': col.red }"
        >
          {{ col.label }}
        </th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="row in rows" :key="row.seat">
        <th scope="row" class="dd-seat">{{ row.seat }}</th>
        <td
          v-for="(cell, i) in row.cells"
          :key="i"
          :title="cellTitle(row.seat, columns[i].strain, cell)"
        >
          {{ cell }}
        </td>
      </tr>
    </tbody>
  </table>
</template>

<style scoped>
.dd {
  border-collapse: collapse;
  margin-top: 6px;
  font-family: var(--mono);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  line-height: 1.3;
}

.dd th,
.dd td {
  border: 1px solid var(--line);
  padding: 1px 4px;
  text-align: center;
  min-width: 2ch;
}

.dd th {
  background: var(--bg-subtle);
  color: var(--fg-muted);
  font-weight: 600;
}

.dd-red {
  color: #c0392b;
}

.dd-seat {
  color: var(--fg);
  /* Room for a merged `NS` without the column moving when it collapses. */
  min-width: 3ch;
}

/* The corner cell heads both axes and names neither, so it is drawn as the
   frame rather than as a heading. */
.dd-corner {
  border-top-color: transparent;
  border-left-color: transparent;
  background: transparent;
}


.dd-sr {
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
</style>
