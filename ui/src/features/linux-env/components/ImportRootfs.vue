<script setup lang="ts">
import { ref, watch, computed } from "vue"
import { useMutation } from "@tanstack/vue-query"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import FilePickerDialog from "@/core/components/FilePickerDialog.vue"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import { errorTitle, showErrorDetails } from "@/core/notify"
import { LoaderCircle } from "@lucide/vue"
import { formatBytes } from "@/core/format"
import { linuxApi, idSchema, configSchema } from "../api"
import type { Inspection } from "../api"
import { useLinuxI18n } from "../i18n"
const props = defineProps<{ disabled: boolean; busy: boolean }>()
const emit = defineEmits<{ imported: [] }>()
const { t } = useLinuxI18n()
const id = ref("debian"),
  name = ref("Debian"),
  path = ref(""),
  picker = ref(false),
  confirm = ref(false)
const inspected = ref<Inspection | null>(null)
const pending = ref<{ id: string; name: string; archive: Inspection } | null>(null)
watch(path, () => {
  inspected.value = null
  inspect.reset()
})
const inspect = useMutation({
  onError: () => undefined,
  mutationFn: async () => {
    const original = path.value
    const result = await linuxApi.inspect(original)
    if (original === path.value) inspected.value = result
  },
})
const importer = useMutation({
  mutationFn: async () => {
    if (!pending.value) return
    await linuxApi.import(pending.value.id, pending.value.name, pending.value.archive)
  },
  onSuccess: () => {
    emit("imported")
    inspected.value = null
  },
})
const validName = computed(
  () =>
    configSchema.safeParse({ name: name.value, auto_start: false, dns: [], services: [] }).success,
)
const invalidId = computed(() => !idSchema.safeParse(id.value).success)
const blocked = computed(() => props.busy || inspect.isPending.value || importer.isPending.value)
let returnFocus: HTMLElement | null = null
let accepted = false
function confirmImport() {
  accepted = true
  importer.mutate()
}
function closeFocus(event: Event) {
  if (!accepted && returnFocus?.isConnected) {
    event.preventDefault()
    returnFocus.focus({ preventScroll: true })
  }
}
function request() {
  if (!inspected.value || invalidId.value || !validName.value || blocked.value || props.disabled)
    return
  returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
  accepted = false
  pending.value = { id: id.value, name: name.value.trim(), archive: { ...inspected.value } }
  confirm.value = true
}
</script>
<template>
  <SectionCard :title="t('import')" :description="t('importHelp')">
    <div class="grid min-w-0 gap-4 sm:grid-cols-2">
      <div class="space-y-2">
        <Label for="linux-name">{{ t("name") }}</Label
        ><Input
          id="linux-name"
          v-model="name"
          :disabled="blocked"
          :aria-invalid="!validName"
          aria-describedby="linux-name-help"
          maxlength="120"
        />
        <p id="linux-name-help" class="text-muted-foreground text-xs">{{ t("nameHelp") }}</p>
      </div>
      <div class="space-y-2">
        <Label for="linux-id">{{ t("id") }}</Label
        ><Input
          id="linux-id"
          v-model="id"
          dir="ltr"
          :disabled="blocked"
          :aria-invalid="invalidId"
          aria-describedby="linux-id-help"
        />
        <p id="linux-id-help" class="text-muted-foreground text-xs">{{ t("idHelp") }}</p>
      </div>
    </div>
    <div class="mt-4 space-y-2">
      <Label for="linux-archive">{{ t("archive") }}</Label
      ><Input id="linux-archive" v-model="path" dir="ltr" :disabled="blocked" />
      <div class="flex flex-wrap gap-2">
        <Button variant="outline" class="min-h-12" :disabled="blocked" @click="picker = true">{{
          t("browse")
        }}</Button
        ><Button
          variant="outline"
          class="min-h-12"
          :disabled="blocked || !path.startsWith('/')"
          @click="inspect.mutate()"
          >{{ t("inspect") }}</Button
        >
      </div>
    </div>
    <p v-if="inspect.isPending.value" class="mt-3 flex items-center gap-2 text-sm" role="status">
      <LoaderCircle class="size-4 motion-safe:animate-spin" aria-hidden="true" />{{
        t("inspectionPending")
      }}
    </p>
    <div v-if="inspect.error.value" class="mt-3 space-y-2" role="alert">
      <p class="text-destructive text-sm">{{ errorTitle(inspect.error.value) }}</p>
      <Button variant="outline" class="min-h-12" @click="showErrorDetails(inspect.error.value)">{{
        t("details")
      }}</Button>
    </div>
    <KeyValueList
      v-if="inspected"
      class="mt-4"
      :rows="[
        { label: t('archive'), value: formatBytes(inspected.bytes) },
        { label: 'SHA-256', value: inspected.sha256, mono: true },
      ]"
    />
    <Button
      class="mt-4 min-h-12"
      :disabled="disabled || blocked || !inspected || invalidId || !validName"
      @click="request"
      >{{ t("import") }}</Button
    >
  </SectionCard>
  <FilePickerDialog
    v-model:open="picker"
    extension=""
    :allow-system="false"
    @pick="
      (file) => {
        if ('path' in file) path = file.path
      }
    "
  />
  <ConfirmDialog
    v-model:open="confirm"
    :title="t('import')"
    :description="t('trust')"
    :confirm-label="t('import')"
    @confirm="confirmImport"
    @close-auto-focus="closeFocus"
    ><KeyValueList
      :rows="[
        { label: t('name'), value: pending?.name },
        { label: t('id'), value: pending?.id, mono: true },
        { label: t('archive'), value: pending?.archive.path, mono: true },
        { label: 'SHA-256', value: pending?.archive.sha256, mono: true },
      ]"
  /></ConfirmDialog>
</template>
