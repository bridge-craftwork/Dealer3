import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'))

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
