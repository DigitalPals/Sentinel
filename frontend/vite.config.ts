import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// During `bun run dev` the API is proxied to the Rust backend.
// `bun run build` emits a static bundle the backend serves itself.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": "http://localhost:8787",
    },
  },
  build: {
    outDir: "dist",
    chunkSizeWarningLimit: 1200,
  },
});
