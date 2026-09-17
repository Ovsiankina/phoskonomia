import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Phoskonomia web frontend — Vite + React.
// No CDN scripts, no in-browser Babel: all deps are bundled and pinned.
export default defineConfig({
  plugins: [react()],
  // Fixed, uncommon port. 3000–3002 are commonly occupied; `strictPort` makes
  // Vite FAIL LOUDLY if 3717 is taken rather than silently hopping to another
  // port — that silent hop is what left `localhost:3001` hitting a stale 404.
  server: { port: 3717, strictPort: true },
})
