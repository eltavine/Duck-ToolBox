import { Cpu } from "@lucide/vue"

import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"

import { NAMESPACE } from "./i18n"

export default defineFeature({
  id: "rkp-device",
  contract: 2,
  namespace: NAMESPACE,
  icon: Cpu,
  order: 25,
  // Opening the tool warns once that it only works on some devices and only issues RKP
  // certificates; the page asks again before the first request is sent.
  confirmOpen: true,
  routes: [{ path: "", name: "rkp-device", component: () => import("./pages/RkpDevicePage.vue") }],
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
