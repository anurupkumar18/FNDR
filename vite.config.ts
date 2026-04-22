import { defineConfig } from "vite";
import { resolve } from "path";
import react from "@vitejs/plugin-react";

export default defineConfig({
    plugins: [react()],
    clearScreen: false,
    envPrefix: ["VITE_", "TAURI_"],
    build: {
        rollupOptions: {
            input: {
                main: resolve(__dirname, "index.html"),
                autofill: resolve(__dirname, "autofill.html"),
            },
        },
    },
    server: {
        port: 1420,
        strictPort: true,
        watch: {
            ignored: ["**/src-tauri/**"],
        },
    },
});
