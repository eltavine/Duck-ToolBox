import { expect, test } from "./fixtures"

test("applies only after confirmation, restores original parameters and compares memory", async ({
  page,
  open,
}) => {
  await open("/zram")
  await expect(page.getByText("Actual memory allocated", { exact: true })).toBeVisible()
  const size = page.getByRole("textbox", { name: "Capacity (MiB)", exact: true })
  await size.fill("2048")
  await page.getByRole("button", { name: "Apply now", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toBeVisible()
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(size).toHaveValue("2048")
  await page.getByRole("button", { name: "Apply now", exact: true }).click()
  await dialog.getByRole("button", { name: "Apply now", exact: true }).click()
  await expect(page.getByText("Memory after reset", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Restore original", exact: true })).toBeEnabled()
  await page.getByRole("button", { name: "Restore original", exact: true }).click()
  await dialog.getByRole("button", { name: "Restore original", exact: true }).click()
  await expect(page.getByRole("alertdialog")).toHaveCount(0)
  await expect(page.getByTestId("zram-page")).toBeFocused()
})
test("saves boot application separately from runtime changes and can remove it", async ({
  page,
  open,
}) => {
  await open("/zram")
  await page.getByRole("switch", { name: "Apply at boot", exact: true }).click()
  await page.getByRole("button", { name: "Save plan", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText("Saved: boot application enabled", { exact: true })).toBeVisible()
  await dialog.getByRole("button", { name: "Save plan", exact: true }).click()
  await expect(page.getByText("Saved: boot application enabled", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Restore original", exact: true })).toBeDisabled()
  await page.getByRole("button", { name: "Remove saved plan", exact: true }).click()
  await expect(page.getByRole("button", { name: "Remove saved plan", exact: true })).toHaveCount(0)
})
for (const scenario of ["mmd", "writeback", "unsupported"]) {
  test(`blocks mutations for ${scenario} while retaining statistics`, async ({ page, open }) => {
    await open("/zram", { query: { zram: scenario } })
    await expect(page.getByText("Actual memory allocated", { exact: true })).toBeVisible()
    await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeDisabled()
    await expect(page.getByRole("button", { name: "Save plan", exact: true })).toBeDisabled()
  })
}
for (const scenario of ["reject", "timeout"]) {
  test(`keeps ${scenario} failure visible and retains the draft`, async ({ page, open }) => {
    await open("/zram", { query: { zram: scenario } })
    await page.getByRole("textbox", { name: "Capacity (MiB)", exact: true }).fill("1024")
    await page.getByRole("button", { name: "Apply now", exact: true }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Apply now", exact: true })
      .click()
    await expect(page.getByRole("alert")).toContainText(
      scenario === "timeout" ? "Timed out; outcome unknown" : "Available memory is insufficient",
    )
    await expect(page.getByRole("textbox", { name: "Capacity (MiB)", exact: true })).toHaveValue(
      "1024",
    )
  })
}
test("validates capacity and automatic priority before confirming", async ({ page, open }) => {
  await open("/zram")
  const size = page.getByRole("textbox", { name: "Capacity (MiB)", exact: true })
  await size.fill("99999")
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeDisabled()
  await expect(size).toHaveAttribute("aria-invalid", "true")
  await size.fill("0")
  await page.getByRole("textbox", { name: "Swap priority", exact: true }).fill("-1")
  await expect(page.getByRole("button", { name: "Apply now", exact: true })).toBeEnabled()
})
for (const [locale, theme, label] of [
  ["en", "light", "Apply now"],
  ["zh-CN", "dark", "立即应用"],
  ["ar", "light", "تطبيق الآن"],
] as const) {
  test(`aligns ${locale} ${theme} at 320px and 200% text`, async ({ page, open }) => {
    await page.setViewportSize({ width: 320, height: 568 })
    await open("/zram", { locale, theme })
    await page.screenshot({
      path: `test-results/screenshots/zram-${locale}-${theme}-normal.png`,
      fullPage: true,
      animations: "disabled",
    })
    await page.addStyleTag({ content: "html { font-size: 200%; }" })
    await expect(page.getByRole("button", { name: label, exact: true })).toBeVisible()
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await page.evaluate(() => scrollTo(0, 0))
    await page.screenshot({
      path: `test-results/screenshots/zram-${locale}-${theme}.png`,
      fullPage: true,
      animations: "disabled",
    })
    await page.getByRole("button", { name: label, exact: true }).click()
    const dialog = page.getByRole("alertdialog")
    await expect(dialog).toBeVisible()
    await expect(dialog.getByRole("button", { name: label, exact: true })).toBeVisible()
    await dialog.getByRole("button", { name: label, exact: true }).scrollIntoViewIfNeeded()
    await expect(dialog.getByRole("button", { name: label, exact: true })).toBeInViewport()
    await page.screenshot({
      path: `test-results/screenshots/zram-${locale}-confirm.png`,
      animations: "disabled",
    })
    expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true)
  })
}
