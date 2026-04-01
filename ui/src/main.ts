import { createApp } from "vue"
import "@fontsource/ibm-plex-sans/400.css"
import "@fontsource/ibm-plex-sans/500.css"
import "@fontsource/ibm-plex-sans/600.css"
import "@fontsource/ibm-plex-mono/400.css"
import "./style.css"
import App from "./App.vue"
import { initializeTheme } from "./lib/theme"

initializeTheme()

createApp(App).mount("#app")
