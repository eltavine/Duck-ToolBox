import { ScanLine } from "@lucide/vue"

import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"

import { NAMESPACE } from "./i18n"

export default defineFeature({
  id: "tee-attestation",
  namespace: NAMESPACE,
  icon: ScanLine,
  order: 26,
  routes: [
    { path: "", name: "tee-attestation", component: () => import("./pages/AttestationPage.vue") },
  ],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
