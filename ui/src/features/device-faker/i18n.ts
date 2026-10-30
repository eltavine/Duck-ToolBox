import { useFeatureI18n } from "@/core/i18n"
import type messages from "./locales/en.json"
export const useFakerI18n = () => useFeatureI18n<typeof messages>("deviceFaker")
