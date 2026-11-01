import { describe, expect, it } from "vitest"
import {
  devicePresets,
  fieldsFromPreset,
  findPresets,
  fingerprintConflicts,
  presetBrands,
  presetFields,
  presetSource,
} from "./catalog"

describe("embedded community device presets", () => {
  it("keeps source identity and only supplied, nonempty supported fields", () => {
    expect(presetSource.contributor).toBe("SkyQiQiQwQ")
    expect(presetSource.sha256).toBe(
      "3217f4e6096e9fd0ca2891d31b3a58a1ddcc1aeca38902c7cf596422af370586",
    )
    expect(devicePresets).toHaveLength(207)
    expect(new Set(devicePresets.map(({ id }) => id)).size).toBe(207)
    expect(devicePresets.filter(({ fields }) => fields.fingerprint)).toHaveLength(16)
    for (const preset of devicePresets) {
      expect(preset.label).toBeTruthy()
      for (const [key, value] of Object.entries(preset.fields)) {
        expect(presetFields).toContain(key)
        expect(value.trim()).toBe(value)
        expect(value).not.toBe("")
        expect(value).not.toMatch(/[\r\n\u0000]|^__(?:EMPTY|DELETE)__$/)
      }
    }
  })
  it("searches names, model codes and codenames with case and width normalization", () => {
    expect(findPresets(" ｔｂ３２３ｆｕ ").map(({ label }) => label)).toEqual(["Lenovo Y700 五代"])
    expect(findPresets("Y700 四代")[0]?.fields.model).toBe("TB322FC")
    expect(findPresets("popsicle")[0]?.fields.model).toBe("2509FPN0BC")
    expect(findPresets("not-a-real-device")).toEqual([])
    const oppo = findPresets("", "oppo")
    expect(oppo).toHaveLength(26)
    expect(new Set(oppo.map(({ fields }) => fields.brand))).toEqual(new Set(["oppo", "OPPO"]))
    expect(presetBrands.filter(([id]) => id === "oppo")).toHaveLength(1)
  })
  it("clears previous fields when a source record is incomplete without inventing values", () => {
    const complete = fieldsFromPreset(findPresets("TB323FU")[0])
    expect(complete.fingerprint).toContain("02.083.251119C")
    expect(complete.manufacturer).toBe("LENOVO")
    const partial = fieldsFromPreset(findPresets("PMA110")[0])
    expect(partial).toMatchObject({
      model: "PMA110",
      fingerprint: "",
      build_id: "",
      device: "",
      product: "",
      name: "",
    })
    expect(fieldsFromPreset()).toEqual(Object.fromEntries(presetFields.map((key) => [key, ""])))
  })
  it("reports fingerprint disagreements while retaining both supplied values", () => {
    const preset = findPresets("TB322FC")[0]!
    expect(fingerprintConflicts(preset)).toEqual(["product", "name"])
    expect(fieldsFromPreset(preset).product).toBe("TB322FC")
    expect(fieldsFromPreset(preset).fingerprint).toContain("/TB322FC_PRC/")
    expect(fingerprintConflicts(findPresets("PMA110")[0]!)).toEqual([])
  })
})
