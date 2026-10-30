<script setup lang="ts">
import { computed, onScopeDispose, ref, shallowRef, watch } from "vue"
import { useI18n } from "vue-i18n"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import SectionCard from "@/core/components/SectionCard.vue"
import { openExternal } from "@/core/links"
import { errorTitle, notifyError, showErrorDetails } from "@/core/notify"
import SettingDetails from "./SettingDetails.vue"
import { pixelArchiveApi } from "../pixel-archive-api"
import type { ArchiveCatalog, ArchiveChannel } from "../pixel-archive-api"
import type { Preview } from "../api"
import { useIntegrityI18n } from "../i18n"

const emit = defineEmits<{ preview: [value: Preview]; invalidate: [] }>()
const { t } = useIntegrityI18n()
const global = useI18n()
const catalog = ref<ArchiveCatalog | null>(null)
const device = ref("bluejay")
const channel = ref<ArchiveChannel>("release")
const busy = ref(false)
const error = shallowRef<unknown>(null)
const selectedDevice = computed(() => catalog.value?.devices[device.value])
const selectedBuild = computed(() => selectedDevice.value?.channels[channel.value])
const channels: ArchiveChannel[] = ["release", "beta", "canary"]
let active = true
onScopeDispose(() => {
  active = false
})
watch([device, channel], () => emit("invalidate"), { flush: "sync" })
watch(selectedDevice, (value) => {
  if (value && !value.channels[channel.value])
    channel.value = channels.find((item) => value.channels[item]) ?? "release"
})
async function fetchCatalog() {
  if (busy.value) return
  busy.value = true
  error.value = null
  emit("invalidate")
  catalog.value = null
  try {
    const value = await pixelArchiveApi.catalog()
    if (!active) return
    if (!value.devices[device.value]) device.value = Object.keys(value.devices)[0] ?? ""
    catalog.value = value
  } catch (cause) {
    if (active) error.value = cause
  } finally {
    busy.value = false
  }
}
async function preview() {
  if (busy.value || !catalog.value || !selectedBuild.value) return
  busy.value = true
  emit("invalidate")
  try {
    const value = await pixelArchiveApi.preview(catalog.value, device.value, channel.value)
    if (active) emit("preview", value)
  } catch (cause) {
    if (active) notifyError(cause)
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <SectionCard :title="t('archive.title')" :description="t('archive.help')">
    <div class="flex min-w-0 flex-col gap-4" :aria-busy="busy">
      <a
        href="https://github.com/VictorGabriel7Dev/build.prop"
        class="text-primary flex min-h-12 w-fit max-w-full items-center break-all rounded-md text-sm underline underline-offset-4 focus-visible:outline-2 focus-visible:outline-offset-2"
        @click.prevent="openExternal('https://github.com/VictorGabriel7Dev/build.prop')"
        >{{ t("archive.source") }} · <bdi dir="ltr">VictorGabriel7Dev/build.prop</bdi></a
      >
      <Button
        :variant="catalog ? 'outline' : 'default'"
        class="self-start"
        :disabled="busy"
        @click="fetchCatalog"
        >{{ busy ? t("pixel.loading") : t("archive.fetch") }}</Button
      >
      <div v-if="error" class="rounded-xl border border-destructive/30 bg-destructive/5 p-4">
        <p role="alert" class="text-destructive text-sm break-words">{{ errorTitle(error) }}</p>
        <Button variant="outline" class="mt-3" @click="showErrorDetails(error)">{{
          global.t("core.actions.details")
        }}</Button>
      </div>
      <p v-if="busy" role="status" aria-live="polite" class="text-muted-foreground text-sm">
        {{ t("pixel.loading") }}
      </p>
      <template v-if="catalog">
        <Field>
          <FieldLabel for="archive-device">{{ t("pixel.device") }}</FieldLabel>
          <Select v-model="device" :disabled="busy">
            <SelectTrigger id="archive-device" class="w-full"><SelectValue /></SelectTrigger>
            <SelectContent
              ><SelectItem v-for="(item, id) in catalog.devices" :key="id" :value="id">{{
                item.model
              }}</SelectItem></SelectContent
            >
          </Select>
        </Field>
        <Field>
          <FieldLabel for="archive-channel">{{ t("archive.channel") }}</FieldLabel>
          <Select v-model="channel" :disabled="busy">
            <SelectTrigger id="archive-channel" class="w-full"><SelectValue /></SelectTrigger>
            <SelectContent
              ><SelectItem
                v-for="item in channels"
                :key="item"
                :value="item"
                :disabled="!selectedDevice?.channels[item]"
                >{{ t(`archive.${item}`) }}</SelectItem
              ></SelectContent
            >
          </Select>
        </Field>
        <dl v-if="selectedBuild" class="grid min-w-0 gap-3 rounded-xl border p-4 text-sm">
          <div>
            <dt class="text-muted-foreground">{{ t("archive.build") }}</dt>
            <dd dir="ltr" class="break-all">
              {{ selectedBuild.build }} · Android {{ selectedBuild.android }}
            </dd>
          </div>
          <div>
            <dt class="text-muted-foreground">{{ t("archive.patch") }}</dt>
            <dd dir="ltr">{{ selectedBuild.security_patch }}</dd>
          </div>
          <div>
            <dt class="text-muted-foreground">{{ t("editing.DEVICE_INITIAL_SDK_INT") }}</dt>
            <dd>{{ selectedDevice?.first_api_level }}</dd>
          </div>
        </dl>
        <p class="text-muted-foreground text-sm break-words">
          {{ t("archive.updated") }}:
          <time :datetime="catalog.generated">{{
            new Date(catalog.generated).toLocaleString(global.locale.value)
          }}</time>
        </p>
        <Button class="self-start" :disabled="busy || !selectedBuild" @click="preview">{{
          t("archive.preview")
        }}</Button>
        <SettingDetails :title="t('archive.details')">
          <dl class="grid min-w-0 gap-4 text-sm">
            <div>
              <dt class="text-muted-foreground">{{ t("archive.revision") }}</dt>
              <dd dir="ltr" class="mt-1 break-all font-mono text-xs">
                {{ catalog.source_revision }}
              </dd>
            </div>
            <div v-if="selectedBuild">
              <dt class="text-muted-foreground">{{ t("archive.digest") }}</dt>
              <dd dir="ltr" class="mt-1 break-all font-mono text-xs">
                {{ selectedBuild.vbmeta_digest }}
              </dd>
            </div>
          </dl>
        </SettingDetails>
      </template>
    </div>
  </SectionCard>
</template>
