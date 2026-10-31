import { useFeatureI18n } from "@/core/i18n"
import type messages from "./locales/en.json"
export const NAMESPACE = "misc"
export const useMiscI18n = () => useFeatureI18n<typeof messages>(NAMESPACE)
