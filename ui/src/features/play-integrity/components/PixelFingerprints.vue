<script setup lang="ts">
import { computed, onScopeDispose, ref, watch } from "vue"
import { useQuery } from "@tanstack/vue-query"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import SectionCard from "@/core/components/SectionCard.vue"
import QueryState from "@/core/components/QueryState.vue"
import { notifyError } from "@/core/notify"
import { pixelApi } from "../pixel-api"
import type { Preview } from "../api"
import { useIntegrityI18n } from "../i18n"
const emit = defineEmits<{ preview: [value: Preview]; invalidate: [] }>()
const { t } = useIntegrityI18n()
const devices = useQuery({ queryKey: ["play-integrity.pixel-devices"], queryFn: pixelApi.devices })
const device = ref("bluejay"),
  build = ref("")
const search = ref("")
const operation = ref<"fetch" | "preview" | null>(null)
const busy = computed(() => operation.value !== null)
const catalog = ref<Awaited<ReturnType<typeof pixelApi.builds>> | null>(null)
const selectedDevice = computed(() =>
  devices.data.value?.devices.find((item) => item.id === device.value),
)
const filteredDevices = computed(() => {
  const query = search.value.trim().toLowerCase()
  return (
    devices.data.value?.devices.filter((item) =>
      `${item.name} ${item.id}`.toLowerCase().includes(query),
    ) ?? []
  )
})
const buildKey = (item: { id: string; incremental: string }) => `${item.id}/${item.incremental}`
const selectedBuild = computed(() =>
  catalog.value?.builds.find((item) => buildKey(item) === build.value),
)
let active = true
onScopeDispose(() => {
  active = false
})
watch(
  device,
  () => {
    catalog.value = null
    build.value = ""
  },
  { flush: "sync" },
)
watch([device, build], () => emit("invalidate"), { flush: "sync" })
async function work(preview = false) {
  if (busy.value) return
  const product = device.value
  const selected = selectedBuild.value
  if (preview && !selected) return
  operation.value = preview ? "preview" : "fetch"
  emit("invalidate")
  if (!preview) {
    catalog.value = null
    build.value = ""
  }
  try {
    if (preview && selected) {
      const candidate = await pixelApi.preview(product, selected.id, selected.incremental)
      if (active && product === device.value && buildKey(selected) === build.value)
        emit("preview", candidate)
    } else {
      const fetched = await pixelApi.builds(product)
      if (!active || product !== device.value) return
      catalog.value = fetched
      const first = fetched.builds.find((b) => !b.notes) || fetched.builds[0]
      build.value = first ? buildKey(first) : ""
    }
  } catch (error) {
    if (active) notifyError(error)
  } finally {
    operation.value = null
  }
}
</script>
<template>
  <SectionCard :title="t('pixel.title')" :description="t('pixel.help')">
    <QueryState
      :loading="devices.isPending.value"
      :error="devices.error.value"
      @retry="devices.refetch()"
    >
      <div class="flex min-w-0 flex-col gap-4" :aria-busy="busy">
        <Field>
          <FieldLabel for="pixel-search">{{ t("pixel.search") }}</FieldLabel>
          <Input
            id="pixel-search"
            v-model="search"
            type="search"
            :disabled="busy"
            autocomplete="off"
          />
        </Field>
        <Field
          ><FieldLabel for="pixel-device">{{ t("pixel.device") }}</FieldLabel>
          <Select v-model="device" :disabled="busy || !filteredDevices.length"
            ><SelectTrigger id="pixel-device" class="w-full"
              ><SelectValue>{{ selectedDevice?.name }}</SelectValue></SelectTrigger
            >
            <SelectContent
              ><SelectItem v-for="item in filteredDevices" :key="item.id" :value="item.id"
                ><bdi dir="ltr">{{ item.name }} · {{ item.id }}</bdi></SelectItem
              ></SelectContent
            >
          </Select>
        </Field>
        <p v-if="!filteredDevices.length" role="status" class="text-muted-foreground text-sm">
          {{ t("pixel.noDevices") }}
        </p>
        <p class="text-muted-foreground text-sm">{{ t("pixel.availability") }}</p>
        <Button class="self-start" :disabled="busy" @click="work()">{{
          operation === "fetch" ? t("pixel.loading") : t("pixel.fetch")
        }}</Button>
        <p v-if="busy" role="status" aria-live="polite" class="text-muted-foreground text-sm">
          {{ operation === "preview" ? t("archive.preparing") : t("pixel.loading") }}
        </p>
        <template v-if="catalog">
          <Field
            ><FieldLabel for="pixel-build">{{ t("pixel.build") }}</FieldLabel>
            <Select v-model="build" :disabled="busy"
              ><SelectTrigger id="pixel-build" class="w-full"><SelectValue /></SelectTrigger>
              <SelectContent
                ><SelectItem
                  v-for="item in catalog.builds"
                  :key="buildKey(item)"
                  :value="buildKey(item)"
                  ><bdi dir="ltr"
                    >{{ item.id }} / {{ item.incremental }} · Android {{ item.release }}
                    {{ item.notes }}</bdi
                  ></SelectItem
                ></SelectContent
              >
            </Select>
          </Field>
          <p dir="ltr" class="text-muted-foreground break-all font-mono text-xs">
            {{ selectedBuild?.fingerprint }}
          </p>
          <p class="text-muted-foreground text-sm">{{ t("pixel.patchUnknown") }}</p>
          <Button class="self-start" :disabled="busy || !selectedBuild" @click="work(true)">{{
            operation === "preview" ? t("archive.preparing") : t("pixel.preview")
          }}</Button>
        </template>
      </div>
    </QueryState>
  </SectionCard>
</template>
