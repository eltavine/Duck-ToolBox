import { computed, onMounted, reactive } from "vue"

import {
  bridgeStatus,
  historyEntry,
  pushToast,
  trickyStoreFilesCommand,
  trickyStoreKeyboxInstallCommand,
  trickyStoreStatusCommand,
  trickyStoreTargetsSaveCommand,
} from "@/lib/bridge"
import { translate } from "@/i18n"
import type {
  Envelope,
  TrickyStorePackageEntry,
  TrickyStoreTargetEntry,
  TrickyStoreTargetMode,
} from "@/lib/types"

import type { TrickyStoreBusyState, TrickyStoreWorkbenchActions, TrickyStoreWorkbenchState } from "./types"

type BusyKey = keyof TrickyStoreBusyState

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

function targetMap(targets: TrickyStoreTargetEntry[]) {
  return new Map(targets.map((entry) => [entry.package_name, entry.mode]))
}

function selectedPackage(
  packageName: string,
  targets: TrickyStoreTargetEntry[],
  fallbackMode: TrickyStoreTargetMode = "auto",
) {
  return targetMap(targets).get(packageName) ?? fallbackMode
}

function syncPackages(
  packages: TrickyStorePackageEntry[],
  targets: TrickyStoreTargetEntry[],
  systemApps: string[],
) {
  const selected = targetMap(targets)
  const systemSet = new Set(systemApps)
  return packages.map((entry) => ({
    ...entry,
    selected: selected.has(entry.package_name),
    mode: selected.get(entry.package_name) ?? entry.mode ?? "auto",
    tracked_system: systemSet.has(entry.package_name),
  }))
}

function normalizePackageName(value: string) {
  return value.trim().replace(/[!?]+$/g, "").trim()
}

export function useTrickyStoreWorkbench() {
  const state = reactive<TrickyStoreWorkbenchState>({
    bridge: bridgeStatus(),
    busy: {
      status: false,
      save: false,
      keybox: false,
      files: false,
    },
    history: [],
    status: null,
    packages: [],
    targets: [],
    systemApps: [],
    autoAddNewApps: false,
    search: "",
    keyboxSourcePath: "/storage/emulated/0/Download/keybox.xml",
    keyboxInstallResult: null,
    fileDialogOpen: false,
    fileList: null,
    filePath: "/storage/emulated/0/Download",
    lastError: "",
    errorDialogText: "",
    errorDialogOpen: false,
  })

  const visiblePackages = computed(() => {
    const query = state.search.trim().toLowerCase()
    if (!query) {
      return state.packages
    }

    return state.packages.filter((entry) => {
      const haystack = `${entry.app_label} ${entry.package_name}`.toLowerCase()
      return haystack.includes(query)
    })
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

  function setTargets(targets: TrickyStoreTargetEntry[]) {
    state.targets = [...targets].sort((left, right) =>
      left.package_name.localeCompare(right.package_name),
    )
    state.packages = syncPackages(state.packages, state.targets, state.systemApps)
  }

  function ensurePackage(packageName: string, system = false) {
    if (state.packages.some((entry) => entry.package_name === packageName)) {
      return
    }

    state.packages.push({
      package_name: packageName,
      app_label: packageName,
      system,
      selected: false,
      mode: "auto",
      tracked_system: state.systemApps.includes(packageName),
    })
  }

  function setTarget(packageName: string, selected: boolean, mode?: TrickyStoreTargetMode) {
    const normalized = normalizePackageName(packageName)
    if (!normalized) {
      return
    }

    ensurePackage(normalized)
    const map = targetMap(state.targets)
    if (selected) {
      map.set(normalized, mode ?? selectedPackage(normalized, state.targets))
    } else {
      map.delete(normalized)
    }

    setTargets(
      Array.from(map, ([package_name, targetMode]) => ({
        package_name,
        mode: targetMode,
      })),
    )
  }

  const actions: TrickyStoreWorkbenchActions = {
    async refresh() {
      await withBusy("status", async () => {
        const payload = accept(await trickyStoreStatusCommand())
        if (!payload) {
          return
        }

        state.status = payload
        state.targets = payload.targets
        state.systemApps = payload.system_apps
        state.autoAddNewApps = payload.auto_config.enabled
        state.packages = syncPackages(payload.packages, state.targets, state.systemApps)
      })
    },
    async saveTargets() {
      await withBusy("save", async () => {
        const payload = accept(
          await trickyStoreTargetsSaveCommand({
            targets: state.targets,
            system_apps: state.systemApps,
            auto_add_new_apps: state.autoAddNewApps,
          }),
          translate("messages.trickyStoreTargetsSaved"),
        )
        if (payload) {
          await actions.refresh()
        }
      })
    },
    async installKeybox() {
      await withBusy("keybox", async () => {
        const payload = accept(
          await trickyStoreKeyboxInstallCommand(state.keyboxSourcePath),
        )
        if (!payload) {
          return
        }

        state.keyboxInstallResult = payload
        pushToast(
          payload.backup_path
            ? translate("messages.trickyStoreKeyboxInstalledWithBackup", {
                backup: payload.backup_path,
              })
            : translate("messages.trickyStoreKeyboxInstalled", {
                target: payload.target_path,
              }),
        )
        await actions.refresh()
      })
    },
    async openFileDialog() {
      state.fileDialogOpen = true
      const currentPath = state.keyboxSourcePath.trim()
      const directory = currentPath.endsWith(".xml")
        ? currentPath.split("/").slice(0, -1).join("/") || "/storage/emulated/0/Download"
        : currentPath || "/storage/emulated/0/Download"
      await actions.listFiles(directory)
    },
    closeFileDialog() {
      state.fileDialogOpen = false
    },
    async listFiles(path) {
      await withBusy("files", async () => {
        const payload = accept(await trickyStoreFilesCommand(path, "xml"))
        if (payload) {
          state.fileList = payload
          state.filePath = payload.path
        }
      })
    },
    chooseFile(path) {
      state.keyboxSourcePath = path
      state.fileDialogOpen = false
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
    selectAllVisible(value) {
      for (const entry of visiblePackages.value) {
        setTarget(entry.package_name, value, entry.mode)
      }
    },
    togglePackage(packageName) {
      const current = targetMap(state.targets)
      setTarget(packageName, !current.has(packageName), current.get(packageName) ?? "auto")
    },
    setMode(packageName, mode) {
      setTarget(packageName, true, mode)
    },
    addSystemApp(packageName) {
      const normalized = normalizePackageName(packageName)
      if (!normalized || state.systemApps.includes(normalized)) {
        return
      }
      state.systemApps = [...state.systemApps, normalized].sort()
      ensurePackage(normalized, true)
      setTarget(normalized, true, selectedPackage(normalized, state.targets))
      state.packages = syncPackages(state.packages, state.targets, state.systemApps)
    },
    removeSystemApp(packageName) {
      const normalized = normalizePackageName(packageName)
      state.systemApps = state.systemApps.filter((entry) => entry !== normalized)
      state.packages = syncPackages(state.packages, state.targets, state.systemApps)
    },
  }

  onMounted(async () => {
    if (state.bridge.mode === "unavailable") {
      state.lastError = translate("messages.ksuUnavailable")
      return
    }

    await actions.refresh()
  })

  return {
    state,
    actions,
    visiblePackages,
  }
}
