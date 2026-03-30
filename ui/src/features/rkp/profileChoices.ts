import type { UiMode } from "./types"

export interface ChoiceOption<T extends string | number> {
  value: T
  labelKey: string
  descriptionKey: string
}

export const keySourceOptions: ChoiceOption<UiMode>[] = [
  {
    value: "hw-key",
    labelKey: "choices.modeHwKeyLabel",
    descriptionKey: "choices.modeHwKeyDescription",
  },
  {
    value: "seed",
    labelKey: "choices.modeSeedLabel",
    descriptionKey: "choices.modeSeedDescription",
  },
]

export const securityLevelOptions: ChoiceOption<string>[] = [
  {
    value: "tee",
    labelKey: "choices.securityTeeLabel",
    descriptionKey: "choices.securityTeeDescription",
  },
  {
    value: "strongbox",
    labelKey: "choices.securityStrongBoxLabel",
    descriptionKey: "choices.securityStrongBoxDescription",
  },
]

export const verifiedBootOptions: ChoiceOption<string>[] = [
  {
    value: "green",
    labelKey: "choices.vbGreenLabel",
    descriptionKey: "choices.vbGreenDescription",
  },
  {
    value: "yellow",
    labelKey: "choices.vbYellowLabel",
    descriptionKey: "choices.vbYellowDescription",
  },
  {
    value: "orange",
    labelKey: "choices.vbOrangeLabel",
    descriptionKey: "choices.vbOrangeDescription",
  },
]

export const bootloaderStateOptions: ChoiceOption<string>[] = [
  {
    value: "locked",
    labelKey: "choices.bootLockedLabel",
    descriptionKey: "choices.bootLockedDescription",
  },
  {
    value: "unlocked",
    labelKey: "choices.bootUnlockedLabel",
    descriptionKey: "choices.bootUnlockedDescription",
  },
]

export const numKeysOptions: ChoiceOption<number>[] = [
  {
    value: 1,
    labelKey: "choices.numKeysOneLabel",
    descriptionKey: "choices.numKeysOneDescription",
  },
  {
    value: 2,
    labelKey: "choices.numKeysTwoLabel",
    descriptionKey: "choices.numKeysTwoDescription",
  },
  {
    value: 3,
    labelKey: "choices.numKeysThreeLabel",
    descriptionKey: "choices.numKeysThreeDescription",
  },
  {
    value: 4,
    labelKey: "choices.numKeysFourLabel",
    descriptionKey: "choices.numKeysFourDescription",
  },
]

export function choiceLabel<T extends string | number>(
  options: ChoiceOption<T>[],
  value: T,
  translate: (key: string) => string,
) {
  const match = options.find((option) => option.value === value)
  return match ? translate(match.labelKey) : String(value)
}

export function choiceDescription<T extends string | number>(
  options: ChoiceOption<T>[],
  value: T,
  translate: (key: string) => string,
) {
  const match = options.find((option) => option.value === value)
  return match ? translate(match.descriptionKey) : ""
}
