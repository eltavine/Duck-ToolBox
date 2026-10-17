import { expect, test } from "./fixtures"

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem("duck-toolbox/acknowledged", JSON.stringify(["rkp-device"])),
  )
})

test("identifies QTI and keeps firmware compatibility unverified", async ({ page, open }) => {
  await open("/rkp-device", { locale: "zh-CN" })
  await expect(page.getByText("QTI", { exact: true }).first()).toBeVisible()
  await expect(page.getByText("已识别", { exact: true })).toBeVisible()
  await expect(page.getByText("尚未验证", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "签发 RKP keybox", exact: true })).toBeEnabled()
  await expect(
    page.getByText("识别到了非高通 SoC 厂商；该操作需要高通 KeyMint。", { exact: true }),
  ).toHaveCount(0)
  await page.getByText("识别依据", { exact: true }).click()
  await expect(page.getByText("ro.soc.manufacturer", { exact: true })).toBeVisible()
  await page.screenshot({ path: "test-results/rkp-device-qti-zh.png", fullPage: true })
})

test("shows kernel fallback evidence when the vendor property is absent", async ({
  page,
  open,
}) => {
  await open("/rkp-device", { query: { soc: "fallback" } })
  await expect(page.getByText("Detected", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Issue RKP keybox", exact: true })).toBeEnabled()
  await page.getByText("Detection evidence", { exact: true }).click()
  await expect(page.getByText("/sys/devices/soc0/family", { exact: true })).toBeVisible()
  await expect(page.getByText("Snapdragon", { exact: true })).toBeVisible()
})

for (const [soc, message] of [
  [
    "unknown",
    "The SoC vendor could not be identified from local properties or kernel information.",
  ],
  ["conflict", "Local SoC information conflicts. Review the detection evidence before continuing."],
  ["other", "A non-Qualcomm SoC vendor was identified; this operation requires Qualcomm KeyMint."],
]) {
  test(`distinguishes ${soc} hardware and blocks issuance`, async ({ page, open }) => {
    await open("/rkp-device", { query: { soc: soc! } })
    await expect(page.getByText(message!, { exact: true })).toBeVisible()
    await expect(page.getByRole("button", { name: "Issue RKP keybox", exact: true })).toBeDisabled()
    if (soc !== "other")
      await expect(
        page.getByText(
          "A non-Qualcomm SoC vendor was identified; this operation requires Qualcomm KeyMint.",
          { exact: true },
        ),
      ).toHaveCount(0)
  })
}
