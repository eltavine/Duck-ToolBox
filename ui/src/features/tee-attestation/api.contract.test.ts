// @vitest-environment node
import { readFileSync } from "node:fs"

import { afterEach, describe, expect, it } from "vitest"

import { isDuckError } from "@/core/duckd"
import type { Host } from "@/core/testing/host"
import { CONTRACT, hasDuckd, installHost } from "@/core/testing/host"

import { attestationApi } from "./api"

let host: Host | undefined
afterEach(() => host?.dispose())

describe.skipIf(!hasDuckd)("TEE materials API against the real duckd", CONTRACT, () => {
  it("reports offline capabilities independently of device hardware", async () => {
    host = installHost()
    expect(await attestationApi.status()).toMatchObject({
      mode: "offline",
      device_extraction: false,
      max_input_bytes: 262144,
      max_inline_bytes: 32768,
    })
  })

  it("verifies and exports a reference keybox through binary file input", async () => {
    host = installHost()
    const bytes = readFileSync(
      new URL("../../../../crates/duck-tricky-store/assets/aosp-keybox.xml", import.meta.url),
    )
    // This reference includes legacy RSA-1024, outside the supported private-key range.
    const xml = bytes.toString("utf8").replace(/<Key algorithm="rsa">[\s\S]*?<\/Key>/, "")
    const source = { base64: Buffer.from(xml).toString("base64") }
    const verified = await attestationApi.keybox({ source, format: "auto", export: false })
    expect(verified.report.keys.map((key) => key.algorithm)).toEqual(["ecdsa"])
    expect(verified.artifacts).toBeNull()
    const saved = await attestationApi.keybox({ source, format: "xml", export: true })
    expect(saved.artifacts?.material_path).toContain("/tee-attestation-")
    expect(readFileSync(saved.artifacts!.material_path, "utf8")).toContain("<AndroidAttestation>")
    expect(JSON.stringify(saved)).not.toContain("PRIVATE KEY")
  })

  it("keeps typed errors and does not echo private input", async () => {
    host = installHost()
    const marker = "private-input-marker"
    const error = await attestationApi
      .keybox({
        source: { base64: Buffer.from(marker).toString("base64") },
        format: "auto",
        export: true,
      })
      .catch((caught: unknown) => caught)
    expect(isDuckError(error)).toBe(true)
    expect(error).toMatchObject({ code: "invalid_keybox" })
    expect(String(error)).not.toContain(marker)
  })

  it("rejects malformed identity material with the seed contract", async () => {
    host = installHost()
    const error = await attestationApi
      .seed({
        source: { base64: "YWJj" },
        csr: { base64: "YWJj" },
        format: "raw",
        derivation: "ed25519-seed",
        export: false,
      })
      .catch((caught: unknown) => caught)
    expect(error).toMatchObject({ code: "invalid_identity" })
  })

  it("transports two maximum-sized inline captures and rejects the next byte", async () => {
    host = installHost()
    const source = { base64: Buffer.alloc(32768, 17).toString("base64") }
    const csr = { base64: Buffer.alloc(32768).toString("base64") }
    const invalidCsr = await attestationApi
      .seed({ source, csr, format: "raw", offset: 0, derivation: "ed25519-seed", export: false })
      .catch((caught: unknown) => caught)
    // Both files reached the verifier; transport and JSON size checks accepted them.
    expect(invalidCsr).toMatchObject({ code: "invalid_identity" })
    const oversized = await attestationApi
      .keybox({
        source: { base64: Buffer.alloc(32769).toString("base64") },
        format: "auto",
        export: false,
      })
      .catch((caught: unknown) => caught)
    expect(oversized).toMatchObject({ code: "invalid_input" })
  })
})
