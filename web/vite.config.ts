import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [react(), tailwindcss(), viteSingleFile()],
  // dist/.gitkeep holds the directory in a fresh checkout, for build.rs
  // to watch; emptying the directory would delete it every build.
  build: { emptyOutDir: false },
});
