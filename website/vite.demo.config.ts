import { defineConfig } from "vite";
import { resolve } from "path";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "./",
  plugins: [react()],
  resolve: { alias: { "@": resolve(__dirname, "../src") } },
  build: {
    rollupOptions: { input: resolve(__dirname, "../demo-preview.html") },
  },
});
