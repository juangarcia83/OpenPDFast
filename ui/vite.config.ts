// SPDX-License-Identifier: AGPL-3.0-or-later
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Tauri expects a fixed port and must see Rust-side errors, so don't clear the screen.
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/crates/**", "**/target/**"] },
  },
  // vite-plugin-solid defaults tests to jsdom; unit tests here are DOM-free.
  test: { environment: "node" },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
