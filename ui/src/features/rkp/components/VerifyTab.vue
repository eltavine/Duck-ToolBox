<script setup lang="ts">
import { useMutation } from "@tanstack/vue-query"
import { FolderOpen, ShieldCheck } from "@lucide/vue"
import { storeToRefs } from "pinia"
import { computed, ref } from "vue"
import { useI18n } from "vue-i18n"

import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Spinner } from "@/components/ui/spinner"
import FilePickerDialog from "@/core/components/FilePickerDialog.vue"
import type { PickedFile } from "@/core/components/FilePickerDialog.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import SectionCard from "@/core/components/SectionCard.vue"

import { useRkpI18n } from "../i18n"
import { useRkpStore } from "../store"

const { t } = useRkpI18n()
const global = useI18n()
const store = useRkpStore()
const { verify, verifyPath } = storeToRefs(store)
const picking = ref(false)
const run = useMutation({ mutationFn: store.runVerify })

function pick(file: PickedFile) {
  if ("path" in file) verifyPath.value = file.path
}

const rows = computed(() => {
  const report = verify.value?.report
  if (!report) return []
  return [
    {
      label: t("verify.signature"),
      value: report.signature_valid ? t("provision.valid") : t("provision.invalid"),
    },
    { label: t("verify.version"), value: report.csr_version },
    { label: t("verify.certType"), value: report.cert_type },
    { label: t("verify.diceEntries"), value: report.dice_entries },
    { label: t("verify.keysToSign"), value: report.keys_to_sign },
    { label: t("verify.unverifiedFingerprint"), value: report.unverified_fingerprint, mono: true },
    { label: t("verify.udsPub"), value: report.uds_pub_hex, mono: true, copy: true },
  ]
})
</script>

<template>
  <div class="flex flex-col gap-6">
    <p v-if="!verify" class="text-muted-foreground px-4 text-sm">{{ t("verify.empty") }}</p>
    <Field>
      <FieldLabel for="verify-path">{{ t("verify.path") }}</FieldLabel>
      <InputGroup>
        <InputGroupInput
          id="verify-path"
          v-model="verifyPath"
          class="font-mono text-sm"
          :placeholder="t('verify.placeholder')"
        />
        <InputGroupAddon align="inline-end">
          <InputGroupButton
            size="icon-sm"
            :aria-label="global.t('core.actions.browse')"
            @click="picking = true"
          >
            <FolderOpen />
          </InputGroupButton>
        </InputGroupAddon>
      </InputGroup>
    </Field>
    <Button size="lg" :disabled="!verifyPath.trim() || store.profileBusy" @click="run.mutate()">
      <Spinner v-if="run.isPending.value" class="size-6" />
      <ShieldCheck v-else />
      {{ t("verify.run") }}
    </Button>

    <SectionCard v-if="verify" :title="verify.path">
      <KeyValueList :rows="rows" />
    </SectionCard>

    <FilePickerDialog
      v-model:open="picking"
      extension="cbor"
      root="/data/adb/duck-toolbox/var"
      start="/data/adb/duck-toolbox/var/outputs"
      :allow-system="false"
      @pick="pick"
    />
  </div>
</template>
