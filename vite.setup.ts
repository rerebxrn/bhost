// BHOST Setup (setup/) : seulement la page de l'installeur, dans dist-setup.
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "dist-setup",
    emptyOutDir: true,
    rollupOptions: { input: "installeur.html" },
  },
});
