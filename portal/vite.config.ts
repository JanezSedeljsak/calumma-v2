import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Project Pages serve from /<repo>/, so every asset URL has to carry that prefix.
export default defineConfig({
  plugins: [react()],
  base: "/calumma-v2/",
});
