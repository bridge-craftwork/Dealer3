// Entry point for the levelling guide page.
//
// A third Vite entry alongside the app and the reference. Like the reference it
// is a document — linkable and printable on its own — but unlike it, it needs
// no engine at all: the text is inlined from `docs/leveling-guide.md` at build
// time, so this page loads nothing.

import { createApp } from 'vue'
import Leveling from './Leveling.vue'

createApp(Leveling).mount('#app')
