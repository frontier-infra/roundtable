import { defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2021", outDir: "dist", emptyOutDir: true },
  test: { environment: "jsdom", include: ["tests/**/*.test.ts"] },
});
