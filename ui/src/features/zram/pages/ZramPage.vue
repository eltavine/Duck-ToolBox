<script setup lang="ts">
import { computed, ref } from "vue"
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query"
import { LoaderCircle, RefreshCw } from "@lucide/vue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { formatBytes } from "@/core/format"
import { errorTitle, showErrorDetails, notifySuccess } from "@/core/notify"
import { zramApi } from "../api"
import type { ZramConfig, ZramDevice, ZramResult } from "../api"
import ConfigEditor from "../components/ConfigEditor.vue"
import DeviceStats from "../components/DeviceStats.vue"
import { useZramI18n } from "../i18n"

const { t, dynamic, locale } = useZramI18n()
const client = useQueryClient()
const pageRoot = ref<HTMLElement | null>(null)
const status = useQuery({ queryKey: ["zram"], queryFn: zramApi.status })
const selected = ref("")
const device = computed(
  () =>
    status.data.value?.devices.find((d) => d.name === selected.value) ??
    status.data.value?.devices[0],
)
const profile = computed(() =>
  status.data.value?.saved.profiles.find((p) => p.device === device.value?.name),
)
const original = computed(() =>
  status.data.value?.saved.originals.find(
    (p) =>
      p.device === device.value?.name &&
      p.boot_id === status.data.value?.boot_id &&
      p.generation === device.value?.generation,
  ),
)
const writable = computed(
  () =>
    !!status.data.value?.operations_supported &&
    !status.isError.value &&
    !status.data.value?.operation_busy &&
    device.value?.blocked.length === 0,
)
type Pending = {
  kind: "apply" | "save" | "restore"
  device: ZramDevice
  config: ZramConfig
  boot: boolean
}
const pending = ref<Pending | null>(null)
const confirmOpen = ref(false)
let accepted = false
const comparison = ref<ZramResult | null>(null)
const operation = useMutation({
  mutationFn: async (
    request: Pending | { kind: "forget"; device: Pick<ZramDevice, "name"> },
  ): Promise<ZramResult | null> => {
    switch (request.kind) {
      case "apply":
        return zramApi.apply(request.device, request.config)
      case "restore":
        return zramApi.restore(request.device)
      case "save":
        await zramApi.save(request.device, request.config, request.boot)
        return null
      case "forget":
        await zramApi.forget(request.device.name)
        return null
    }
  },
  onError: () => undefined,
  onSuccess: (data) => {
    if (data) comparison.value = data
    notifySuccess(t("done"))
  },
  onSettled: async (_data, error, request) => {
    await client.invalidateQueries({ queryKey: ["zram"] })
    if (!error && request.kind === "forget") pageRoot.value?.focus({ preventScroll: true })
  },
})
function request(kind: "apply" | "save" | "restore", config: ZramConfig, boot = false) {
  if (!device.value || !writable.value || operation.isPending.value) return
  pending.value = {
    kind,
    config: { ...config },
    device: {
      ...device.value,
      stats: { ...device.value.stats },
      algorithms: [...device.value.algorithms],
      blocked: [...device.value.blocked],
    },
    boot,
  }
  confirmOpen.value = true
}
function confirm() {
  if (!pending.value || !writable.value || operation.isPending.value) return
  accepted = true
  operation.mutate(pending.value)
}
function restoreFocus(event: Event) {
  const targetChanged = pending.value?.device.generation !== device.value?.generation
  pending.value = null
  if (!accepted && !targetChanged) return
  accepted = false
  event.preventDefault()
  pageRoot.value?.focus({ preventScroll: true })
}
const confirmRows = computed(() =>
  pending.value
    ? [
        { label: t("device"), value: pending.value.device.name, mono: true },
        {
          label: t("sizeInput"),
          value: new Intl.NumberFormat(locale.value, { maximumFractionDigits: 20 }).format(
            pending.value.config.size_bytes / 1048576,
          ),
        },
        { label: t("algorithm"), value: pending.value.config.algorithm, mono: true },
        {
          label: t("memoryLimit"),
          value:
            pending.value.config.memory_limit === 0
              ? t("unlimited")
              : new Intl.NumberFormat(locale.value, {
                  style: "unit",
                  unit: "byte",
                  unitDisplay: "short",
                  maximumFractionDigits: 0,
                }).format(pending.value.config.memory_limit),
        },
        {
          label: t("priority"),
          value:
            pending.value.config.priority === null
              ? t("automatic")
              : new Intl.NumberFormat(locale.value).format(pending.value.config.priority),
        },
        ...(pending.value.kind === "save"
          ? [{ label: t("boot"), value: t(pending.value.boot ? "bootOn" : "bootOff") }]
          : []),
      ]
    : [],
)
const unavailablePlans = computed(
  () =>
    status.data.value?.saved.profiles.filter(
      (p) => !status.data.value?.devices.some((d) => d.name === p.device),
    ) ?? [],
)
const bootResults = computed(
  () =>
    status.data.value?.saved.boot_results.filter((r) => r.boot_id === status.data.value?.boot_id) ??
    [],
)
function bytes(value: number | null | undefined) {
  return value === undefined || value === null ? "—" : formatBytes(value)
}
function count(value: number | null | undefined) {
  return value === undefined || value === null
    ? "—"
    : new Intl.NumberFormat(locale.value).format(value)
}
</script>

<template>
  <div
    ref="pageRoot"
    tabindex="-1"
    data-testid="zram-page"
    class="flex min-w-0 flex-col gap-6 p-4 outline-none"
  >
    <SectionCard :title="t('overview')" :description="t('overviewHelp')">
      <template #action>
        <Button
          variant="ghost"
          size="icon"
          class="min-h-12 min-w-12"
          data-zram-refresh
          :aria-label="t('refresh')"
          :disabled="status.isFetching.value || operation.isPending.value || confirmOpen"
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
        <KeyValueList
          :rows="[
            { label: t('memoryTotal'), value: bytes(status.data.value?.memory_total) },
            { label: t('available'), value: bytes(status.data.value?.memory_available) },
            { label: t('swapIn'), value: count(status.data.value?.swap_in_pages) },
            { label: t('swapOut'), value: count(status.data.value?.swap_out_pages) },
          ]"
        />
      </QueryState>
      <p
        v-if="status.data.value && !status.data.value.operations_supported"
        class="text-muted-foreground mt-4 text-sm"
      >
        {{ t("unsupported") }}
      </p>
      <p
        v-if="status.data.value?.operation_busy"
        role="status"
        class="text-muted-foreground mt-4 text-sm"
      >
        {{ t("errors.operation_busy") }}
      </p>
      <div v-if="status.data.value?.warnings.length" class="mt-4 text-sm" role="status">
        <p>{{ t("incomplete") }}</p>
        <details class="mt-2 min-w-0">
          <summary class="text-primary min-h-12 cursor-pointer py-3">{{ t("details") }}</summary>
          <ul class="space-y-2">
            <li
              v-for="warning in status.data.value.warnings"
              :key="warning"
              class="font-mono text-xs break-words"
              dir="ltr"
            >
              {{ warning }}
            </li>
          </ul>
        </details>
      </div>
    </SectionCard>
    <div
      v-if="operation.isPending.value"
      role="status"
      class="bg-secondary flex items-center gap-3 rounded-lg p-4 text-sm"
    >
      <LoaderCircle aria-hidden="true" class="size-5 shrink-0 motion-safe:animate-spin" />{{
        t("running")
      }}
    </div>
    <SectionCard
      v-if="operation.error.value"
      :title="errorTitle(operation.error.value)"
      role="alert"
    >
      <p class="mb-3 text-sm">
        {{
          t(
            operation.variables.value?.kind === "apply" ||
              operation.variables.value?.kind === "restore"
              ? "partialFailure"
              : "planFailure",
          )
        }}
      </p>
      <Button variant="outline" class="min-h-12" @click="showErrorDetails(operation.error.value)">{{
        t("details")
      }}</Button>
    </SectionCard>
    <template v-if="status.data.value">
      <SectionCard
        v-if="!status.data.value.devices.length && !status.data.value.warnings.length"
        :title="t('device')"
        ><p class="text-muted-foreground text-sm">{{ t("empty") }}</p></SectionCard
      >
      <template v-if="device">
        <div v-if="status.data.value.devices.length > 1" class="space-y-2">
          <Label for="zram-device">{{ t("device") }}</Label>
          <Select
            :model-value="device.name"
            :disabled="operation.isPending.value"
            @update:model-value="selected = String($event)"
          >
            <SelectTrigger id="zram-device" class="w-full"><SelectValue /></SelectTrigger>
            <SelectContent
              ><SelectItem
                v-for="item in status.data.value.devices"
                :key="item.name"
                :value="item.name"
                ><bdi dir="ltr">{{ item.name }}</bdi></SelectItem
              ></SelectContent
            >
          </Select>
        </div>
        <SectionCard :title="device.name">
          <template #action
            ><Badge variant="secondary">{{
              t(device.swap ? "active" : "inactive")
            }}</Badge></template
          >
          <DeviceStats :device="device" />
          <p
            v-for="reason in device.blocked"
            :key="reason"
            class="text-muted-foreground mt-3 text-sm"
          >
            {{ dynamic(`blocked.${reason}`, t("protected")) }}
          </p>
        </SectionCard>
        <SectionCard :title="t('plan')">
          <ConfigEditor
            :key="`${device.name}:${device.generation}`"
            :device="device"
            :profile="profile"
            :total="status.data.value.memory_total"
            :writable="writable"
            :busy="operation.isPending.value || status.data.value.operation_busy"
            :can-restore="!!original"
            @request="request"
            @restore="original && request('restore', original.config)"
            @forget="operation.mutate({ kind: 'forget', device })"
          />
        </SectionCard>
      </template>
      <SectionCard v-if="unavailablePlans.length" :title="t('unavailablePlans')" plain>
        <div
          v-for="plan in unavailablePlans"
          :key="plan.device"
          class="bg-card mb-0.5 flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-xs p-4 first:rounded-t-lg last:rounded-b-lg"
        >
          <div class="min-w-0">
            <p class="font-mono text-sm break-all">
              <bdi dir="ltr">{{ plan.device }}</bdi>
            </p>
            <p class="text-muted-foreground mt-1 text-sm">
              {{ t(plan.apply_at_boot ? "bootEnabled" : "bootDisabled") }}
            </p>
          </div>
          <Button
            variant="outline"
            class="min-h-12"
            :aria-label="`${t('forget')} ${plan.device}`"
            :disabled="operation.isPending.value || status.data.value.operation_busy"
            @click="operation.mutate({ kind: 'forget', device: { name: plan.device } })"
            >{{ t("forget") }}</Button
          >
        </div>
      </SectionCard>
      <SectionCard v-if="comparison" :title="t('comparison')" :description="t('comparisonHelp')">
        <KeyValueList
          :rows="[
            { label: t('device'), value: comparison.device, mono: true },
            { label: t('before'), value: bytes(comparison.before.memory_used) },
            { label: t('after'), value: bytes(comparison.after.memory_used) },
          ]"
        />
      </SectionCard>
      <SectionCard v-if="bootResults.length" :title="t('bootResult')">
        <div v-for="result in bootResults" :key="result.device" class="mb-3 last:mb-0">
          <p class="text-sm">
            <bdi dir="ltr" class="font-mono">{{ result.device }}</bdi> ·
            {{
              result.code === "applied"
                ? t("done")
                : dynamic(`errors.${result.code}`, t("protected"))
            }}
          </p>
          <details v-if="result.message" class="mt-2 text-sm">
            <summary class="min-h-12 cursor-pointer py-3">{{ t("details") }}</summary>
            <p dir="auto" class="text-muted-foreground break-all">{{ result.message }}</p>
          </details>
        </div>
      </SectionCard>
      <SectionCard :title="t('swaps')" :description="t('swapsHelp')" plain>
        <div
          v-for="swap in status.data.value.swaps"
          :key="swap.path"
          class="bg-card mb-0.5 min-w-0 rounded-xs p-4 first:rounded-t-lg last:rounded-b-lg"
        >
          <p class="mb-3 font-mono text-sm break-all">
            <bdi dir="ltr">{{ swap.path }}</bdi>
          </p>
          <KeyValueList
            :rows="[
              { label: t('size'), value: bytes(swap.size) },
              { label: t('used'), value: bytes(swap.used) },
              { label: t('priority'), value: count(swap.priority) },
            ]"
          />
        </div>
        <p v-if="!status.data.value.swaps.length" class="text-muted-foreground p-4 text-sm">
          {{ t("empty") }}
        </p>
      </SectionCard>
    </template>
    <ConfirmDialog
      v-model:open="confirmOpen"
      :title="t(pending?.kind ?? 'apply')"
      :confirm-label="t(pending?.kind ?? 'apply')"
      :description="t(pending?.kind === 'save' ? 'saveConfirm' : 'applyConfirm')"
      :destructive="pending?.kind !== 'save'"
      @confirm="confirm"
      @close-auto-focus="restoreFocus"
    >
      <KeyValueList :rows="confirmRows" />
    </ConfirmDialog>
  </div>
</template>
