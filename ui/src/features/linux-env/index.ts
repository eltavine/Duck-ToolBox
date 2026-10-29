import { Terminal } from "@lucide/vue"
import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"
import { NAMESPACE } from "./i18n"
export default defineFeature({
  id: "linux-env",
  namespace: NAMESPACE,
  contract: 1,
  icon: Terminal,
  order: 62,
  routes: [{ path: "", name: "linux-env", component: () => import("./pages/LinuxPage.vue") }],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
