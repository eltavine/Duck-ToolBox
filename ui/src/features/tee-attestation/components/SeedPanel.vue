<script setup lang="ts">
import { useMutation } from "@tanstack/vue-query"
import { FileKey2, ShieldCheck } from "@lucide/vue"
import { computed, ref, watch } from "vue"

import { Alert, AlertDescription } from "@/components/ui/alert"
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
import { Spinner } from "@/components/ui/spinner"
import KeyValueList from "@/core/components/KeyValueList.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { notifySuccess } from "@/core/notify"

import { attestationApi } from "../api"
import type { Derivation, InputFile, SeedFormat, SeedResult } from "../api"
import { useAttestationI18n } from "../i18n"
import CaptureInput from "./CaptureInput.vue"

const { t } = useAttestationI18n()
const source = ref<InputFile | null>(null)
const csr = ref<InputFile | null>(null)
const format = ref<SeedFormat>("raw")
const derivation = ref<Derivation>("ed25519-seed")
const offset = ref("")
const result = ref<SeedResult | null>(null)
const validOffset = computed(
  () =>
    format.value !== "raw" ||
    !offset.value ||
    (/^\d+$/.test(offset.value) && Number.isSafeInteger(Number(offset.value))),
)
const run = useMutation({
  mutationFn: (save: boolean) =>
    attestationApi.seed({
      source: source.value!,
      csr: csr.value!,
      format: format.value,
      derivation: derivation.value,
      offset: format.value === "raw" && offset.value ? Number(offset.value) : undefined,
      export: save,
    }),
  onMutate: () => {
    result.value = null
  },
  onSuccess: (data) => {
    result.value = data
    if (data.artifacts) notifySuccess(t("result.saved"))
  },
})
watch([source, csr, format, derivation, offset], () => {
  result.value = null
})
</script>

<template>
  <div class="flex flex-col gap-5">
    <p class="text-muted-foreground text-sm">{{ t("seed.description") }}</p>
    <fieldset :disabled="run.isPending.value" class="flex min-w-0 flex-col gap-4">
      <CaptureInput id="tee-seed" v-model="source" :label="t('seed.source')" />
      <CaptureInput id="tee-csr" v-model="csr" :label="t('seed.csr')" />
      <Field>
        <FieldLabel for="tee-seed-format">{{ t("seed.format") }}</FieldLabel>
        <Select v-model="format"
          ><SelectTrigger id="tee-seed-format" class="w-full"><SelectValue /></SelectTrigger
          ><SelectContent>
            <SelectItem value="raw">{{ t("seed.raw") }}</SelectItem>
            <SelectItem value="hex">{{ t("seed.hex") }}</SelectItem>
          </SelectContent></Select
        >
      </Field>
      <Field>
        <FieldLabel for="tee-seed-derivation">{{ t("seed.derivation") }}</FieldLabel>
        <Select v-model="derivation"
          ><SelectTrigger id="tee-seed-derivation" class="w-full"><SelectValue /></SelectTrigger
          ><SelectContent>
            <SelectItem value="ed25519-seed">Ed25519 seed</SelectItem>
            <SelectItem value="p256-seed">P-256 seed · mod(n − 1) + 1</SelectItem>
            <SelectItem value="p256-scalar">{{ t("seed.scalar") }}</SelectItem>
          </SelectContent></Select
        >
      </Field>
      <Field v-if="format === 'raw'">
        <FieldLabel for="tee-seed-offset">{{ t("seed.offset") }}</FieldLabel>
        <Input
          id="tee-seed-offset"
          v-model="offset"
          inputmode="numeric"
          placeholder="0"
          :aria-invalid="!validOffset"
        />
        <p class="text-muted-foreground text-sm">{{ t("seed.offsetHint") }}</p>
      </Field>
      <Button size="lg" :disabled="!source || !csr || !validOffset" @click="run.mutate(false)"
        ><Spinner v-if="run.isPending.value" /><ShieldCheck v-else />{{ t("seed.run") }}</Button
      >
      <SectionCard v-if="result" :title="t('result.title')">
        <Alert :variant="result.report.identity_matches ? 'default' : 'destructive'"
          ><ShieldCheck /><AlertDescription>{{
            result.report.identity_matches ? t("seed.matched") : t("seed.mismatch")
          }}</AlertDescription></Alert
        >
        <KeyValueList
          class="mt-4"
          :rows="[
            {
              label: t('result.publicHash'),
              value: result.report.public_key_sha256,
              mono: true,
              copy: true,
            },
            { label: t('result.csrHash'), value: result.report.csr_sha256, mono: true, copy: true },
          ]"
        />
      </SectionCard>
      <p v-if="result" class="text-muted-foreground text-sm">{{ t("seed.limits") }}</p>
      <Button
        v-if="result?.report.identity_matches && !result.artifacts"
        variant="outline"
        size="lg"
        @click="run.mutate(true)"
        ><FileKey2 />{{ t("seed.export") }}</Button
      >
      <KeyValueList
        v-if="result?.artifacts"
        :rows="[
          {
            label: t('result.material'),
            value: result.artifacts.material_path,
            mono: true,
            copy: true,
          },
          {
            label: t('result.report'),
            value: result.artifacts.report_path,
            mono: true,
            copy: true,
          },
        ]"
      />
    </fieldset>
  </div>
</template>
