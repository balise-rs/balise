import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import balise from "@balise-rs/react/vite";

// `balise` sets the `@/` alias, the proxy to the Rust server, and browser error reporting in dev.
export default defineConfig({
  plugins: [react(), balise()],
});
