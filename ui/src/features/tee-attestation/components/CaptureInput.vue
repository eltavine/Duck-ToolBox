<script setup lang="ts">
import { FolderOpen, X } from "@lucide/vue"
import { computed, ref, useTemplateRef } from "vue"

import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group"
import { notifyError } from "@/core/notify"

import type { InputFile } from "../api"
import { MAX_UPLOAD } from "../api"
import { useAttestationI18n } from "../i18n"

defineProps<{ id: string; label: string }>()
const model = defineModel<InputFile | null>({ required: true })
const { t } = useAttestationI18n()
const picker = useTemplateRef<HTMLInputElement>("picker")
const fileName = ref("")
const reading = ref(false)
function clear() {
  model.value = null
  fileName.value = ""
}
const value = computed({
  get: () => (model.value && "path" in model.value ? model.value.path : fileName.value),
  set: (path: string) => {
    model.value = path.trim() ? { path: path.trim() } : null
    fileName.value = ""
  },
})

async function pick(event: Event) {
  const element = event.target as HTMLInputElement
  const file = element.files?.[0]
  if (!file) return
  // Clear the old source before reading: a failed selection must never reuse it.
  model.value = null
  fileName.value = ""
  reading.value = true
  try {
    if (!file.size || file.size > MAX_UPLOAD) throw new Error(t("input.limit"))
    const bytes = new Uint8Array(await file.arrayBuffer())
    let binary = ""
    for (let index = 0; index < bytes.length; index += 8192) {
      binary += String.fromCharCode(...bytes.subarray(index, index + 8192))
    }
    model.value = { base64: btoa(binary) }
    fileName.value = file.name
  } catch (error) {
    notifyError(error)
  } finally {
    reading.value = false
    element.value = ""
  }
}
</script>

<template>
  <Field>
    <FieldLabel :for="id">{{ label }}</FieldLabel>
    <InputGroup>
      <InputGroupInput
        :id="id"
        v-model="value"
        :readonly="reading || (!!model && 'base64' in model)"
        :placeholder="t('input.path')"
        class="font-mono text-sm"
      />
      <InputGroupAddon align="inline-end">
        <InputGroupButton
          v-if="model"
          size="icon-sm"
          :aria-label="t('input.clear', { label })"
          :disabled="reading"
          @click="clear"
          ><X
        /></InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
    <Button variant="outline" :disabled="reading" @click="picker?.click()"
      ><FolderOpen />{{ t("input.choose", { label }) }}</Button
    >
    <input
      :id="`${id}-upload`"
      ref="picker"
      type="file"
      class="hidden"
      :aria-label="t('input.choose', { label })"
      @change="pick"
    />
  </Field>
</template>
