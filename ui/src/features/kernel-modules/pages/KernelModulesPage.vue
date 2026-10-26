<script setup lang="ts">
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query"
import { LoaderCircle, RefreshCw } from "@lucide/vue"
import { computed, ref } from "vue"
import { I18nT } from "vue-i18n"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { formatBytes } from "@/core/format"
import { errorTitle, notifySuccess, showErrorDetails } from "@/core/notify"

import type { KernelModule, ModuleFile } from "../api"
import { kernelModulesApi } from "../api"
import KernelFiles from "../components/KernelFiles.vue"
import ModuleDetails from "../components/ModuleDetails.vue"
import { useModulePresentation } from "../presentation"

const { t, stateLabel, number } = useModulePresentation()
const pageRoot = ref<HTMLElement | null>(null)
let accepted = false
const client = useQueryClient()
const status = useQuery({
  queryKey: ["kernel-modules", "status"],
  queryFn: kernelModulesApi.status,
})
const search = ref("")
const selected = ref<string | null>(null)
const tab = ref("runtime")
const modules = computed(
  () =>
    status.data.value?.modules.filter((item) =>
      item.name.toLowerCase().includes(search.value.trim().toLowerCase()),
    ) ?? [],
)
const writable = computed(
  () =>
    !!status.data.value?.operations_supported &&
    !status.isError.value &&
    status.data.value.proc_available &&
    status.data.value.modules_disabled !== true,
)
type Operation =
  { action: "load"; file: ModuleFile; parameters: string } | { action: "unload"; name: string }
const operation = useMutation({
  mutationFn: (request: Operation) =>
    request.action === "load"
      ? kernelModulesApi.load(request.file, request.parameters)
      : kernelModulesApi.unload(request.name),
  onError: () => undefined,
  onSettled: async (_data, error) => {
    await client.invalidateQueries({ queryKey: ["kernel-modules"] })
    if (!error) notifySuccess(t("operation.accepted"))
  },
})
const runningName = computed(() =>
  operation.variables.value?.action === "load"
    ? operation.variables.value.file.name
    : (operation.variables.value?.name ?? ""),
)
const loadedCount = computed(
  () => status.data.value?.modules.filter((m) => m.source === "loaded").length ?? 0,
)
const pending = ref<Operation | null>(null)
const confirmOpen = computed({
  get: () => pending.value !== null,
  set: (open) => {
    if (!open) pending.value = null
  },
})
const confirmation = computed(() =>
  pending.value?.action === "load"
    ? t("files.loadConfirm", { name: pending.value.file.name })
    : pending.value
      ? t("runtime.unloadConfirm", { name: pending.value.name })
      : "",
)

function canUnload(item: KernelModule) {
  return (
    writable.value &&
    item.source === "loaded" &&
    item.state === "Live" &&
    item.references === 0 &&
    item.used_by.length === 0
  )
}

function unload(item: KernelModule) {
  if (!canUnload(item) || operation.isPending.value) return
  pending.value = { action: "unload", name: item.name }
}

function load(file: ModuleFile, parameters: string) {
  if (!writable.value || operation.isPending.value) return
  pending.value = { action: "load", file, parameters }
}

function acceptOperation() {
  if (pending.value && writable.value && !operation.isPending.value) {
    accepted = true
    operation.mutate(pending.value)
  }
}

function restoreOperationFocus(event: Event) {
  if (!accepted) return
  accepted = false
  // The mutation can disable or remove its trigger before the dialog closes.
  event.preventDefault()
  pageRoot.value?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus()
}
</script>

<template>
  <div ref="pageRoot" class="flex min-w-0 flex-col gap-6 p-4">
    <SectionCard :title="t('overview.title')" :description="t('operation.currentBoot')">
      <template #action>
        <Button
          variant="ghost"
          size="icon"
          class="min-h-12 min-w-12"
          :aria-label="t('refresh')"
          :disabled="status.isFetching.value"
          @click="status.refetch()"
        >
          <RefreshCw :class="{ 'motion-safe:animate-spin': status.isFetching.value }" />
        </Button>
      </template>
      <QueryState
        :loading="status.isPending.value"
        :error="status.error.value"
        @retry="status.refetch()"
      >
        <p class="text-muted-foreground text-xs">{{ t("overview.kernel") }}</p>
        <p class="mt-1 font-mono text-sm break-all">
          <bdi dir="ltr">{{ status.data.value?.kernel_release ?? "—" }}</bdi>
        </p>
        <p v-if="status.data.value" class="text-muted-foreground mt-3 text-sm">
          {{
            t("overview.counts", {
              loaded: number(loadedCount),
              sysfs: number(status.data.value.modules.length - loadedCount),
            })
          }}
        </p>
      </QueryState>
      <p v-if="status.data.value?.modules_disabled" class="text-destructive mt-2 text-sm">
        {{ t("errors.modules_disabled") }}
      </p>
      <p v-else-if="status.data.value && !writable" class="text-muted-foreground mt-2 text-sm">
        {{ t("runtime.unavailable") }}
      </p>
      <p
        v-for="warning in status.data.value?.warnings"
        :key="warning"
        class="text-destructive mt-2 text-xs break-all"
        dir="auto"
      >
        {{ warning }}
      </p>
    </SectionCard>

    <div
      v-if="operation.isPending.value"
      role="status"
      aria-live="polite"
      class="bg-secondary flex items-center gap-3 rounded-lg p-4 text-sm"
      data-testid="kernel-operation-pending"
    >
      <LoaderCircle aria-hidden="true" class="size-5 shrink-0 motion-safe:animate-spin" />
      <I18nT
        :keypath="
          operation.variables.value?.action === 'load'
            ? 'kernelModules.operation.loading'
            : 'kernelModules.operation.unloading'
        "
        tag="span"
        scope="global"
      >
        <template #name
          ><bdi dir="ltr">{{ runningName }}</bdi></template
        >
      </I18nT>
    </div>
    <SectionCard
      v-if="operation.error.value"
      :title="errorTitle(operation.error.value)"
      role="alert"
      data-testid="kernel-operation-error"
    >
      <p class="mb-3 text-sm break-all">
        <bdi dir="ltr">{{
          operation.variables.value?.action === "load"
            ? operation.variables.value.file.path
            : operation.variables.value?.name
        }}</bdi>
      </p>
      <div class="flex flex-wrap gap-2">
        <Button variant="outline" @click="showErrorDetails(operation.error.value)">{{
          t("operation.details")
        }}</Button>
        <Button :disabled="status.isFetching.value" @click="status.refetch()">{{
          t("operation.refresh")
        }}</Button>
      </div>
    </SectionCard>

    <Tabs v-model="tab">
      <TabsList class="w-full">
        <TabsTrigger value="runtime" class="flex-1">{{ t("tabs.runtime") }}</TabsTrigger>
        <TabsTrigger value="files" class="flex-1">{{ t("tabs.files") }}</TabsTrigger>
      </TabsList>
      <TabsContent
        value="runtime"
        force-mount
        :class="['mt-4 flex flex-col gap-4', { hidden: tab !== 'runtime' }]"
      >
        <p class="text-muted-foreground text-sm">{{ t("runtime.description") }}</p>
        <Input v-model="search" :aria-label="t('search')" :placeholder="t('search')" />
        <QueryState
          :loading="status.isPending.value"
          :error="status.error.value"
          @retry="status.refetch()"
        >
          <p v-if="!modules.length" class="text-muted-foreground py-6 text-center text-sm">
            {{ t("runtime.empty") }}
          </p>
          <div v-else class="@container flex flex-col gap-0.5" role="list">
            <div
              v-for="item in modules"
              :key="item.name"
              class="bg-card flex min-w-0 flex-col items-start gap-2 rounded-xs p-4 first:rounded-t-lg last:rounded-b-lg @min-[18rem]:flex-row"
              role="listitem"
              :data-module="item.name"
            >
              <button
                class="state-layer focus-visible:ring-ring/50 relative min-h-12 w-full min-w-0 flex-1 rounded-md text-start outline-none focus-visible:ring-3 @min-[18rem]:w-auto"
                :aria-describedby="`kernel-meta-${item.name}`"
                :aria-label="t('runtime.details', { name: item.name })"
                @click="selected = item.name"
              >
                <span class="block font-mono text-sm font-semibold break-all"
                  ><bdi dir="ltr">{{ item.name }}</bdi></span
                >
                <span
                  :id="`kernel-meta-${item.name}`"
                  class="text-muted-foreground mt-1 flex flex-wrap items-center gap-2 text-xs"
                >
                  <Badge variant="secondary">{{
                    t(item.source === "loaded" ? "runtime.loaded" : "runtime.sysfsOnly")
                  }}</Badge>
                  <span v-if="item.size !== null">{{ formatBytes(item.size) }}</span>
                  <span v-if="item.references !== null && item.references >= 0">{{
                    t("runtime.references", { count: number(item.references) })
                  }}</span>
                  <span v-else-if="item.source === 'loaded'">{{
                    t("runtime.referencesUnknown")
                  }}</span>
                  <span v-if="item.state">{{ stateLabel(item.state) }}</span>
                  <span v-if="item.used_by.includes('[permanent]')">{{
                    t("runtime.permanent")
                  }}</span>
                </span>
                <span
                  v-if="item.used_by.some((name) => name !== '[permanent]')"
                  class="text-muted-foreground mt-1 block text-xs break-all"
                  >{{ t("runtime.usedBy") }}:
                  <bdi dir="ltr">{{
                    item.used_by.filter((name) => name !== "[permanent]").join(", ")
                  }}</bdi></span
                >
              </button>
              <Button
                v-if="item.source === 'loaded'"
                variant="outline"
                class="min-h-12 shrink-0 self-end text-destructive @min-[18rem]:self-auto"
                :aria-label="t('runtime.unloadNamed', { name: item.name })"
                :aria-describedby="!canUnload(item) ? `kernel-blocked-${item.name}` : undefined"
                :disabled="!canUnload(item) || operation.isPending.value"
                @click="unload(item)"
                >{{ t("runtime.unload") }}</Button
              >
              <span v-if="!canUnload(item)" :id="`kernel-blocked-${item.name}`" class="sr-only">{{
                t("runtime.unloadBlocked")
              }}</span>
            </div>
          </div>
        </QueryState>
      </TabsContent>
      <TabsContent value="files" force-mount :class="['mt-4', { hidden: tab !== 'files' }]">
        <KernelFiles
          :writable="writable"
          :active="tab === 'files'"
          :busy="operation.isPending.value"
          :loaded-names="status.data.value?.modules.map((m) => m.name) ?? []"
          @load="load"
        />
      </TabsContent>
    </Tabs>
    <ModuleDetails :name="selected" @close="selected = null" />
    <ConfirmDialog
      v-model:open="confirmOpen"
      :title="t(pending?.action === 'load' ? 'files.load' : 'runtime.unload')"
      :description="confirmation"
      destructive
      @confirm="acceptOperation"
      @close-auto-focus="restoreOperationFocus"
    >
      <KeyValueList
        v-if="pending"
        :rows="
          pending.action === 'load'
            ? [
                { label: t('files.path'), value: pending.file.path, mono: true },
                {
                  label: t('files.parameters'),
                  value: pending.parameters || t('files.noParameters'),
                  mono: pending.parameters.length > 0,
                },
                { label: 'SHA-256', value: pending.file.sha256, mono: true },
              ]
            : [{ label: t('metadata.name'), value: pending.name, mono: true }]
        "
      />
    </ConfirmDialog>
  </div>
</template>
