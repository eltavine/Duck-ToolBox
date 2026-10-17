<script setup lang="ts">
import { useMutation } from "@tanstack/vue-query"
import { FileKey2, ScanLine } from "@lucide/vue"
import { ref, watch } from "vue"
import { useI18n } from "vue-i18n"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import KeyValueList from "@/core/components/KeyValueList.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { contributions } from "@/core/features"
import type { KeyboxTarget } from "@/core/features"
import { notifySuccess } from "@/core/notify"

import { attestationApi } from "../api"
import type { InputFile, KeyboxResult } from "../api"
import { useAttestationI18n } from "../i18n"
import CaptureInput from "./CaptureInput.vue"

const { t } = useAttestationI18n()
const global = useI18n()
const source = ref<InputFile | null>(null)
const result = ref<KeyboxResult | null>(null)
const targets = contributions("keyboxTargets")
const run = useMutation({
  mutationFn: (save: boolean) =>
    attestationApi.keybox({ source: source.value!, format: "auto", export: save }),
  onMutate: () => {
    result.value = null
  },
  onSuccess: (data) => {
    result.value = data
    if (data.artifacts) notifySuccess(t("result.saved"))
  },
})
const install = useMutation({
  mutationFn: (target: KeyboxTarget) => target.install(result.value!.artifacts!.material_path),
  onSuccess: () => notifySuccess(t("keybox.installed")),
})
watch(source, () => {
  result.value = null
})
</script>

<template>
  <div class="flex flex-col gap-5">
    <p class="text-muted-foreground text-sm">{{ t("keybox.description") }}</p>
    <fieldset
      :disabled="run.isPending.value || install.isPending.value"
      class="flex min-w-0 flex-col gap-4"
    >
      <CaptureInput id="tee-keybox" v-model="source" :label="t('keybox.source')" />
      <Button size="lg" :disabled="!source" @click="run.mutate(false)"
        ><Spinner v-if="run.isPending.value" /><ScanLine v-else />{{ t("keybox.run") }}</Button
      >
      <SectionCard v-if="result" :title="t('result.title')">
        <KeyValueList
          :rows="[
            { label: t('keybox.count'), value: result.report.keyboxes },
            {
              label: t('result.sourceHash'),
              value: result.report.source_sha256,
              mono: true,
              copy: true,
            },
          ]"
        />
        <div
          v-for="(key, index) in result.report.keys"
          :key="index"
          class="mt-4 flex flex-col gap-2"
        >
          <p class="font-medium">
            {{ key.algorithm.toUpperCase() }} ·
            {{ t("keybox.certificates", { count: key.certificates }) }}
          </p>
          <KeyValueList
            :rows="[
              {
                label: t('result.publicHash'),
                value: key.public_key_sha256,
                mono: true,
                copy: true,
              },
              {
                label: t('keybox.validity'),
                value: key.valid_now ? t('result.yes') : t('result.no'),
              },
            ]"
          />
        </div>
      </SectionCard>
      <p v-if="result" class="text-muted-foreground text-sm">{{ t("keybox.limits") }}</p>
      <Button
        v-if="result && !result.artifacts"
        variant="outline"
        size="lg"
        @click="run.mutate(true)"
        ><FileKey2 />{{ t("keybox.export") }}</Button
      >
      <template v-if="result?.artifacts">
        <KeyValueList
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
        <Button
          v-for="target in targets"
          :key="target.id"
          variant="outline"
          @click="install.mutate(target)"
          ><Spinner v-if="install.isPending.value" /><FileKey2 v-else />{{
            t("keybox.install", { target: global.t(target.labelKey) })
          }}</Button
        >
      </template>
    </fieldset>
  </div>
</template>
