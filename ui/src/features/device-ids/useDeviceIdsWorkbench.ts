import { onMounted, reactive } from "vue"

import {
  bridgeStatus,
  deviceIdsDefaultsCommand,
  deviceIdsProvisionCommand,
  historyEntry,
  pushToast,
} from "@/lib/bridge"
import { translate } from "@/i18n"
import type { DeviceIdsProfileData, Envelope } from "@/lib/types"

import type { DeviceIdsWorkbenchActions, DeviceIdsWorkbenchState } from "./types"

type BusyKey = keyof DeviceIdsWorkbenchState["busy"]

function defaultProfile(): DeviceIdsProfileData {
  return {
    brand: "",
    device: "",
    product: "",
    serial: "",
    manufacturer: "",
    model: "",
    imei: "",
    imei2: "",
    meid: "",
    meid2: "",
    ta_name: "keymaster64",
    ta_path: "/vendor/firmware_mnt/image",
    dry_run: false,
  }
}

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

export function useDeviceIdsWorkbench() {
  const state = reactive<DeviceIdsWorkbenchState>({
    bridge: bridgeStatus(),
    profile: defaultProfile(),
    busy: {
      defaults: false,
      provision: false,
    },
    history: [],
    result: null,
    lastError: "",
    errorDialogText: "",
    errorDialogOpen: false,
  })

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

  async function withBusy(key: BusyKey, action: () => Promise<void>) {
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

  const actions: DeviceIdsWorkbenchActions = {
    async loadDefaults() {
      await withBusy("defaults", async () => {
        const payload = accept(await deviceIdsDefaultsCommand())
        if (payload) {
          state.profile = payload
        }
      })
    },
    async runProvision() {
      await withBusy("provision", async () => {
        const payload = accept(
          await deviceIdsProvisionCommand(state.profile),
          translate("messages.deviceIdsDone"),
        )
        if (payload) {
          state.result = payload
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
    dismissErrorDialog() {
      state.errorDialogOpen = false
    },
  }

  onMounted(async () => {
    if (state.bridge.mode === "unavailable") {
      state.lastError = translate("messages.ksuUnavailable")
      return
    }

    await actions.loadDefaults()
  })

  return {
    state,
    actions,
  }
}
