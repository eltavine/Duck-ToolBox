import { expect, test } from "./fixtures"

for (const feature of ["play-integrity", "device-faker"]) {
  test(`${feature} opens the matching repository source instead of a bundled archive`, async ({
    page,
    open,
  }) => {
    await page.addInitScript(() => {
      window.open = (url) => {
        document.documentElement.dataset.openedSource = String(url)
        return null
      }
    })
    await open(`/${feature}`)
    if (feature === "device-faker") await page.locator("summary").last().click()
    const link = page.getByRole("link", { name: "Corresponding source", exact: true })
    await expect(link).toHaveAttribute(
      "href",
      new RegExp(
        `^https://github\\.com/eltavine/Duck-ToolBox/tree/(main|[a-f0-9]{40})/components/${feature}$`,
      ),
    )
    expect(await link.getAttribute("download")).toBeNull()
    const href = await link.getAttribute("href")
    await link.click()
    await expect
      .poll(() => page.evaluate(() => document.documentElement.dataset.openedSource))
      .toBe(href)
    await expect(page).toHaveURL(new RegExp(`#/${feature}`))
  })
}
