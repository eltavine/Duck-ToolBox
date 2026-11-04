import { expect, test } from "./fixtures"

const label = "Include build fingerprint in signing request"

test("profile fingerprint inclusion survives saving and device detection", async ({
  page,
  open,
}) => {
  await open("/rkp")
  await page.getByRole("tab", { name: "Profile", exact: true }).click()
  await page.getByText("Advanced overrides", { exact: true }).click()
  const toggle = page.getByRole("switch", { name: label })
  await expect(toggle).not.toBeChecked()
  await toggle.click()
  await page.getByRole("button", { name: "Save profile", exact: true }).click()
  await expect(page.getByText("Profile saved", { exact: true })).toBeVisible()
  await expect(toggle).toBeChecked()
  await page.getByRole("button", { name: "Read from device", exact: true }).click()
  await expect(toggle).toBeChecked()
  await expect(page.getByLabel("Build fingerprint", { exact: true })).not.toHaveValue("")
  await page.screenshot({ path: "test-results/rkp-profile-fingerprint.png", fullPage: true })
})

test("changing the imported fingerprint policy requires fresh validation", async ({
  page,
  open,
}) => {
  await open("/rkp")
  const policy = page.getByRole("combobox", { name: label })
  await expect(policy).toContainText("Use material configuration (off if absent)")
  await page.getByLabel("Bundle file path", { exact: true }).fill("/tmp/dice-bundle.json")
  const validate = page.getByRole("button", { name: "Validate material", exact: true })
  const issue = page.getByRole("button", { name: "Request online and export keybox", exact: true })
  await validate.click()
  await expect(issue).toBeEnabled()
  await policy.click()
  await page.getByRole("option", { name: "Include fingerprint", exact: true }).click()
  await expect(issue).toBeDisabled()
  await validate.click()
  await expect(issue).toBeEnabled()
  await page.screenshot({ path: "test-results/rkp-import-fingerprint.png", fullPage: true })
})

test("device fingerprint inclusion persists and changed parameters clear old results", async ({
  page,
  open,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem("duck-toolbox/acknowledged", JSON.stringify(["rkp-device"])),
  )
  await open("/rkp-device", { locale: "zh-CN" })
  const toggle = page.getByRole("switch", { name: "在签发请求中附带构建指纹" })
  await expect(toggle).not.toBeChecked()
  await toggle.click()
  await page.reload()
  await expect(toggle).toBeChecked()
  await page.getByLabel("指纹", { exact: true }).fill("synthetic/source/build")
  await page.getByRole("button", { name: "签发 RKP keybox", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "签发 RKP keybox", exact: true })
    .click()
  await expect(page.getByRole("heading", { name: "结果", exact: true })).toBeVisible()
  await expect(page.getByText("synthetic/source/build", { exact: true })).toBeVisible()
  await page.screenshot({ path: "test-results/rkp-device-fingerprint-zh.png", fullPage: true })
  await toggle.click()
  await expect(page.getByRole("heading", { name: "结果", exact: true })).toHaveCount(0)
})
