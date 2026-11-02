import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// Tauri expects a fixed port and exposes the dev host through an env variable
// when running on a physical device or in a container.
var host = process.env.TAURI_DEV_HOST;
export default defineConfig({
    plugins: [react()],
    // Keep the Rust compiler output visible while Vite is running.
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                protocol: "ws",
                host: host,
                port: 1421,
            }
            : undefined,
        watch: {
            ignored: ["**/src-tauri/**"],
        },
    },
});
