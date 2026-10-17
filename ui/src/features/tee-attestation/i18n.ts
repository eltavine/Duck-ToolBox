import { useFeatureI18n } from "@/core/i18n"

import type messages from "./locales/en.json"

export const NAMESPACE = "teeAttestation"
export const useAttestationI18n = () => useFeatureI18n<typeof messages>(NAMESPACE)
