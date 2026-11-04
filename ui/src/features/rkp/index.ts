import { KeyRound } from "@lucide/vue"

import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"

import { NAMESPACE } from "./i18n"

export default defineFeature({
  id: "rkp",
  contract: 3,
  namespace: NAMESPACE,
  icon: KeyRound,
  order: 20,
  routes: [{ path: "", name: "rkp", component: () => import("./pages/RkpPage.vue") }],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
