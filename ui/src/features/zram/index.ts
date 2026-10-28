import { MemoryStick } from "@lucide/vue"
import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"
import { NAMESPACE } from "./i18n"
export default defineFeature({
  id: "zram",
  namespace: NAMESPACE,
  contract: 3,
  icon: MemoryStick,
  order: 61,
  routes: [{ path: "", name: "zram", component: () => import("./pages/ZramPage.vue") }],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
