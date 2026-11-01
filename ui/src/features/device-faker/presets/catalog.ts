import catalog from "./devices.json"

export const presetFields = [
  "manufacturer",
  "brand",
  "model",
  "marketname",
  "device",
  "product",
  "name",
  "build_id",
  "fingerprint",
] as const
export type PresetField = (typeof presetFields)[number]
export interface DevicePreset {
  readonly id: string
  readonly label: string
  readonly fields: Readonly<Partial<Record<PresetField, string>>>
}
export const presetSource = Object.freeze(catalog.source)
export const devicePresets: readonly DevicePreset[] = catalog.devices
const normalize = (value: string) => value.normalize("NFKC").toLowerCase().trim()
export const presetBrands = [
  ...new Map(
    devicePresets.map(({ fields }) => [normalize(fields.brand ?? ""), fields.brand ?? ""]),
  ).entries(),
].sort(([left], [right]) => left.localeCompare(right))

export function findPresets(query: string, brand = ""): readonly DevicePreset[] {
  const tokens = normalize(query).split(/\s+/).filter(Boolean)
  return devicePresets.filter((preset) => {
    if (brand && normalize(preset.fields.brand ?? "") !== brand) return false
    const text = normalize([preset.label, ...Object.values(preset.fields)].join(" "))
    return tokens.every((token) => text.includes(token))
  })
}

/** Replacing a preset clears fields absent from the next source record. */
export function fieldsFromPreset(preset?: DevicePreset): Record<PresetField, string> {
  return Object.fromEntries(presetFields.map((key) => [key, preset?.fields[key] ?? ""])) as Record<
    PresetField,
    string
  >
}

export function fingerprintConflicts(preset: DevicePreset): PresetField[] {
  const parts = preset.fields.fingerprint?.match(
    /^([^/]+)\/([^/]+)\/([^:]+):[^/]+\/([^/]+)\/[^:]+:[^/]+\/[^/]+$/,
  )
  if (!parts) return []
  const expected = {
    brand: parts[1],
    product: parts[2],
    name: parts[2],
    device: parts[3],
    build_id: parts[4],
  }
  return (Object.keys(expected) as (keyof typeof expected)[]).filter(
    (key) => preset.fields[key] && preset.fields[key] !== expected[key],
  )
}
