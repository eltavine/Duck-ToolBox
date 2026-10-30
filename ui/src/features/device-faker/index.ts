import { Smartphone } from "@lucide/vue"
import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"

export default defineFeature({
  id: "device-faker",
  contract: 2,
  namespace: "deviceFaker",
  icon: Smartphone,
  order: 18,
  routes: [{ path: "", component: () => import("./pages/DeviceFakerPage.vue") }],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
