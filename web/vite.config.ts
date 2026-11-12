import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// In sviluppo (`npm run dev`) le chiamate /api vengono inoltrate al server Rust
// (`fine serve`), così si ha l'hot reload del frontend sui dati veri.
// Cambia la porta con FINE_API=http://127.0.0.1:7899 se serve.
const api = process.env.FINE_API ?? "http://127.0.0.1:7878";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: {
    port: 5173,
    strictPort: false,
    proxy: {
      "/api": { target: api, changeOrigin: false },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    // Served from the local binary, not the network: one bundle is fine.
    chunkSizeWarningLimit: 1500,
    target: "es2022",
    // The hashed file names under dist/assets are what lets the server cache them as immutable.
  },
});
