import RecoveryNotice from "./components/RecoveryNotice.vue"
import { ShieldEllipsis } from "@lucide/vue"
import { defineFeature } from "@/core/features"
import { localeFiles } from "@/core/i18n"

export default defineFeature({
  id: "play-integrity",
  namespace: "playIntegrity",
  icon: ShieldEllipsis,
  order: 15,
  contributes: { homeWidgets: [RecoveryNotice] },
  routes: ["", "config", "generate", "archive", "compatibility", "diagnostics", "license"].map(
    (section) => ({
      path: section,
      component: () => import("./pages/IntegrityPage.vue"),
      props: { section: section || "status" },
      meta: { titleKey: `playIntegrity.nav.${section || "status"}`, backTo: "/" },
    }),
  ),
  messages: localeFiles(import.meta.glob("./locales/*.json")),
})
