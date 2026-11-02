import { readFileSync } from "node:fs"
import { expect, test } from "./fixtures"

function messages(locale: string) {
  return JSON.parse(
    readFileSync(
      new URL(`../src/features/device-faker/locales/${locale}.json`, import.meta.url),
      "utf8",
    ),
  ) as {
    addProfile: string
    packages: string
    append: string
    fields: Record<string, string>
    presets: Record<string, string>
  }
}

test("loads exact preset fields, clears absent values and keeps targets and the saved state", async ({
  page,
  open,
}) => {
  const t = messages("en")
  await open("/device-faker")
  const draft = page.locator("#faker-config")
  const saved = await draft.inputValue()
  await page.context().setOffline(true)
  await page.getByText(t.addProfile, { exact: true }).click()
  await page.getByLabel(t.packages, { exact: true }).fill("com.example.preset@999")
  await page.getByText(t.presets.title!, { exact: true }).click()
  const search = page.getByRole("searchbox", { name: t.presets.search, exact: true })
  await search.fill("TB323FU")
  await page.getByRole("button", { name: /Lenovo Y700 五代/ }).click()
  await page.getByRole("button", { name: t.presets.use, exact: true }).click()
  await expect(page.getByLabel(t.fields.manufacturer!, { exact: true })).toHaveValue("LENOVO")
  await expect(page.getByLabel(t.fields.fingerprint!, { exact: true })).toHaveValue(
    /02.083.251119C/,
  )
  await expect(draft).toHaveValue(saved)
  await page.getByText(t.presets.title!, { exact: true }).click()
  await search.fill("PMA110")
  await page.getByRole("button", { name: /OPPO Find X9 Ultra/ }).click()
  await page.getByRole("button", { name: t.presets.use, exact: true }).click()
  await expect(page.getByLabel(t.fields.fingerprint!, { exact: true })).toHaveValue("")
  await expect(page.getByLabel(t.fields.device!, { exact: true })).toHaveValue("")
  await expect(page.getByLabel(t.packages, { exact: true })).toHaveValue("com.example.preset@999")
  await page.getByRole("button", { name: t.append, exact: true }).click()
  await expect(draft).toHaveValue(/model = "PMA110"/)
  await expect(draft).not.toHaveValue(/fingerprint =|build_id =/)
  await page.getByRole("button", { name: "Validate and preview", exact: true }).click()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toBeVisible()
  await expect(
    page.getByRole("status").filter({ hasText: "Disabled for new app processes" }),
  ).toBeVisible()
})

test("filters brands, recovers empty search and reveals source disagreements", async ({
  page,
  open,
}) => {
  const t = messages("en")
  await open("/device-faker")
  await page.getByText(t.addProfile, { exact: true }).click()
  await page.getByText(t.presets.title!, { exact: true }).click()
  const search = page.getByRole("searchbox", { name: t.presets.search, exact: true })
  const results = page.getByRole("list", { name: t.presets.resultsLabel })
  await expect(results.getByRole("button")).toHaveCount(8)
  await page.getByRole("button", { name: t.presets.showMore, exact: true }).click()
  await expect(results.getByRole("button")).toHaveCount(16)
  await search.fill("no-such-model")
  await expect(page.getByText(t.presets.noResults!, { exact: true })).toBeVisible()
  await search.fill("TB322FC")
  await page.getByRole("button", { name: /Lenovo Y700 四代/ }).click()
  await expect(page.getByRole("status").filter({ hasText: /Source fields differ/ })).toBeVisible()
  await page.getByText(t.presets.details!, { exact: true }).click()
  await expect(page.getByText("TB322FC", { exact: true }).first()).toBeVisible()
  await search.fill("")
  await expect(page.getByRole("region", { name: t.presets.selected })).toHaveCount(0)
  await page.getByRole("combobox", { name: t.presets.brand, exact: true }).click()
  await page.getByRole("option", { name: "OPPO", exact: true }).click()
  await expect(
    page.getByRole("status").filter({ hasText: t.presets.results!.replace("{count}", "26") }),
  ).toBeVisible()
  await expect(page.getByRole("list", { name: t.presets.resultsLabel })).not.toContainText("Lenovo")
})

for (const locale of ["en", "zh-CN", "ar", "de"]) {
  for (const theme of ["light", "dark"] as const) {
    test(`presets stay aligned and keyboard accessible in ${locale} ${theme}`, async ({
      page,
      open,
    }, testInfo) => {
      const t = messages(locale)
      await page.setViewportSize({ width: 320, height: 900 })
      await open("/device-faker", { locale, theme })
      await page.evaluate(() => {
        document.documentElement.style.fontSize = "200%"
      })
      await page.getByText(t.addProfile, { exact: true }).click()
      await page.getByText(t.presets.title!, { exact: true }).click()
      const search = page.getByRole("searchbox", { name: t.presets.search, exact: true })
      await search.fill("TB323FU")
      const result = page.getByRole("button", { name: /Lenovo Y700 五代/ })
      await result.focus()
      await page.keyboard.press("Enter")
      await expect(result).toHaveAttribute("aria-pressed", "true")
      const use = page.getByRole("button", { name: t.presets.use, exact: true })
      await expect(use).toBeVisible()
      expect((await result.boundingBox())!.height).toBeGreaterThanOrEqual(48)
      expect((await result.boundingBox())!.height).toBeLessThanOrEqual(240)
      expect((await use.boundingBox())!.height).toBeGreaterThanOrEqual(48)
      const aligned = await Promise.all([
        search.boundingBox(),
        result.boundingBox(),
        page.getByRole("region", { name: t.presets.selected }).boundingBox(),
      ])
      expect(Math.abs(aligned[0]!.x - aligned[1]!.x)).toBeLessThanOrEqual(1)
      expect(Math.abs(aligned[0]!.x - aligned[2]!.x)).toBeLessThanOrEqual(1)
      await page.getByText(t.presets.details!, { exact: true }).click()
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      )
      await result.screenshot({ path: testInfo.outputPath("preset-result.png") })
      await search.locator("xpath=ancestor::details[1]").screenshot({
        path: `/tmp/duck-presets-${locale}-${theme}.png`,
        animations: "disabled",
      })
      await use.click()
      await expect(page.getByRole("group", { name: t.presets.reviewFields })).toBeFocused()
      await expect(page.getByLabel(t.fields.fingerprint!, { exact: true })).toHaveAttribute(
        "dir",
        "ltr",
      )
      await page.evaluate(() => {
        document.documentElement.style.fontSize = ""
      })
      const wide = locale === "de" || locale === "ar"
      await page.setViewportSize({ width: wide ? 1024 : 412, height: 915 })
      await page.getByText(t.presets.title!, { exact: true }).click()
      if (wide) {
        const filter = page.getByRole("combobox", { name: t.presets.brand, exact: true })
        expect(
          Math.abs((await search.boundingBox())!.y - (await filter.boundingBox())!.y),
        ).toBeLessThanOrEqual(1)
      }
      if (locale === "ar") {
        const values = page.getByRole("region", { name: t.presets.selected }).locator("dd")
        for (const value of await values.all()) {
          expect(await value.evaluate((node) => getComputedStyle(node).textAlign)).toBe("right")
        }
      }
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      )
      await search.locator("xpath=ancestor::details[1]").screenshot({
        path: `/tmp/duck-presets-${locale}-${theme}-normal.png`,
        animations: "disabled",
      })
    })
  }
}
