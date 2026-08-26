// Entry point for the language reference page.
//
// A second Vite entry rather than a route inside the app: the reference is a
// document, it should be linkable and printable on its own, and it needs none
// of the app's editor or generator code. Sharing the engine chunk is the only
// overlap, and the build already splits that out.

import { createApp } from 'vue'
import Reference from './Reference.vue'

createApp(Reference).mount('#app')
