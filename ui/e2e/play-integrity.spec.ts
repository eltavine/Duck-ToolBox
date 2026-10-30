import { expect, test } from "./fixtures"

test("previews arbitrary configuration before applying and exposes GPL notices", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config")
  await page.getByRole("button", { name: "Raw text", exact: true }).click()
  await expect(
    page.getByText("Based on Play Integrity Fork (GPLv3), modified.", { exact: true }),
  ).toBeVisible()
  await page
    .getByLabel("Configuration", { exact: true })
    .fill("MODEL=Example\nEXTRA=preserved\nSECURITY_PATCH=2026-10-05")
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toHaveCount(
    0,
  )
  await page.getByRole("button", { name: "Preview changes", exact: true }).click()
  await expect(page.getByRole("cell", { name: "EXTRA", exact: true })).toBeVisible()
  await expect(page.getByLabel(/Also apply this keystore patch policy/)).not.toBeChecked()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await expect(page.getByLabel("Configuration", { exact: true })).toHaveValue(/EXTRA=preserved/)
  await page.getByRole("button", { name: "GPLv3", exact: true }).click()
  await expect(page.getByText(/GNU GENERAL PUBLIC LICENSE/)).toBeVisible()
  await expect(page.getByRole("link", { name: "Corresponding source", exact: true })).toBeVisible()
})
test("generates a candidate without replacing the saved configuration", async ({ page, open }) => {
  await open("/play-integrity/generate")
  await page.getByRole("button", { name: "Generate fingerprint", exact: true }).click()
  await expect(page.getByText("Candidate ready for preview", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Dismiss preview", exact: true }).click()
  await page.getByRole("link", { name: "Configuration", exact: true }).click()
  await page.getByRole("button", { name: "Raw text", exact: true }).click()
  await expect(page.getByLabel("Configuration", { exact: true })).not.toHaveValue(
    /MODEL=Pixel Canary/,
  )
})
test("supports mode changes, process diagnostics and Chinese notices", async ({ page, open }) => {
  await open("/play-integrity/compatibility")
  await page.getByRole("combobox", { name: "Engine mode", exact: true }).click()
  await page.getByRole("option", { name: "Scripts only", exact: true }).click()
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await page.getByRole("link", { name: "Status", exact: true }).click()
  await expect(page.getByText("Scripts-only mode", { exact: true })).toBeVisible()
  await page.getByRole("link", { name: "Diagnostics", exact: true }).click()
  await page.getByRole("button", { name: "Engine logs and process restart", exact: true }).click()
  await page.getByRole("button", { name: "Read engine logs", exact: true }).click()
  await expect(page.getByText(/Injection initialized/)).toBeVisible()
})

test("edits launch API levels and custom fields through the shared form controls", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", { theme: "dark" })
  await page.getByRole("button", { name: "Advanced options", exact: true }).click()
  await page
    .getByLabel("First shipped API level · DEVICE_INITIAL_SDK_INT", { exact: true })
    .fill("36")
  await page.getByLabel("ro.board.first_api_level", { exact: true }).fill("35")
  await page.getByLabel("Custom field or property", { exact: true }).fill("custom.flag")
  await page.getByRole("button", { name: "Add", exact: true }).click()
  await page.getByLabel("custom.flag", { exact: true }).fill("retain")
  await page.getByRole("button", { name: "Preview changes", exact: true }).click()
  await expect(
    page.getByRole("cell", { name: "DEVICE_INITIAL_SDK_INT", exact: true }),
  ).toBeVisible()
  await expect(page.getByRole("cell", { name: "custom.flag", exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await page.getByRole("button", { name: "Raw text", exact: true }).click()
  await expect(page.getByLabel("Configuration", { exact: true })).toHaveValue(/custom.flag=retain/)
  await expect(page.getByLabel("Configuration", { exact: true })).toHaveValue(
    /DEVICE_INITIAL_SDK_INT=36/,
  )
})

test("shows the Chinese license footer", async ({ page, open }) => {
  await open("/play-integrity", { locale: "zh-CN" })
  await expect(
    page.getByText("基于 Play Integrity Fork（GPLv3），已修改。", { exact: true }),
  ).toBeVisible()
})

test("imports a file into a preview and rolls back a rejected patch update", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", { query: { "patch-fail": "1" } })
  await page.getByRole("button", { name: "Import, export and raw files", exact: true }).click()
  await page.locator('input[aria-label="Import"]').setInputFiles({
    name: "custom.pif.prop",
    mimeType: "text/plain",
    buffer: Buffer.from("MODEL=Candidate\nSECURITY_PATCH=2026-10-05\nEXTRA=keep"),
  })
  await expect(page.getByRole("cell", { name: "EXTRA", exact: true })).toBeVisible()
  await page.getByLabel(/Also apply this keystore patch policy/).check()
  await page.getByRole("button", { name: "Apply configuration", exact: true }).click()
  await expect(page.getByText("Patch fixture rejected", { exact: true })).toBeVisible()
  await expect(page.getByLabel("Configuration", { exact: true })).toHaveValue(/MODEL=Candidate/)
  await expect(page.getByText("Draft · not applied", { exact: true })).toBeVisible()
  await page.getByRole("link", { name: "Diagnostics", exact: true }).click()
  await page.getByRole("button", { name: "Show field comparison", exact: true }).click()
  await expect(page.locator("dl").getByText("Pixel 9 Pro", { exact: true })).toBeVisible()
})

test("requires confirmation to recover an interrupted backend transaction", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", { query: { "interrupted-transaction": "1" } })
  await page.getByRole("button", { name: "Raw text", exact: true }).click()
  await expect(page.getByText("Interrupted configuration update", { exact: true })).toBeVisible()
  await page.getByLabel("Configuration", { exact: true }).fill("MODEL=Blocked")
  await page.getByRole("button", { name: "Preview changes", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Apply configuration", exact: true }),
  ).toBeDisabled()
  await page.getByRole("button", { name: "Confirm recovery", exact: true }).click()
  await expect(page.getByRole("alertdialog")).toBeVisible()
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Apply configuration", exact: true }),
  ).toBeDisabled()
  await page.getByRole("button", { name: "Confirm recovery", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Confirm recovery", exact: true })
    .click()
  await expect(page.getByRole("button", { name: "Apply configuration", exact: true })).toBeEnabled()
})

test("shows interrupted application recovery on the home page", async ({ page, open }) => {
  await open("/", { query: { "interrupted-transaction": "1" } })
  await expect(page.getByRole("button", { name: "Confirm recovery", exact: true })).toBeVisible()
})

test("can disable PIF even when its configuration page reports an error", async ({
  page,
  open,
}) => {
  await open("/play-integrity/config", { query: { "invalid-pif": "1" } })
  await expect(page.getByText("Invalid configuration fixture", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Disable PIF now", exact: true }).click()
  await expect(page.getByRole("switch", { name: "Enable the engine", exact: true })).toHaveCount(0)
  await expect(
    page
      .getByText(/PIF is disabled\. Configuration and other toolbox features are retained\./)
      .first(),
  ).toBeVisible()
})
test("disabling cancels generation and the independent power switch can enable again", async ({
  page,
  open,
}) => {
  await open("/play-integrity/generate")
  await page.getByRole("button", { name: "Generate fingerprint", exact: true }).click()
  await page.getByRole("button", { name: "Disable PIF now", exact: true }).click()
  await expect(page.getByText("Generation cancelled", { exact: true })).toBeVisible()
  await expect(page.getByRole("switch", { name: "Enable the engine", exact: true })).toHaveCount(0)
  await page.getByRole("link", { name: "Status", exact: true }).click()
  await page.getByRole("switch", { name: "Enable the engine", exact: true }).click()
  await expect(page.getByRole("switch", { name: "Enable the engine", exact: true })).toBeChecked()
})

test("only status exposes engine enable controls", async ({ page, open }) => {
  await open("/play-integrity")
  const power = page.getByRole("switch", { name: "Enable the engine" })
  const stop = page.getByRole("button", { name: "Disable PIF now" })
  const tabs = ["Configuration", "Fingerprints", "Compatibility", "Diagnostics", "Licenses"]
  await expect(power).toBeVisible()
  for (const tab of tabs) {
    await page.getByRole("link", { name: tab, exact: true }).click()
    await expect(power).toHaveCount(0)
    await expect(stop).toBeVisible()
  }
  await page.getByRole("link", { name: "Status", exact: true }).click()
  await expect(power).toBeVisible()
})

test("Play Integrity back navigates home from every tab", async ({ page, open }) => {
  const paths = [
    "/play-integrity",
    "/play-integrity/config",
    "/play-integrity/generate",
    "/play-integrity/compatibility",
    "/play-integrity/diagnostics",
    "/play-integrity/license",
  ]
  for (const path of paths) {
    await open(path)
    await page.getByRole("button", { name: "Back" }).click()
    await expect(page).toHaveURL(/#\/$/)
  }
})

test("tab pressed overlay stays within its link bounds", async ({ page, open }) => {
  await open("/play-integrity")
  const nav = page.getByRole("navigation", { name: "Play Integrity" })
  for (const link of await nav.getByRole("link").all()) {
    const bounds = await link.evaluate((element) => {
      const layer = getComputedStyle(element, "::before")
      const rect = element.getBoundingClientRect()
      return {
        position: getComputedStyle(element).position,
        width: parseFloat(layer.width),
        height: parseFloat(layer.height),
        maxWidth: rect.width,
        maxHeight: rect.height,
      }
    })
    expect(bounds.position).toBe("relative")
    expect(bounds.width).toBeGreaterThan(0)
    expect(bounds.width).toBeLessThanOrEqual(bounds.maxWidth + 1)
    expect(bounds.height).toBeLessThanOrEqual(bounds.maxHeight + 1)
  }
})

test("retains uncommitted edits during tab changes", async ({ page, open }) => {
  await open("/play-integrity/config")
  await page.getByRole("button", { name: "Raw text", exact: true }).click()
  const draft = "MODEL=DraftOnly\nEXTRA=keep"
  const content = page.getByLabel("Configuration", { exact: true })
  await content.fill(draft)
  await page.getByRole("link", { name: "Diagnostics", exact: true }).click()
  await page.getByRole("link", { name: "Configuration", exact: true }).click()
  await expect(content).toHaveValue(draft)
})

test("aligns Play Integrity panels on mobile without page-level horizontal overflow", async ({
  page,
  open,
}) => {
  await page.setViewportSize({ width: 360, height: 780 })
  await open("/play-integrity")
  const nav = page.getByRole("navigation", { name: "Play Integrity" })
  const navLeft = await nav.evaluate((element) => element.getBoundingClientRect().left)
  const panelLeft = await page
    .getByRole("heading", { name: "Status", exact: true })
    .locator("xpath=ancestor::section[1]")
    .evaluate((element) => element.getBoundingClientRect().left)
  expect(Math.abs(navLeft - panelLeft)).toBeLessThanOrEqual(1)

  await nav.getByRole("link", { name: "Configuration", exact: true }).click()
  const stopLeft = await page
    .getByRole("button", { name: "Disable PIF now", exact: true })
    .locator("xpath=ancestor::section[1]")
    .evaluate((element) => element.getBoundingClientRect().left)
  expect(Math.abs(navLeft - stopLeft)).toBeLessThanOrEqual(1)

  await nav.getByRole("link", { name: "Licenses", exact: true }).click()
  await expect(nav.getByRole("link", { name: "Licenses", exact: true })).toHaveAttribute(
    "aria-current",
    "page",
  )
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
  ).toBe(true)
})
