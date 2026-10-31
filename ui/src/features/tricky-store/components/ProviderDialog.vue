<script setup lang="ts">
import { ref, watch } from "vue"
import { useI18n } from "vue-i18n"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

import type { Provider } from "../api"
import { useTrickyI18n } from "../i18n"

/** Tricky Addon's custom keybox dialog: name, link and decode script. */
const props = defineProps<{ provider: Provider | null; saving: boolean }>()
const open = defineModel<boolean>("open", { required: true })
const emit = defineEmits<{ save: [provider: Provider]; remove: [] }>()
const { t } = useTrickyI18n()
const { t: coreT } = useI18n()

const form = ref<Provider>({ name: "", url: "", decode: "" })
watch(
  () => open.value && props.provider,
  () => {
    if (open.value) form.value = { ...(props.provider ?? { name: "", url: "", decode: "" }) }
  },
  { immediate: true },
)

function submit() {
  emit("save", {
    name: form.value.name.trim(),
    url: form.value.url.trim(),
    decode: form.value.decode.trim(),
  })
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>{{ t("ta.customkb_dialog_title") }}</DialogTitle>
      </DialogHeader>
      <form class="flex flex-col gap-4" @submit.prevent="submit">
        <Field>
          <FieldLabel for="provider-name">{{ t("ta.customkb_name_placeholder") }}</FieldLabel>
          <Input id="provider-name" v-model="form.name" required />
        </Field>
        <Field>
          <FieldLabel for="provider-url">{{ t("keybox.url") }}</FieldLabel>
          <Input
            id="provider-url"
            v-model="form.url"
            type="url"
            required
            placeholder="https://"
            autocapitalize="off"
          />
        </Field>
        <Field>
          <FieldLabel for="provider-decode">{{ t("ta.customkb_script_placeholder") }}</FieldLabel>
          <Input
            id="provider-decode"
            v-model="form.decode"
            placeholder="base64 -d"
            class="font-mono text-sm"
            autocapitalize="off"
            spellcheck="false"
          />
          <FieldDescription>{{ t("keybox.decodeHint") }}</FieldDescription>
        </Field>
        <DialogFooter class="flex-row gap-2">
          <Button
            v-if="provider"
            type="button"
            variant="ghost"
            class="text-destructive me-auto"
            @click="emit('remove')"
          >
            {{ t("ta.functional_button_remove") }}
          </Button>
          <Button type="button" variant="outline" @click="open = false">{{
            coreT("core.actions.cancel")
          }}</Button>
          <Button type="submit" :disabled="saving">{{ t("ta.functional_button_save") }}</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
