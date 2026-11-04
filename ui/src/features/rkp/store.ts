import { defineStore } from "pinia"
import { computed, ref, watch } from "vue"

import { DuckError } from "@/core/duckd"

import type { ImportDraft, ImportResult, Info, Keybox, Provision, Verify } from "./api"
import { hasImportFiles, rkpApi } from "./api"
import type { Draft } from "./draft"
import { applyDetected, hasGenericDevice, toDraft, toProfile } from "./draft"

/** Profile draft and the latest results, shared by every RKP tab. */
export const useRkpStore = defineStore("rkp", () => {
  const draft = ref<Draft | null>(null)
  const secretsPath = ref("")
  const info = ref<Info | null>(null)
  const provision = ref<Provision | null>(null)
  const keybox = ref<Keybox | null>(null)
  const verify = ref<Verify | null>(null)
  const verifyPath = ref("")
  const profileBusy = ref(false)
  let profileRevision = 0
  const importDraft = ref<ImportDraft>({
    mode: "bundle",
    bundlePath: "",
    chainPath: "",
    signingPath: "",
    configPath: "",
    fingerprintPolicy: "config",
  })
  const importResult = ref<ImportResult | null>(null)
  const importBusy = ref(false)
  const importReady = ref(false)
  const importComplete = computed(() => hasImportFiles(importDraft.value))
  let importRevision = 0

  watch(
    importDraft,
    () => {
      importRevision++
      importResult.value = null
      importReady.value = false
    },
    { deep: true, flush: "sync" },
  )
  watch(
    draft,
    () => {
      profileRevision++
      info.value = provision.value = keybox.value = null
    },
    { deep: true, flush: "sync" },
  )
  watch(verifyPath, () => (verify.value = null), { flush: "sync" })

  async function runImport(action: "validate" | "diagnose" | "issue") {
    if (importBusy.value || !importComplete.value || (action === "issue" && !importReady.value)) {
      return
    }
    const revision = importRevision
    const input = { ...importDraft.value }
    importBusy.value = true
    importResult.value = null
    try {
      const result = await {
        validate: rkpApi.importValidate,
        diagnose: rkpApi.importDiagnose,
        issue: rkpApi.importIssue,
      }[action](input)
      if (revision === importRevision) {
        importResult.value = result
        if (result.stage === "validated") importReady.value = result.ready_for_issuance
      }
    } catch (error) {
      importReady.value = false
      throw error
    } finally {
      importBusy.value = false
    }
  }

  async function load(discard = false) {
    if (draft.value && !discard) return
    const shown = await rkpApi.show()
    secretsPath.value = shown.paths.profile_secrets_path
    let next = toDraft(shown.profile)
    if (hasGenericDevice(next)) {
      next = applyDetected(next, (await rkpApi.detect()).profile)
    }
    draft.value = next
  }

  async function saveProfile() {
    if (!draft.value)
      throw new DuckError("profile_unavailable", "Load an RKP profile first", "rkp.profile.save")
    const revision = profileRevision
    const saved = await rkpApi.save(toProfile(draft.value))
    secretsPath.value = saved.paths.profile_secrets_path
    if (revision !== profileRevision) return null
    draft.value = toDraft(saved.profile)
    return profileRevision
  }

  async function exclusive<T>(action: () => Promise<T>) {
    if (profileBusy.value)
      throw new DuckError("operation_pending", "An RKP operation is already running", "rkp")
    profileBusy.value = true
    try {
      return await action()
    } finally {
      profileBusy.value = false
    }
  }

  /** Every action runs against the saved profile, so unsaved edits are saved first. */
  function run<T>(action: () => Promise<T>, apply: (result: T) => void) {
    return exclusive(async () => {
      const revision = await saveProfile()
      const result = await action()
      if (revision === profileRevision) apply(result)
    })
  }

  return {
    draft,
    secretsPath,
    info,
    provision,
    keybox,
    verify,
    verifyPath,
    profileBusy,
    importDraft,
    importResult,
    importBusy,
    importReady,
    importComplete,
    validateImport: () => runImport("validate"),
    diagnoseImport: () => runImport("diagnose"),
    issueImport: () => runImport("issue"),
    load,
    save: () => exclusive(saveProfile),
    detect: () =>
      exclusive(async () => {
        if (!draft.value) return
        const revision = profileRevision
        const detected = await rkpApi.detect()
        if (revision === profileRevision) draft.value = applyDetected(draft.value, detected.profile)
      }),
    clear: () =>
      exclusive(async () => {
        await rkpApi.clear()
        draft.value = null
        info.value = provision.value = keybox.value = verify.value = null
        await load(true)
      }),
    checkKey: () =>
      run(rkpApi.info, (result) => {
        info.value = result
      }),
    runProvision: () =>
      run(
        () => rkpApi.provision(),
        (result) => {
          provision.value = result
          verifyPath.value = result.csr_path
        },
      ),
    runKeybox: () =>
      run(rkpApi.keybox, (result) => {
        keybox.value = result
        verifyPath.value = result.csr_path
      }),
    runVerify: () =>
      exclusive(async () => {
        const path = verifyPath.value
        verify.value = null
        const result = await rkpApi.verify(path.trim())
        if (path === verifyPath.value) verify.value = result
      }),
  }
})
