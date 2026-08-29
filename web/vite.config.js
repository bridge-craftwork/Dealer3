import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

const entry = (name) => fileURLToPath(new URL(name, import.meta.url))

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'))

/// What `SharedArrayBuffer` needs, and so what wasm threads need. Kept in step
/// with `public/_headers`, which is what production serves.
const ISOLATION = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
}

export default defineConfig({
  plugins: [vue()],

  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },

  // Relative asset URLs, so the build works from a subpath as well as a root.
  base: './',

  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },

  build: {
    outDir: 'dist',
    rollupOptions: {
      // Three pages: the app, a language reference generated from the engine's
      // own vocabulary, and the levelling guide. The reference imports the
      // engine, so it has to be a bundler entry rather than a static file
      // copied past it; the guide inlines `docs/leveling-guide.md` with `?raw`,
      // which likewise only happens for something the bundler owns.
      input: {
        main: entry('index.html'),
        reference: entry('reference.html'),
        leveling: entry('leveling.html'),
      },
      output: {
        // Keep the engine and the editor in their own chunks. Both are large,
        // both are content-hashed, and neither changes when app code does — so
        // a redeploy does not force everyone to re-download them.
        manualChunks(id) {
          // The engine is the one genuinely large, rarely-changing asset. Its
          // own content-hashed chunk means an app-code deploy does not force
          // everyone to re-download a megabyte of wasm.
          if (id.includes('/wasm/')) return 'engine'
          if (id.includes('@codemirror') || id.includes('@lezer')) return 'editor'
        },
      },
    },
    // The engine is ~1 MB on its own; the default 500 kB warning is noise.
    chunkSizeWarningLimit: 2000,
    target: 'es2022',
  },

  worker: { format: 'es' },

  // A threaded engine needs `SharedArrayBuffer`, which a browser only exposes
  // to a cross-origin isolated page. In production Cloudflare sends these from
  // `public/_headers`; dev and preview need them here or the pool silently
  // fails to start and every run falls back to one thread — the same deals,
  // several times slower, with nothing on screen to say why.
  server: { headers: ISOLATION },
  preview: { headers: ISOLATION },

  // `wasm-pack --target web` loads the binary with
  // `new URL('..._bg.wasm', import.meta.url)`. Excluding it from dep
  // optimisation keeps that URL intact in dev.
  optimizeDeps: {
    exclude: ['@/wasm/dealer3_wasm.js'],
  },

  test: {
    environment: 'node',
    include: ['src/**/*.test.js'],
  },
})
