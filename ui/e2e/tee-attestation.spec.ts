import { expect, test } from "./fixtures"

test("extracts a keybox and exports only after verification", async ({ page, open }) => {
  await open("/tee-attestation")
  await expect(page.getByText("Offline processing", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Export keybox", exact: true })).toHaveCount(0)
  await page.getByLabel("Keybox capture", { exact: true }).fill("/data/local/tmp/capture.pem")
  await page.getByRole("button", { name: "Extract and verify", exact: true }).click()
  await expect(page.getByText("Verification result", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Export keybox", exact: true }).click()
  await expect(page.getByText("Private files saved", { exact: true })).toBeVisible()
  await expect(
    page.getByRole("button", { name: "Install to Tricky Store", exact: true }),
  ).toBeVisible()
  await page.getByLabel("Keybox capture", { exact: true }).fill("/data/local/tmp/other.pem")
  await expect(page.getByText("Verification result", { exact: true })).toHaveCount(0)
})

test("loads binary captures from the system file input", async ({ page, open }) => {
  await open("/tee-attestation")
  await page.locator("#tee-keybox-upload").setInputFiles({
    name: "capture.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.from([0, 255, 17, 128]),
  })
  await expect(page.getByLabel("Keybox capture", { exact: true })).toHaveValue("capture.bin")
  await expect(page.getByRole("button", { name: "Extract and verify", exact: true })).toBeEnabled()
  await page.getByRole("button", { name: "Clear Keybox capture", exact: true }).click()
  await expect(page.getByRole("button", { name: "Extract and verify", exact: true })).toBeDisabled()
})

test("withholds export for a mismatched RKP identity and resets stale results", async ({
  page,
  open,
}) => {
  await open("/tee-attestation")
  await page.getByRole("tab", { name: "RKP identity", exact: true }).click()
  await page.getByLabel("Identity capture", { exact: true }).fill("/data/local/tmp/mismatch.bin")
  await page.getByLabel("CSR file", { exact: true }).fill("/data/local/tmp/csr.cbor")
  await page.getByRole("button", { name: "Verify identity", exact: true }).click()
  await expect(
    page.getByText("The candidate does not match the CSR signing identity.", { exact: true }),
  ).toBeVisible()
  await expect(
    page.getByRole("button", { name: "Export matched identity", exact: true }),
  ).toHaveCount(0)
  await page.getByLabel("Identity capture", { exact: true }).fill("/data/local/tmp/matched.bin")
  await expect(page.getByText("Verification result", { exact: true })).toHaveCount(0)
  await page.getByRole("button", { name: "Verify identity", exact: true }).click()
  await expect(
    page.getByText("The candidate matches the CSR signing identity.", { exact: true }),
  ).toBeVisible()
  await page.getByRole("button", { name: "Export matched identity", exact: true }).click()
  await expect(page.getByText("Private files saved", { exact: true })).toBeVisible()
})

test("rejects oversized uploads and invalid offsets", async ({ page, open }) => {
  await open("/tee-attestation")
  await page.locator("#tee-keybox-upload").setInputFiles({
    name: "large.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.alloc(32769),
  })
  await expect(page.getByRole("button", { name: "Extract and verify", exact: true })).toBeDisabled()
  await expect(
    page.getByText(
      "Choose a nonempty file of at most 32 KiB. For larger captures, enter a device file path (up to 256 KiB).",
      { exact: true },
    ),
  ).toBeVisible()
  await page.getByRole("tab", { name: "RKP identity", exact: true }).click()
  await page.getByLabel("Identity capture", { exact: true }).fill("/tmp/seed.bin")
  await page.getByLabel("CSR file", { exact: true }).fill("/tmp/csr.cbor")
  await page.getByLabel("Byte offset (optional)", { exact: true }).fill("-1")
  await expect(page.getByRole("button", { name: "Verify identity", exact: true })).toBeDisabled()
  await page.getByRole("combobox", { name: "Input encoding", exact: true }).click()
  await page.getByRole("option", { name: "64 hexadecimal digits", exact: true }).click()
  await expect(page.getByLabel("Byte offset (optional)", { exact: true })).toHaveCount(0)
  await expect(page.getByRole("button", { name: "Verify identity", exact: true })).toBeEnabled()
  await page.getByRole("combobox", { name: "Input encoding", exact: true }).click()
  await page.getByRole("option", { name: "Raw bytes", exact: true }).click()
  await expect(page.getByRole("button", { name: "Verify identity", exact: true })).toBeDisabled()
})
