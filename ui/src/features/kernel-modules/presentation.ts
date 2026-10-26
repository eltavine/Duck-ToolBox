import { computed } from "vue"

import { useKernelModulesI18n } from "./i18n"

export function useModulePresentation() {
  const { t, locale } = useKernelModulesI18n()
  const numbers = computed(() => new Intl.NumberFormat(locale.value))
  function stateLabel(state: string | null) {
    switch (state?.toLowerCase()) {
      case "live":
        return t("runtime.stateLive")
      case "loading":
      case "coming":
        return t("runtime.stateLoading")
      case "unloading":
      case "going":
        return t("runtime.stateUnloading")
      default:
        return state || t("runtime.stateUnknown")
    }
  }
  return { t, stateLabel, number: (value: number) => numbers.value.format(value) }
}
