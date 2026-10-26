import { Cpu } from "@lucide/vue"

import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"

import { NAMESPACE } from "./i18n"

export default defineFeature({
  id: "kernel-modules",
  namespace: NAMESPACE,
  contract: 3,
  icon: Cpu,
  order: 60,
  routes: [
    { path: "", name: "kernel-modules", component: () => import("./pages/KernelModulesPage.vue") },
  ],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
