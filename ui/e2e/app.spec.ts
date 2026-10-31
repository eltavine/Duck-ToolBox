import { expect, test } from "./fixtures"

test("lists every tool on the home page", async ({ page, open }) => {
  await open("/")
  for (const name of [
    "Tricky Store",
    "Play Integrity",
    "RKP Workbench",
    "Device RKP Issuer",
    "TEE Attestation Extractor",
    "Device ID Provisioner",
    "Module & updates",
    "Miscellaneous",
  ]) {
    await expect(page.getByRole("button", { name: new RegExp(name) })).toBeVisible()
  }
})

test("confirms the Device ID warning before opening the tool", async ({ page, open }) => {
  await open("/")
  await page.getByRole("button", { name: /Device ID Provisioner/ }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText(/Qualcomm-based OnePlus/)).toBeVisible()
  await dialog.getByRole("button", { name: "Continue" }).click()
  await expect(page.getByLabel("Brand")).toHaveValue("OnePlus")
})

test("confirms the device RKP warning before opening the tool", async ({ page, open }) => {
  await open("/")
  await page.getByRole("button", { name: /Device RKP Issuer/ }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText(/only works on some devices/)).toBeVisible()
  await dialog.getByRole("button", { name: "Continue" }).click()
  await expect(page.getByText("Prerequisites").first()).toBeVisible()
})

test("preserves MediaTek identifier defaults, dry run and confirmed live write", async ({
  page,
  open,
}) => {
  await open("/device-ids", { query: { soc: "mediatek" } })
  await expect(page.getByLabel("Brand", { exact: true })).toHaveValue("Xelex")
  await expect(page.getByText(/TrustKernel provisioning is experimental/)).toBeVisible()
  await page.getByRole("button", { name: "Run dry run" }).click()
  await expect(page.getByText("TrustKernel", { exact: true })).toBeVisible()
  await page.getByRole("switch", { name: "Dry run" }).click()
  await page.getByRole("button", { name: "Provision device IDs" }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toBeVisible()
  await dialog.getByRole("button", { name: "Provision device IDs" }).click()
  await expect(page.getByText("Object existence only", { exact: true })).toBeVisible()
  await expect(page.getByText("/vendor/lib64/libkphproxy.so", { exact: true })).toBeVisible()
})

test("asks again before sending a request to the KeyMint TA", async ({ page, open }) => {
  // The opening warning is acknowledged once per session, as on the home page.
  await page.addInitScript(() =>
    localStorage.setItem("duck-toolbox/acknowledged", JSON.stringify(["rkp-device"])),
  )
  await open("/rkp-device")
  await page.getByRole("button", { name: "Issue RKP keybox" }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog.getByText(/only works on some devices/)).toBeVisible()
  await dialog.getByRole("button", { name: "Issue RKP keybox" }).click()
  await expect(page.getByText("RKP keybox issued")).toBeVisible()
})

test("runs a Device ID dry run", async ({ page, open }) => {
  await open("/device-ids")
  await expect(page.getByRole("switch", { name: "Dry run" })).toBeChecked()
  await page.getByRole("button", { name: "Run dry run" }).click()
  await expect(page.getByText("Device ID provisioning finished")).toBeVisible()
  await expect(page.getByText("device-ids-20260928.json")).toBeVisible()
})

test("fills Device ID defaults when revisiting with a fresh cache", async ({ page, open }) => {
  await open("/device-ids")
  await expect(page.getByLabel("Brand", { exact: true })).toHaveValue("OnePlus")
  await page.evaluate(() => {
    location.hash = "#/"
  })
  await expect(page.getByRole("button", { name: /Tricky Store/ })).toBeVisible()
  await page.evaluate(() => {
    location.hash = "#/device-ids"
  })
  await expect(page.getByLabel("Brand", { exact: true })).toHaveValue("OnePlus")
})

test("keeps RKP navigation usable while a profile request is pending", async ({ page, open }) => {
  await open("/rkp")
  await expect(page.getByRole("tab", { name: "Import DICE", exact: true })).toBeVisible()
  await page.evaluate(() => {
    const host = (
      window as unknown as {
        ksu: { exec: (shell: string, options: string, callback: string) => void }
      }
    ).ksu
    const original = host.exec
    host.exec = (shell, options, callback) => {
      if (!shell.includes("'rkp' 'profile' 'show'")) original(shell, options, callback)
    }
  })
  await page.getByRole("tab", { name: "Profile", exact: true }).click()
  await page.getByRole("tab", { name: "Import DICE", exact: true }).click({ timeout: 2000 })
  await expect(page.getByLabel("Bundle file path", { exact: true })).toBeVisible()
})

test("saves an RKP profile with a direct seed", async ({ page, open }) => {
  await open("/rkp")
  await page.getByRole("tab", { name: "Profile", exact: true }).click()
  await page.getByText("Direct seed").click()
  await page.getByLabel("Seed (CDI_Leaf)").fill("ab".repeat(32))
  await page.getByRole("button", { name: "Save profile" }).click()
  await expect(page.getByText("Profile saved")).toBeVisible()
})

test("installs an RKP keybox through the Tricky Store extension point", async ({ page, open }) => {
  await open("/rkp")
  await page.getByRole("tab", { name: "Profile", exact: true }).click()
  await expect(page.getByText("Experimental mode", { exact: true })).toBeVisible()
  await page.getByText("Direct seed").click()
  await page.getByLabel("Seed (CDI_Leaf)").fill("ab".repeat(32))
  await page.getByRole("tab", { name: "Keybox" }).click()
  await page.getByRole("button", { name: "Generate keybox" }).click()
  await expect(page.getByText("keybox.xml generated")).toBeVisible()
  await page.getByRole("button", { name: "Install to Tricky Store" }).click()
  await expect(
    page.getByText("Keybox installed to /data/adb/tricky_store/keybox.xml"),
  ).toBeVisible()
})

test("imports DICE material before requesting and clears changed results", async ({
  page,
  open,
}) => {
  await open("/rkp")
  await expect(page.getByRole("tab", { name: "Import DICE", exact: true })).toHaveAttribute(
    "data-state",
    "active",
  )
  const issue = page.getByRole("button", { name: "Request online and export keybox", exact: true })
  const validate = page.getByRole("button", { name: "Validate material", exact: true })
  await expect(validate).toBeDisabled()
  await expect(issue).toBeDisabled()
  await expect(page.getByText("Experimental mode", { exact: true })).toHaveCount(0)
  await page.getByLabel("Bundle file path", { exact: true }).fill("/tmp/dice-bundle.json")
  await expect(issue).toBeDisabled()
  await validate.click()
  await expect(page.getByText("Validation passed; ready for an online request.")).toBeVisible()
  await issue.click()
  await expect(
    page.getByText(
      "The server accepted the request. Certificates were verified and the keybox was exported.",
    ),
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Install to Tricky Store" })).toBeVisible()
  await expect(
    page.getByText("/data/adb/duck-toolbox/var/outputs/rkp-import/keybox.xml"),
  ).toBeVisible()
  await expect(issue).toBeEnabled()
  await page.getByRole("tab", { name: "Import DICE", exact: true }).scrollIntoViewIfNeeded()
  await page.screenshot({ path: "test-results/rkp-import-exported.png", fullPage: true })
  await page.getByLabel("Request configuration file (JSON, optional)").fill("/tmp/changed.json")
  await expect(page.getByText("Result files", { exact: true })).toHaveCount(0)
  await expect(page.getByRole("button", { name: "Install to Tricky Store" })).toHaveCount(0)
  await expect(issue).toBeDisabled()
  const storage = await page.evaluate(() => JSON.stringify({ ...localStorage }))
  expect(storage).not.toContain("dice-bundle")
  expect(storage).not.toContain("changed.json")
})

test("keeps DICE split inputs and offline diagnostics distinct from issuance", async ({
  page,
  open,
}) => {
  await open("/rkp")
  await page.getByRole("radio", { name: "Separate chain and signing files" }).check()
  const validate = page.getByRole("button", { name: "Validate material", exact: true })
  await page.getByLabel("DICE chain file (raw CBOR)").fill("/tmp/chain.cbor")
  await expect(validate).toBeDisabled()
  await page.getByLabel("Signing material file (JSON)").fill("/tmp/signing.json")
  await expect(validate).toBeEnabled()
  await page.getByRole("button", { name: "Build diagnostic CSR (offline)" }).click()
  await expect(
    page.getByText("Diagnostic only: generated offline; no certificate request was sent."),
  ).toBeVisible()
  await expect(
    page.getByRole("button", { name: "Request online and export keybox" }),
  ).toBeDisabled()
  await expect(page.getByRole("button", { name: "Install to Tricky Store" })).toHaveCount(0)
  await page.getByRole("tab", { name: "Profile", exact: true }).click()
  await expect(page.getByText("Experimental mode", { exact: true })).toBeVisible()
  await page.getByRole("tab", { name: "CSR diagnostics", exact: true }).click()
  await expect(page.getByText("Experimental mode", { exact: true })).toHaveCount(0)
})

test("lays out right-to-left languages mirrored", async ({ page, open }) => {
  await open("/", { locale: "ar" })
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl")
  await expect(page.locator("html")).toHaveAttribute("lang", "ar")
})

test("follows the saved language", async ({ page, open }) => {
  await open("/", { locale: "zh-CN" })
  await expect(page.getByRole("heading", { name: "工具", exact: true })).toBeVisible()
  await expect(page.locator("html")).toHaveAttribute("dir", "ltr")
})

test("updates Tricky Addon's translations without a module update", async ({ page, open }) => {
  await page.route("https://raw.githubusercontent.com/**", (route) => {
    const url = route.request().url()
    if (url.endsWith("/version")) return route.fulfill({ body: "29991231" })
    if (url.endsWith("/languages.json")) return route.fulfill({ body: '{"en":"English"}' })
    if (url.endsWith("/strings/en.xml")) {
      return route.fulfill({
        body: '<resources><string name="menu_keybox">Keyboxes (%s)</string></resources>',
      })
    }
    return route.fulfill({ status: 404 })
  })
  await open("/settings")
  await expect(page.getByText("Bundle 20251021")).toBeVisible()
  await page.getByRole("button", { name: "Update translation bundle" }).click()
  await expect(page.getByText("Translation bundle updated successfully")).toBeVisible()
  await page.waitForEvent("load")
  await expect(page.getByText("Bundle 29991231")).toBeVisible()

  await page.getByRole("button", { name: "Help translate" }).click()
  await expect(page.getByRole("link", { name: "Crowdin" })).toBeVisible()
  await page.keyboard.press("Escape")

  await open("/tricky-store")
  await page.getByRole("button", { name: "More" }).click()
  // Downloaded strings are compiled at runtime, placeholders included.
  await expect(page.getByRole("menuitem", { name: "Keyboxes ({0})" })).toBeHidden()
  await expect(page.getByRole("menuitem", { name: /^Keyboxes \(/ })).toBeVisible()
})

test("shows the entry toggle contributed by the Tricky Store feature", async ({ page, open }) => {
  await open("/settings")
  await expect(page.getByRole("switch", { name: "Keystore module entry" })).toBeChecked()
})
