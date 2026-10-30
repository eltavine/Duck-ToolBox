import { computed, nextTick, ref, watch } from "vue"
import { useSessionStorage } from "@vueuse/core"
import { useQuery } from "@tanstack/vue-query"
import { isDuckError } from "@/core/duckd"
import { fakerApi } from "./api"
import type { Preview } from "./api"
import { useFakerI18n } from "./i18n"

const errorCodes = [
  "engine_failed",
  "configuration_changed",
  "power_changed",
  "global_ack_required",
  "backup_missing",
  "backup_stale",
  "no_targets",
  "standalone_conflict",
  "startup_not_ready",
  "recovery_pending",
  "invalid_config",
  "invalid_package",
  "config_too_large",
  "density_range",
] as const
export function useFakerEditor() {
  const { t } = useFakerI18n()
  const status = useQuery({ queryKey: ["device-faker.status"], queryFn: fakerApi.status })
  const data = computed(() => status.data.value)
  const stored = useSessionStorage<{ content: string; revision: string } | null>(
    "duck-toolbox/device-faker/draft",
    null,
    { serializer: { read: (value) => JSON.parse(value), write: (value) => JSON.stringify(value) } },
  )
  const draft = ref("")
  const base = ref("")
  const baseRevision = ref("")
  const initialized = ref(false)
  const dirty = computed(() => draft.value !== base.value)
  const changed = computed(() => !!data.value && baseRevision.value !== data.value.config.revision)
  const preview = ref<Preview | null>(null)
  const busy = ref(false)
  const stopping = ref(false)
  const error = ref("")
  const errorDetails = ref("")
  const notice = ref("")
  const confirm = ref(false)
  const discard = ref(false)
  const importConfirm = ref(false)
  const pendingFile = ref("")
  const previewRegion = ref<HTMLElement>()
  function message(caught: unknown): string {
    const detail =
      isDuckError(caught) && typeof caught.details === "object" && caught.details !== null
        ? (caught.details as { code?: unknown; params?: Record<string, unknown> })
        : undefined
    const code = errorCodes.find((code) => code === detail?.code)
    return t(`errors.${code ?? "engine_failed"}`, detail?.params)
  }
  watch(
    () => data.value?.config,
    (config) => {
      if (!config) return
      if (!initialized.value) {
        const value = stored.value
        const cached =
          value && typeof value.content === "string" && typeof value.revision === "string"
            ? value
            : null
        base.value = config.content
        draft.value =
          typeof cached?.content === "string" && typeof cached.revision === "string"
            ? cached.content
            : config.content
        baseRevision.value = cached?.revision || config.revision
        initialized.value = true
      } else if (!dirty.value) {
        base.value = config.content
        draft.value = config.content
        baseRevision.value = config.revision
      }
    },
    { immediate: true },
  )
  watch(draft, () => {
    preview.value = null
    if (initialized.value && new TextEncoder().encode(draft.value).length <= 48 * 1024) {
      stored.value = dirty.value ? { content: draft.value, revision: baseRevision.value } : null
    }
  })
  async function action(work: () => Promise<unknown>) {
    if (busy.value) return
    busy.value = true
    error.value = ""
    notice.value = ""
    errorDetails.value = ""
    try {
      await work()
    } catch (caught) {
      error.value = message(caught)
      errorDetails.value = String(caught)
    } finally {
      busy.value = false
    }
  }
  async function refresh() {
    await status.refetch()
  }
  async function show(candidate: Preview) {
    preview.value = candidate
    await nextTick()
    previewRegion.value?.focus({ preventScroll: true })
    previewRegion.value?.scrollIntoView({ block: "nearest", behavior: "instant" })
  }
  async function prepare() {
    const content = draft.value
    if (new TextEncoder().encode(content).length > 48 * 1024) {
      error.value = t("tooLarge")
      return
    }
    const candidate = await fakerApi.preview(content)
    if (draft.value === content) await show(candidate)
  }
  async function importLegacy() {
    const candidate = await fakerApi.importPreview()
    await show(candidate)
  }
  async function restore() {
    const candidate = await fakerApi.restorePreview()
    await show(candidate)
  }
  async function apply() {
    const candidate = preview.value
    if (!candidate) return
    const saved = await fakerApi.apply(candidate, candidate.summary.global_effects)
    base.value = saved.content
    baseRevision.value = saved.revision
    draft.value = saved.content
    stored.value = null
    preview.value = null
    notice.value = t("restartHelp")
    await refresh()
  }
  async function enable() {
    if (!data.value) return
    await fakerApi.enable(data.value.config.revision, data.value.stop_revision)
    notice.value = t("restartHelp")
    await refresh()
  }
  async function disable() {
    if (stopping.value) return
    stopping.value = true
    error.value = ""
    try {
      await fakerApi.disable()
      notice.value = t("stopHelp")
      await refresh()
    } catch (caught) {
      error.value = message(caught)
      errorDetails.value = String(caught)
    } finally {
      stopping.value = false
    }
  }
  function reload() {
    if (!data.value) return
    base.value = data.value.config.content
    baseRevision.value = data.value.config.revision
    draft.value = base.value
    stored.value = null
    preview.value = null
  }
  async function loadFile(event: Event) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]
    if (!file) return
    await action(async () => {
      if (file.size > 48 * 1024) {
        error.value = t("tooLarge")
        return
      }
      const content = await file.text()
      if (dirty.value) {
        pendingFile.value = content
        importConfirm.value = true
      } else {
        draft.value = content
      }
    })
    input.value = ""
  }
  function importFileDraft() {
    draft.value = pendingFile.value
    pendingFile.value = ""
  }
  return {
    status,
    data,
    draft,
    dirty,
    changed,
    preview,
    busy,
    stopping,
    error,
    errorDetails,
    notice,
    confirm,
    discard,
    previewRegion,
    importConfirm,
    importFileDraft,
    message,
    action,
    refresh,
    prepare,
    importLegacy,
    restore,
    apply,
    enable,
    disable,
    reload,
    loadFile,
  }
}
