import { computed, onMounted, reactive } from "vue"

import {
  artifactsCommand,
  bridgeStatus,
  historyEntry,
  infoCommand,
  keyboxCommand,
  profileClear,
  profileSave,
  profileShow,
  provisionCommand,
  pushToast,
  replaceTrickyStoreKeyboxCommand,
  systemProfileDefaults,
  verifyCommand,
} from "@/lib/bridge"
import { translate } from "@/i18n"
import { defaultProfile } from "@/lib/types"
import type {
  Envelope,
  ProfileData,
  ProfileEnvelopeData,
} from "@/lib/types"

import type {
  RkpWorkbenchActions,
  RkpWorkbenchState,
  UiProfile,
} from "./types"
import {
  DEFAULT_KDF_LABEL,
  applyDetectedSystemProfile,
  mergeMissingSystemProfile,
  normalizeUiProfile,
} from "./profileState"

type BusyKey = keyof RkpWorkbenchState["busy"]

function envelopeErrorText<T>(envelope: Envelope<T>) {
  if (!envelope.error) {
    return translate("messages.commandFailed")
  }

  const parts = [
    `${translate("dialog.command")}: ${envelope.command}`,
    `${translate("dialog.errorCode")}: ${envelope.error.code}`,
    `${translate("dialog.errorMessage")}: ${envelope.error.message}`,
  ]

  if (envelope.error.details !== undefined) {
    parts.push("")
    parts.push(JSON.stringify(envelope.error.details, null, 2))
  }

  return parts.join("\n")
}

function unexpectedErrorText(error: unknown) {
  return error instanceof Error && error.message.trim()
    ? error.message
    : translate("messages.unexpectedError")
}

function toUiProfile(source: ProfileData): UiProfile {
  const shared = {
    device: {
      ...source.device,
      vbmeta_digest: source.device.vbmeta_digest ?? "",
    },
    fingerprint: source.fingerprint.value,
    server_url: source.server_url,
    num_keys: source.num_keys,
    output_path: source.output_path,
  }

  if (source.key_source.kind === "hw-key") {
    return normalizeUiProfile({
      mode: "hw-key",
      seed_hex: "",
      hw_key_hex: source.key_source.hw_key_hex,
      kdf_label: source.key_source.kdf_label || DEFAULT_KDF_LABEL,
      ...shared,
    })
  }

  if (source.key_source.kind === "seed") {
    return normalizeUiProfile({
      mode: "seed",
      seed_hex: source.key_source.seed_hex,
      hw_key_hex: "",
      kdf_label: "",
      ...shared,
    })
  }

  return normalizeUiProfile({
    mode: "hw-key",
    seed_hex: "",
    hw_key_hex: "",
    kdf_label: DEFAULT_KDF_LABEL,
    ...shared,
  })
}

function toProfileData(source: UiProfile): ProfileData {
  const normalized = normalizeUiProfile(source)
  const vbmetaDigest = normalized.device.vbmeta_digest?.trim()
  const seedHex = normalized.seed_hex.trim()
  const hwKeyHex = normalized.hw_key_hex.trim()
  const kdfLabel = normalized.kdf_label.trim() || DEFAULT_KDF_LABEL

  return {
    key_source:
      normalized.mode === "hw-key"
        ? hwKeyHex
          ? {
              kind: "hw-key",
              hw_key_hex: hwKeyHex,
              kdf_label: kdfLabel,
            }
          : { kind: "unset" }
        : seedHex
          ? {
              kind: "seed",
              seed_hex: seedHex,
            }
          : { kind: "unset" },
    device: {
      ...normalized.device,
      vbmeta_digest: vbmetaDigest ? vbmetaDigest : "",
    },
    fingerprint: {
      value: normalized.fingerprint.trim(),
    },
    server_url: normalized.server_url.trim(),
    num_keys: Math.max(1, Number(normalized.num_keys || 1)),
    output_path: normalized.output_path.trim(),
  }
}

export function useRkpWorkbench() {
  const bridge = bridgeStatus()
  const state = reactive<RkpWorkbenchState>({
    bridge,
    profile: toUiProfile(defaultProfile()),
    paths: null,
    busy: {
      load: false,
      device: false,
      save: false,
      clear: false,
      info: false,
      provision: false,
      keybox: false,
      replaceTrickyStore: false,
      verify: false,
      artifacts: false,
    },
    history: [],
    infoResult: null,
    provisionResult: null,
    keyboxResult: null,
    verifyResult: null,
    artifacts: null,
    verifyPath: "var/outputs/keybox.cbor",
    lastError: "",
    errorDialogText: "",
    errorDialogOpen: false,
    keyboxPreviewOpen: false,
    trickyStorePromptOpen: false,
    trickyStoreInstallResult: null,
    activeWorkspace: "profile",
  })

  const moduleRoot = computed(() => state.paths?.root ?? state.bridge.moduleRoot)
  const outputDirectory = computed(
    () => state.paths?.outputs_dir ?? `${state.bridge.dataRoot}/var/outputs`,
  )
  const secretPath = computed(
    () =>
      state.paths?.profile_secrets_path ??
      `${state.bridge.dataRoot}/var/profile.secrets.toml`,
  )
  const historyCount = computed(() => state.history.length)

  function applyProfileData(payload: ProfileEnvelopeData) {
    state.profile = toUiProfile(payload.profile)
    state.paths = payload.paths
  }

  function remember<T>(envelope: Envelope<T>) {
    state.history.unshift(historyEntry(envelope))
    state.history.splice(10)
  }

  function accept<T>(envelope: Envelope<T>, successMessage?: string): T | null {
    remember(envelope)

    if (!envelope.ok || !envelope.data) {
      state.lastError =
        envelope.error?.message ?? translate("messages.commandFailed")
      state.errorDialogText = envelopeErrorText(envelope)
      state.errorDialogOpen = true
      pushToast(state.lastError)
      return null
    }

    state.lastError = ""
    if (successMessage) {
      pushToast(successMessage)
    }
    return envelope.data
  }

  function handleUnexpectedError(error: unknown) {
    state.lastError = unexpectedErrorText(error)
    state.errorDialogText = state.lastError
    state.errorDialogOpen = true
    pushToast(state.lastError)
  }

  async function hydrateProfileDefaults(
    overwriteDetected = false,
    successMessage?: string,
  ) {
    const defaults = await systemProfileDefaults()
    if (!defaults) {
      if (overwriteDetected) {
        state.lastError = translate("messages.deviceReadFailed")
        pushToast(state.lastError)
      }
      return false
    }

    state.profile = overwriteDetected
      ? applyDetectedSystemProfile(state.profile, defaults)
      : mergeMissingSystemProfile(state.profile, defaults)
    state.lastError = ""
    if (successMessage) {
      pushToast(successMessage)
    }
    return true
  }

  async function withBusy(
    key: BusyKey,
    action: () => Promise<void>,
  ) {
    if (state.busy[key]) {
      return
    }

    state.busy[key] = true
    try {
      await action()
    } catch (error) {
      handleUnexpectedError(error)
    } finally {
      state.busy[key] = false
    }
  }

  async function persistProfile(notify = false) {
    state.profile = normalizeUiProfile(state.profile)
    const saved = accept(
      await profileSave(toProfileData(state.profile)),
      notify ? translate("messages.profileSaved") : undefined,
    )
    if (!saved) {
      return false
    }

    applyProfileData(saved)
    return true
  }

  const actions: RkpWorkbenchActions = {
    setWorkspace(workspace) {
      state.activeWorkspace = workspace
    },
    dismissErrorDialog() {
      state.errorDialogOpen = false
    },
    openKeyboxPreview() {
      if (state.keyboxResult?.keybox_xml) {
        state.keyboxPreviewOpen = true
      }
    },
    closeKeyboxPreview() {
      state.keyboxPreviewOpen = false
    },
    openTrickyStorePrompt() {
      if (state.keyboxResult) {
        state.trickyStorePromptOpen = true
      }
    },
    closeTrickyStorePrompt() {
      state.trickyStorePromptOpen = false
    },
    async loadProfile() {
      await withBusy("load", async () => {
        const payload = accept(await profileShow())
        if (payload) {
          applyProfileData(payload)
          await hydrateProfileDefaults()
        }
      })
    },
    async syncDeviceProfile() {
      await withBusy("device", async () => {
        await hydrateProfileDefaults(
          true,
          translate("messages.deviceValuesLoaded"),
        )
      })
    },
    async saveProfile() {
      await withBusy("save", async () => {
        await persistProfile(true)
        await actions.reloadArtifacts()
      })
    },
    async clearProfile() {
      await withBusy("clear", async () => {
        const payload = accept(
          await profileClear(),
          translate("messages.profileCleared"),
        )
        if (payload) {
          await actions.loadProfile()
          state.infoResult = null
          state.provisionResult = null
          state.keyboxResult = null
          state.trickyStoreInstallResult = null
          state.trickyStorePromptOpen = false
          state.verifyResult = null
          await actions.reloadArtifacts()
        }
      })
    },
    async reloadArtifacts() {
      await withBusy("artifacts", async () => {
        const payload = accept(await artifactsCommand())
        if (payload) {
          state.artifacts = payload
        }
      })
    },
    async runInfo() {
      await withBusy("info", async () => {
        if (await persistProfile()) {
          const payload = accept(await infoCommand())
          if (payload) {
            state.infoResult = payload
            state.activeWorkspace = "info"
          }
        }
      })
    },
    async runProvision() {
      await withBusy("provision", async () => {
        if (await persistProfile()) {
          const payload = accept(
            await provisionCommand(),
            translate("messages.provisionDone"),
          )
          if (payload) {
            state.provisionResult = payload
            state.verifyPath = payload.csr_path
            state.activeWorkspace = "provision"
            await actions.reloadArtifacts()
          }
        }
      })
    },
    async runKeybox() {
      await withBusy("keybox", async () => {
        if (await persistProfile()) {
          const payload = accept(
            await keyboxCommand(),
            translate("messages.keyboxDone"),
          )
          if (payload) {
            state.keyboxResult = payload
            state.trickyStoreInstallResult = null
            state.verifyPath = payload.csr_path
            state.keyboxPreviewOpen = false
            state.trickyStorePromptOpen = true
            state.activeWorkspace = "keybox"
            await actions.reloadArtifacts()
          }
        }
      })
    },
    async replaceTrickyStoreKeybox() {
      if (!state.keyboxResult) {
        return
      }
      const keyboxPath = state.keyboxResult.keybox_path

      await withBusy("replaceTrickyStore", async () => {
        state.trickyStorePromptOpen = false

        const payload = accept(
          await replaceTrickyStoreKeyboxCommand(keyboxPath),
        )
        if (!payload) {
          return
        }

        state.trickyStoreInstallResult = payload
        pushToast(
          payload.backup_path
            ? translate("messages.trickyStoreKeyboxInstalledWithBackup", {
                backup: payload.backup_path,
              })
            : translate("messages.trickyStoreKeyboxInstalled", {
                target: payload.target_path,
              }),
        )
      })
    },
    async runVerify() {
      await withBusy("verify", async () => {
        const payload = accept(await verifyCommand(state.verifyPath))
        if (payload) {
          state.verifyResult = payload
          state.activeWorkspace = "verify"
        }
      })
    },
    async copyText(value) {
      try {
        const writeText = globalThis.navigator?.clipboard?.writeText
        if (!writeText) {
          throw new Error("clipboard unavailable")
        }
        await writeText.call(globalThis.navigator.clipboard, value)
        pushToast(translate("messages.copied"))
      } catch {
        pushToast(translate("messages.clipboardUnsupported"))
      }
    },
  }

  onMounted(async () => {
    if (state.bridge.mode === "unavailable") {
      state.lastError = translate("messages.ksuUnavailable")
      return
    }

    await actions.loadProfile()
    await actions.reloadArtifacts()
  })

  return {
    state,
    actions,
    moduleRoot,
    outputDirectory,
    secretPath,
    historyCount,
  }
}
