import { expect, test } from "./fixtures"

test("browses modules and parameters and refuses busy module removal", async ({ page, open }) => {
  await open("/kernel-modules")
  await expect(page.getByText("6.6.30-android15", { exact: true })).toBeVisible()
  await expect(
    page.locator('[data-module="cfg80211"]').getByRole("button", { name: /^Unload / }),
  ).toBeDisabled()
  await expect(
    page.locator('[data-module="binder"]').getByRole("button", { name: /^Unload / }),
  ).toHaveCount(0)
  await page.locator('[data-module="usb_f_mass_storage"]').getByRole("button").first().click()
  await expect(page.getByRole("dialog")).toBeVisible()
  await expect(page.getByText("Parameters (read only)", { exact: true })).toBeVisible()
  await expect(page.getByText("debug", { exact: true })).toBeVisible()
  await page.keyboard.press("Escape")
  await page.getByRole("textbox", { name: "Search modules", exact: true }).fill("cfg80211")
  await expect(page.locator("[data-module]")).toHaveCount(1)
})

test("confirms removal and refreshes the runtime inventory", async ({ page, open }) => {
  await open("/kernel-modules")
  await page
    .locator('[data-module="usb_f_mass_storage"]')
    .getByRole("button", { name: /^Unload / })
    .click()
  await expect(page.getByRole("alertdialog")).toBeVisible()
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true }),
  ).toBeFocused()
  await expect(page.locator('[data-module="usb_f_mass_storage"]')).toBeVisible()
  await page
    .locator('[data-module="usb_f_mass_storage"]')
    .getByRole("button", { name: /^Unload / })
    .click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: /^(Unload|Load module)$/, exact: true })
    .click()
  await expect(page.locator('[data-module="usb_f_mass_storage"]')).toHaveCount(0)
  await expect(page.getByRole("tab", { name: "Runtime", exact: true })).toBeFocused()
})

test("inspects a module, loads it after confirmation and clears stale inspection", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.getByRole("button", { name: "/vendor/lib/modules/sample.ko", exact: false }).click()
  await expect(page.getByText("Kernel release matches.", { exact: true })).toBeVisible()
  await page
    .getByRole("textbox", { name: "Load parameters (optional)", exact: true })
    .fill("debug=1")
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: /^(Unload|Load module)$/, exact: true })
    .click()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await expect(page.locator('[data-module="sample"]')).toBeVisible()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/other.ko")
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toHaveCount(0)
})

test("keeps file inspection available when kernel operations are disabled", async ({
  page,
  open,
}) => {
  await open("/kernel-modules", { query: { kernelModules: "disabled" } })
  await expect(
    page.getByText("The kernel has disabled module loading and unloading.", { exact: true }),
  ).toBeVisible()
  await expect(
    page.locator('[data-module="usb_f_mass_storage"]').getByRole("button", { name: /^Unload / }),
  ).toBeDisabled()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/mismatch.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await expect(page.getByText("Kernel release does not match.", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
})

test("shows kernel rejection without adding the module", async ({ page, open }) => {
  await open("/kernel-modules", { query: { kernelModules: "reject" } })
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/sample.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: /^(Unload|Load module)$/, exact: true })
    .click()
  await expect(page.getByText("The kernel rejected the operation.", { exact: true })).toBeVisible()
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await expect(page.locator('[data-module="sample"]')).toHaveCount(0)
})

test("treats release differences as advisory and shows the exact load request", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/mismatch.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await expect(page.getByText("Kernel release does not match.", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeEnabled()
  await page
    .getByRole("textbox", { name: "Load parameters (optional)", exact: true })
    .fill("debug=1")
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText("/data/local/tmp/mismatch.ko", { exact: true })).toBeVisible()
  await expect(dialog.getByText("debug=1", { exact: true })).toBeVisible()
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click()
})

test("announces a pending operation and keeps draft parameters across tabs", async ({
  page,
  open,
}) => {
  await open("/kernel-modules", { query: { latency: "200" } })
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/sample.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  const parameters = page.getByRole("textbox", { name: "Load parameters (optional)", exact: true })
  await expect(parameters).toBeVisible()
  await parameters.fill("debug=1")
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await expect(parameters).toHaveValue("debug=1")
  await page.getByRole("button", { name: "Load module", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: /^(Unload|Load module)$/, exact: true })
    .click()
  await expect(page.getByTestId("kernel-operation-pending")).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
  await expect(page.getByTestId("kernel-operation-pending")).toHaveCount(0)
})

test("supports keyboard focus and reachable controls at 200 percent text", async ({
  page,
  open,
}) => {
  await page.setViewportSize({ width: 320, height: 568 })
  await open("/kernel-modules")
  await page.evaluate(() => {
    document.documentElement.style.fontSize = "32px"
  })
  const details = page.getByRole("button", {
    name: "View details of usb_f_mass_storage",
    exact: true,
  })
  await details.focus()
  await page.keyboard.press("Enter")
  await expect(page.getByRole("dialog")).toBeVisible()
  await page.keyboard.press("Escape")
  await expect(details).toBeFocused()
  for (const control of [
    details,
    page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true }),
  ]) {
    const bounds = await control.boundingBox()
    expect(bounds!.width).toBeGreaterThanOrEqual(48)
    expect(bounds!.height).toBeGreaterThanOrEqual(48)
  }
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true)
  const unload = page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true })
  await unload.click()
  const confirm = page.getByRole("alertdialog").getByRole("button", { name: "Unload", exact: true })
  await confirm.scrollIntoViewIfNeeded()
  await expect(confirm).toBeInViewport()
  const bounds = await confirm.boundingBox()
  expect(bounds!.height).toBeGreaterThanOrEqual(48)
  expect(bounds!.x).toBeGreaterThanOrEqual(0)
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(320)
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(unload).toBeFocused()
})

test("explains why an ARM32 module cannot be loaded by the ARM64 manager", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page
    .getByRole("textbox", { name: "Device .ko path", exact: true })
    .fill("/data/local/tmp/arm32.ko")
  await page.getByRole("button", { name: "Inspect file", exact: true }).click()
  await expect(
    page.getByText("Loading requires a little-endian ARM64 ELF64 module.", { exact: true }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
})

for (const locale of ["en", "zh-CN", "ar"] as const) {
  test(`preserves technical direction, alignment and modal bounds in ${locale}`, async ({
    page,
    open,
  }) => {
    await page.setViewportSize({ width: 320, height: 568 })
    await open("/kernel-modules", { locale })
    const row = page.locator('[data-module="usb_f_mass_storage"]')
    await expect(row).toBeVisible()
    await page.evaluate(() => window.scrollTo(0, 0))
    const alignment = await row
      .getByRole("button")
      .first()
      .evaluate((button) => {
        const name = button.querySelector("bdi")!
        const range = document.createRange()
        range.selectNodeContents(name)
        const text = range.getBoundingClientRect()
        const box = button.getBoundingClientRect()
        return {
          direction: getComputedStyle(button).direction,
          left: text.left - box.left,
          right: box.right - text.right,
        }
      })
    expect(locale === "ar" ? alignment.right : alignment.left).toBeLessThan(2)
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-runtime.png`,
      fullPage: true,
      animations: "disabled",
    })
    await page.getByRole("tab").nth(1).click()
    const input = page.locator("#kernel-module-path")
    await input.fill("/data/local/tmp/sample.ko")
    expect(await input.getAttribute("dir")).toBe("ltr")
    const labels = {
      en: { inspect: "Inspect file", load: "Load module" },
      "zh-CN": { inspect: "解析文件", load: "加载模块" },
      ar: { inspect: "فحص الملف", load: "تحميل الوحدة" },
    }[locale]
    await page.getByRole("button", { name: labels.inspect, exact: true }).click()
    const load = page.getByRole("button", { name: labels.load, exact: true })
    await expect(load).toBeVisible()
    const parameters = page.locator("#kernel-module-parameters")
    await parameters.fill("字".repeat(1366))
    await expect(parameters).toHaveAttribute("aria-invalid", "true")
    await expect(page.locator("#kernel-module-parameters-error")).toBeVisible()
    await expect(load).toBeDisabled()
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true)
    await page.evaluate(() => window.scrollTo(0, 0))
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-parameter-error.png`,
      fullPage: true,
      animations: "disabled",
    })
    await parameters.fill("")
    await expect(load).toBeEnabled()
    await page.evaluate(() => window.scrollTo(0, 0))
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-file.png`,
      fullPage: true,
      animations: "disabled",
    })
    await load.click()
    const dialog = page.getByRole("alertdialog")
    await expect(dialog).toBeVisible()
    await dialog.evaluate(async (element) => {
      await Promise.all(
        element.getAnimations().map((animation) => animation.finished.catch(() => undefined)),
      )
    })
    await expect(dialog.getByRole("button", { name: labels.load, exact: true })).toBeVisible()
    await expect(dialog.locator('bdi[dir="ltr"]').first()).toHaveText("sample")
    const bounds = await dialog.boundingBox()
    expect(bounds!.x).toBeGreaterThanOrEqual(0)
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(320)
    expect(bounds!.y).toBeGreaterThanOrEqual(0)
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(568)
    await page.screenshot({
      path: `test-results/screenshots/kernel-review-${locale}-confirm.png`,
      fullPage: false,
      animations: "disabled",
    })
  })
}

for (const theme of ["light", "dark"] as const) {
  test(`destructive labels and confirmation remain readable in ${theme}`, async ({
    page,
    open,
  }) => {
    await open("/kernel-modules", { theme })
    const unload = page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true })
    await unload.click()
    const confirm = page
      .getByRole("alertdialog")
      .getByRole("button", { name: /^(Unload|Load module)$/, exact: true })
    const ratios = await confirm.evaluate((button) => {
      const canvas = document.createElement("canvas")
      canvas.width = canvas.height = 1
      const context = canvas.getContext("2d")!
      const root = getComputedStyle(document.documentElement)
      function luminance(colors: string[]) {
        context.clearRect(0, 0, 1, 1)
        for (const color of colors) {
          context.fillStyle = color
          context.fillRect(0, 0, 1, 1)
        }
        const [r, g, b] = [...context.getImageData(0, 0, 1, 1).data].slice(0, 3).map((v) => {
          const c = v / 255
          return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
        })
        return r! * 0.2126 + g! * 0.7152 + b! * 0.0722
      }
      const ratio = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
      const red = luminance([root.getPropertyValue("--destructive")])
      const style = getComputedStyle(button)
      return {
        text: ratio(red, luminance([root.getPropertyValue("--background")])),
        row: ratio(red, luminance([root.getPropertyValue("--card")])),
        confirm: ratio(
          luminance([style.color]),
          luminance([root.getPropertyValue("--popover"), style.backgroundColor]),
        ),
      }
    })
    for (const [surface, ratio] of Object.entries(ratios))
      expect(ratio, surface).toBeGreaterThanOrEqual(4.5)
  })
}

test("inspects from the keyboard and expands metadata only on demand", async ({ page, open }) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.locator("#kernel-module-path").fill("/data/local/tmp/sample.ko")
  await page.locator("#kernel-module-path").press("Enter")
  await expect(page.getByText("Kernel release matches.", { exact: true })).toBeVisible()
  await expect(page.getByText("Example kernel module", { exact: true })).toHaveCount(0)
  await page.locator("summary").click()
  await expect(page.getByText("Example kernel module", { exact: true })).toBeVisible()
})

test("explains the ARM64 module name limit before loading", async ({ page, open }) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.locator("#kernel-module-path").fill("/data/local/tmp/long-name.ko")
  await page.locator("#kernel-module-path").press("Enter")
  await expect(
    page.getByText("ARM64 module names must fit in 55 bytes.", { exact: true }),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Load module", exact: true })).toBeDisabled()
})

test("protects the root core even with zero references and shows the reason", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  const row = page.locator('[data-module="kernelsu"]')
  await expect(
    row.getByText("Protected root module. Unloading is blocked by this app.", { exact: true }),
  ).toBeVisible()
  await expect(row.getByRole("button", { name: "Unload kernelsu", exact: true })).toBeDisabled()
  await row.getByRole("button", { name: "View details of kernelsu", exact: true }).click()
  await expect(
    page
      .getByRole("dialog")
      .getByText("Protected root module. Unloading is blocked by this app.", { exact: true }),
  ).toBeVisible()
})

test("distinguishes an unreadable holder list from no dependents", async ({ page, open }) => {
  await open("/kernel-modules", { query: { kernelModules: "holders-unavailable" } })
  await page
    .getByRole("button", { name: "View details of usb_f_mass_storage", exact: true })
    .click()
  await expect(
    page.getByText("Holder state is unavailable. Unloading is blocked until it can be read.", {
      exact: true,
    }),
  ).toBeVisible()
})

test("refuses removal when holder state cannot be established", async ({ page, open }) => {
  await open("/kernel-modules", { query: { kernelModules: "holders-unavailable" } })
  await page.getByRole("button", { name: "Unload usb_f_mass_storage", exact: true }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Unload", exact: true }).click()
  await expect(page.getByTestId("kernel-operation-error")).toContainText(
    "Kernel state could not be read.",
  )
  await expect(page.locator('[data-module="usb_f_mass_storage"]')).toBeVisible()
})

for (const [scenario, value] of [
  ["untainted", "No taint flags"],
  ["taint-unavailable", "Unreadable"],
] as const) {
  test(`distinguishes module taint state in ${scenario}`, async ({ page, open }) => {
    await open("/kernel-modules", { query: { kernelModules: scenario } })
    await page
      .getByRole("button", { name: "View details of usb_f_mass_storage", exact: true })
      .click()
    const dialog = page.getByRole("dialog")
    await expect(dialog.getByText("Module taint flags", { exact: true })).toBeVisible()
    await expect(dialog.getByText(value, { exact: true })).toBeVisible()
  })
}

test("keeps inspection errors visible and clears them for a new path", async ({ page, open }) => {
  await open("/kernel-modules", { query: { latency: "600" } })
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  const path = page.locator("#kernel-module-path")
  await path.fill("/data/local/tmp/invalid.ko")
  await path.press("Enter")
  await expect(page.getByTestId("kernel-inspection-pending")).toContainText(
    "Inspecting module file…",
  )
  const error = page.getByTestId("kernel-inspection-error")
  await expect(error).toContainText("Invalid module input.")
  await expect(path).toHaveAttribute("aria-invalid", "true")
  await page.getByRole("tab", { name: "Runtime", exact: true }).click()
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await expect(error).toBeVisible()
  await error.getByRole("button", { name: "Error details", exact: true }).click()
  await expect(page.getByRole("dialog")).toBeVisible()
  await page.keyboard.press("Escape")
  await path.fill("/data/local/tmp/sample.ko")
  await expect(error).toHaveCount(0)
  await expect(path).not.toHaveAttribute("aria-invalid", "true")
  await path.press("Enter")
  await expect(page.getByText("Kernel release matches.", { exact: true })).toBeVisible()
})

test("checks UTF-8 parameter bytes and control characters before confirmation", async ({
  page,
  open,
}) => {
  await open("/kernel-modules")
  await page.getByRole("tab", { name: "Module files", exact: true }).click()
  await page.locator("#kernel-module-path").fill("/data/local/tmp/sample.ko")
  await page.locator("#kernel-module-path").press("Enter")
  const input = page.locator("#kernel-module-parameters")
  const load = page.getByRole("button", { name: "Load module", exact: true })
  await input.fill("字".repeat(1366))
  await expect(
    page.getByText("Load parameters must fit within 4,096 UTF-8 bytes.", { exact: true }),
  ).toBeVisible()
  await expect(input).toHaveAttribute("aria-invalid", "true")
  await expect(load).toBeDisabled()
  await input.fill("字".repeat(1365))
  await expect(load).toBeEnabled()
  await input.fill("label=\u0085")
  await expect(
    page.getByText("Load parameters cannot contain control characters.", { exact: true }),
  ).toBeVisible()
  await expect(load).toBeDisabled()
  await input.fill("debug=1")
  await expect(load).toBeEnabled()
  await expect(input).not.toHaveAttribute("aria-invalid", "true")
})

for (const [file, message] of [
  ["no-name", "This name is estimated from the filename. The kernel determines the runtime name."],
  ["repeated-name", "Multiple name entries exist. The first metadata entry is shown."],
] as const) {
  test(`explains module name provenance in ${file}`, async ({ page, open }) => {
    await open("/kernel-modules")
    await page.getByRole("tab", { name: "Module files", exact: true }).click()
    await page.locator("#kernel-module-path").fill(`/data/local/tmp/${file}.ko`)
    await page.locator("#kernel-module-path").press("Enter")
    await expect(page.getByText(message, { exact: true })).toBeVisible()
  })
}
