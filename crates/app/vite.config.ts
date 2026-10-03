import { defineConfig } from "vite";

// Фиксированный порт и strictPort — чтобы Tauri dev не «уплывал» на другой порт
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: ["es2021", "chrome100", "safari13"],
    minify: "esbuild",
    sourcemap: false,
  },
});
