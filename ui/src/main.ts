import { createApp } from "vue"
import { enableEdgeToEdge } from "kernelsu"
import "@fontsource/ibm-plex-sans/400.css"
import "@fontsource/ibm-plex-sans/500.css"
import "@fontsource/ibm-plex-sans/600.css"
import "@fontsource/ibm-plex-mono/400.css"
import "./style.css"
import App from "./App.vue"
import { initializeTheme } from "./lib/theme"

try {
  enableEdgeToEdge?.(true)
} catch {
  // Ignore when KernelSU WebUI APIs are unavailable.
}

initializeTheme()

createApp(App).mount("#app")
