<script setup lang="ts">
import type { ZramDevice } from "../api"
import { useZramI18n } from "../i18n"
import KeyValueList from "@/core/components/KeyValueList.vue"
import { formatBytes } from "@/core/format"
const props = defineProps<{ device: ZramDevice }>()
const { t, locale } = useZramI18n()
function bytes(n: number | null) {
  return n === null ? "—" : formatBytes(n)
}
function ratio() {
  const { original, compressed } = props.device.stats
  return original && compressed !== null
    ? new Intl.NumberFormat(locale.value, { style: "percent", maximumFractionDigits: 1 }).format(
        compressed / original,
      )
    : "—"
}
</script>
<template>
  <KeyValueList
    :rows="[
      { label: t('size'), value: bytes(device.size) },
      { label: t('used'), value: bytes(device.swap?.used ?? null) },
      { label: t('algorithm'), value: device.algorithm, mono: true },
      {
        label: t('priority'),
        value: device.swap ? new Intl.NumberFormat(locale).format(device.swap.priority) : '—',
      },
      { label: t('original'), value: bytes(device.stats.original) },
      { label: t('compressed'), value: bytes(device.stats.compressed) },
      { label: t('memoryUsed'), value: bytes(device.stats.memory_used) },
      {
        label: t('memoryLimit'),
        value: device.stats.memory_limit === 0 ? t('unlimited') : bytes(device.stats.memory_limit),
      },
      { label: t('ratio'), value: ratio() },
    ]"
  />
</template>
