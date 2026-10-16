import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

// Tests assert on formatted numbers and dates ("1,500 memories"). A worker
// reads its locale from the environment when it starts, so pin it here and
// the suite gives the same result on a machine set to any language.
process.env.LC_ALL = "en_US.UTF-8";
process.env.LANG = "en_US.UTF-8";

export default defineConfig({
    plugins: [react()],
    resolve: {
        alias: {
            "@": resolve(__dirname, "./src"),
        },
    },
    test: {
        environment: "jsdom",
        globals: false,
        setupFiles: ["./src/test/setup.ts"],
        include: ["src/**/*.test.{ts,tsx}"],
    },
});
