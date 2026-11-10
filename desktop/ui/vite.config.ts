import { defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  // The browser preview reads the repo's persona packs (../../personas).
  server: { port: 5173, strictPort: true, fs: { allow: ["../.."] } },
  build: { target: "es2021", outDir: "dist", emptyOutDir: true },
  test: { environment: "jsdom", include: ["tests/**/*.test.ts"] },
});
