import { useQuery, useQueryClient } from "@tanstack/vue-query"
import { useStorage } from "@vueuse/core"
import { effectScope, onScopeDispose, ref, watch } from "vue"
import { contributions } from "@/core/features"
import type { SecurityPatchPreview, SecurityPatchTarget } from "@/core/features"
import { notifyError, notifySuccess } from "@/core/notify"
import { integrityApi } from "./api"
import { libraryApi } from "./library-api"
import type { Options, Preview, Task } from "./api"
import { useIntegrityI18n } from "./i18n"

const TASK_STORAGE_KEY = "duck-toolbox/play-integrity-task"

export function useIntegrity() {
  const { t } = useIntegrityI18n()
  const client = useQueryClient()
  const status = useQuery({
    queryKey: ["play-integrity.status"],
    queryFn: integrityApi.status,
    refetchOnWindowFocus: false,
  })
  const config = useQuery({
    queryKey: ["play-integrity.config"],
    queryFn: integrityApi.config,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  })
  const transaction = useQuery({
    queryKey: ["integrity-transaction.status"],
    queryFn: integrityApi.transactionStatus,
    refetchOnWindowFocus: true,
  })
  const content = ref("")
  const preview = ref<Preview | null>(null)
  const patches = ref<
    { target: SecurityPatchTarget; preview: SecurityPatchPreview; selected: boolean }[]
  >([])
  const task = ref<Task | null>(null)
  const taskId = useStorage(TASK_STORAGE_KEY, "")
  const backup = useStorage("duck-toolbox/play-integrity-backup", "")
  const options = ref<Options>({
    enabled: true,
    mode: "full",
    skipdelprop: false,
    skippersistprop: false,
    skipmatchdevice: false,
  })
  const busy = ref(false)
  const log = ref("")
  let timer: ReturnType<typeof setTimeout> | undefined
  let disposed = false
  let previewGeneration = 0
  let taskGeneration = 0
  function clearPreview() {
    previewGeneration++
    preview.value = null
    patches.value = []
  }
  watch(
    config.data,
    (value, previous) => {
      if (value && (!previous || content.value === previous.content)) content.value = value.content
    },
    { immediate: true },
  )
  watch(
    status.data,
    (value) => {
      if (value) options.value = { ...value.options }
    },
    { immediate: true },
  )
  watch(content, clearPreview, { flush: "sync" })
  async function action(work: () => Promise<unknown>) {
    if (busy.value) return
    busy.value = true
    try {
      await work()
    } catch (error) {
      notifyError(error)
    } finally {
      busy.value = false
    }
  }
  async function refresh() {
    await Promise.all([
      client.invalidateQueries({ queryKey: ["integrity-transaction.status"] }),
      client.invalidateQueries({ queryKey: ["play-integrity.status"] }),
      client.invalidateQueries({ queryKey: ["play-integrity.config"] }),
      client.invalidateQueries({ queryKey: ["play-integrity.inspection"] }),
    ])
  }
  async function prepare(value: Preview) {
    clearPreview()
    const generation = previewGeneration
    const prepared: typeof patches.value = []
    if (value.suggested_patch) {
      for (const target of contributions("securityPatchTargets")) {
        let patch: SecurityPatchPreview | null
        try {
          patch = await target.preview(value.suggested_patch)
        } catch (error) {
          if (disposed || generation !== previewGeneration) return
          throw error
        }
        if (disposed || generation !== previewGeneration) return
        if (patch) prepared.push({ target, preview: patch, selected: false })
      }
    }
    if (disposed || generation !== previewGeneration) return
    patches.value = prepared
    preview.value = value
  }
  async function requestPreview(load: () => Promise<Preview>) {
    clearPreview()
    const generation = previewGeneration
    let value: Preview
    try {
      value = await load()
    } catch (error) {
      if (!disposed && generation === previewGeneration) throw error
      return
    }
    if (!disposed && generation === previewGeneration) await prepare(value)
  }
  async function apply() {
    const candidate = preview.value
    if (!candidate) return
    const selected = patches.value.filter((item) => item.selected)
    if (selected.length > 1) throw new Error(t("patch.multipleTargets"))
    const patch = selected[0]
    try {
      const applied = await integrityApi.applyTransaction(
        candidate,
        patch
          ? {
              target: patch.target.id,
              patch: patch.preview.patch,
              revision: patch.preview.revision,
              partitions: patch.preview.partitions,
            }
          : undefined,
      )
      backup.value = applied.backup
      content.value = applied.config.content
      if (applied.restart_required)
        notifySuccess(t(patch ? "activation.reboot" : "activation.process"))
    } finally {
      await refresh()
    }
    preview.value = null
    await refresh()
    notifySuccess(t("saved"))
  }
  async function poll() {
    if (disposed || !taskId.value) return
    const id = taskId.value
    try {
      const result = await integrityApi.taskStatus(id)
      if (disposed || taskId.value !== id) return
      task.value = result
      if (task.value.state === "completed" && task.value.result) {
        if (taskGeneration === previewGeneration) await prepare(task.value.result)
        if (!disposed && taskId.value === id) taskId.value = ""
      } else if (task.value.state !== "running") {
        taskId.value = ""
      } else timer = setTimeout(() => void poll(), 1000)
    } catch (error) {
      if (disposed || taskId.value !== id) return
      notifyError(error)
      taskId.value = ""
    }
  }
  watch(
    taskId,
    (id) => {
      if (timer) clearTimeout(timer)
      if (id && !disposed) void poll()
    },
    { immediate: true, flush: "sync" },
  )
  onScopeDispose(() => {
    disposed = true
    clearPreview()
    if (timer) clearTimeout(timer)
  })
  return {
    t,
    status,
    transaction,
    config,
    content,
    preview,
    patches,
    task,
    taskId,
    backup,
    options,
    busy,
    log,
    action,
    refresh,
    apply,
    prepare,
    clearPreview,
    patchPreview: (id: string, partitions: Record<string, string>) =>
      action(async () => {
        const item = patches.value.find((p) => p.target.id === id)
        if (!item) return
        const generation = previewGeneration
        let next: SecurityPatchPreview | null
        try {
          next = await item.target.preview(item.preview.patch, partitions)
        } catch (error) {
          if (disposed || generation !== previewGeneration) return
          throw error
        }
        if (disposed || generation !== previewGeneration) return
        if (!next) throw new Error(t("patch.targetUnavailable"))
        item.preview = next
      }),
    previewContent: (migrate = false) =>
      action(() => requestPreview(() => integrityApi.preview(content.value, "auto", migrate))),
    importSource: (source: string) =>
      action(() => requestPreview(() => integrityApi.importPreview(source))),
    previewProfile: (id: string) =>
      action(() => requestPreview(() => libraryApi.profilePreview(id))),
    saveOptions: (list: string) =>
      action(async () => {
        await integrityApi.options({
          mode: options.value.mode,
          skipdelprop: options.value.skipdelprop,
          skippersistprop: options.value.skippersistprop,
          skipmatchdevice: options.value.skipmatchdevice,
          app_replace_list: list,
        })
        await refresh()
        notifySuccess(t("reboot"))
      }),
    generate: (strong: boolean, match: boolean) =>
      action(async () => {
        clearPreview()
        taskGeneration = previewGeneration
        // Keep only the pending request's storage adapter alive across page disposal.
        const requestScope = effectScope(true)
        try {
          const tracked = requestScope.run(() =>
            useStorage(TASK_STORAGE_KEY, "", undefined, { flush: "sync" }),
          )!
          const previous = tracked.value
          let result: Task
          try {
            result = await integrityApi.taskStart(strong, match)
          } catch (error) {
            if (!disposed && (!tracked.value || tracked.value === previous)) throw error
            return
          }
          if (tracked.value && tracked.value !== previous) return
          if (!disposed) task.value = result
          tracked.value = result.id
        } finally {
          requestScope.stop()
        }
      }),
    cancel: () =>
      action(async () => {
        const id = taskId.value
        if (!id) return
        const result = await integrityApi.taskCancel(id)
        if (!disposed && taskId.value === id) task.value = result
      }),
    restore: () =>
      action(async () => {
        if (backup.value) {
          const restored = await integrityApi.restore(backup.value)
          content.value = restored.config.content
          backup.value = ""
          await refresh()
        }
      }),
    restart: () =>
      action(async () => {
        const result = await integrityApi.restart()
        log.value = JSON.stringify(result.processes, null, 2)
        await refresh()
        notifySuccess(t("restartSent"))
      }),
    diagnose: () =>
      action(async () => {
        log.value = (await integrityApi.diagnose()).log
      }),
  }
}
