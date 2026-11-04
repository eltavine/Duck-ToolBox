<script setup lang="ts">
import { useMutation } from "@tanstack/vue-query"
import { CloudUpload, FileCheck, FlaskConical, HardDriveDownload, TriangleAlert } from "@lucide/vue"
import { storeToRefs } from "pinia"
import { computed } from "vue"
import { useI18n } from "vue-i18n"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import KeyboxCompatibilityDialog from "@/core/components/KeyboxCompatibilityDialog.vue"
import KeyValueList from "@/core/components/KeyValueList.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import type { KeyboxTarget } from "@/core/features"
import { contributions } from "@/core/features"
import { useKeyboxInstallConfirmation } from "@/core/keybox"
import { notifySuccess } from "@/core/notify"

import { useRkpI18n } from "../i18n"
import { useRkpStore } from "../store"

const { t } = useRkpI18n()
const global = useI18n()
const store = useRkpStore()
const { importDraft, importResult, importBusy, importReady, importComplete } = storeToRefs(store)
const validate = useMutation({ mutationFn: store.validateImport })
const diagnose = useMutation({ mutationFn: store.diagnoseImport })
const issue = useMutation({ mutationFn: store.issueImport })
const exported = computed(() =>
  importResult.value?.stage === "exported" ? importResult.value : null,
)
const targets = contributions("keyboxTargets")
const compatibility = useKeyboxInstallConfirmation()
const install = useMutation({
  mutationFn: (target: KeyboxTarget) => compatibility.install(target, exported.value!.keybox_path),
  onSuccess: (result) => {
    if (result) notifySuccess(t("keybox.installed", { target: result.targetPath }))
  },
})
const materialRows = computed(() => {
  const data = importResult.value?.material
  if (!data) return []
  return [
    { label: t("import.materialType"), value: data.material_type },
    { label: t("import.algorithm"), value: data.algorithm },
    { label: t("import.entries"), value: data.dice_entries },
    { label: t("import.chain"), value: t("provision.valid") },
    { label: t("import.key"), value: t("provision.valid") },
    { label: t("import.trust"), value: t("import.unverified") },
  ]
})
const resultRows = computed(() => {
  const result = importResult.value
  if (!result || result.stage === "validated") return []
  const rows = [{ label: t("verify.path"), value: result.csr_path, mono: true, copy: true }]
  if (result.stage === "exported") {
    rows.push(
      { label: t("keybox.path"), value: result.keybox_path, mono: true, copy: true },
      { label: t("keybox.deviceId"), value: result.device_id, mono: true, copy: false },
      { label: t("import.server"), value: t("provision.valid"), mono: false, copy: false },
      { label: t("import.certificates"), value: t("provision.valid"), mono: false, copy: false },
    )
  }
  return rows
})
</script>

<template>
  <div class="flex flex-col gap-6">
    <SectionCard :title="t('import.title')">
      <div class="flex flex-col gap-5">
        <p class="text-muted-foreground text-sm">{{ t("import.workflow") }}</p>
        <p class="text-sm">{{ t("import.description") }}</p>
        <fieldset :disabled="importBusy || install.isPending.value" class="flex flex-col gap-5">
          <Field>
            <FieldLabel for="import-fingerprint-policy">{{
              t("profile.includeFingerprint")
            }}</FieldLabel>
            <Select
              v-model="importDraft.fingerprintPolicy"
              :disabled="importBusy || install.isPending.value"
            >
              <SelectTrigger id="import-fingerprint-policy" class="w-full"
                ><SelectValue
              /></SelectTrigger>
              <SelectContent>
                <SelectItem value="config">{{ t("import.fingerprintFromConfig") }}</SelectItem>
                <SelectItem value="include">{{ t("import.fingerprintInclude") }}</SelectItem>
                <SelectItem value="omit">{{ t("import.fingerprintOmit") }}</SelectItem>
              </SelectContent>
            </Select>
            <FieldDescription>{{ t("import.fingerprintHint") }}</FieldDescription>
          </Field>
          <RadioGroup v-model="importDraft.mode" class="gap-3">
            <Label for="import-bundle" class="items-start font-normal">
              <RadioGroupItem id="import-bundle" value="bundle" />{{ t("import.bundle") }}
            </Label>
            <Label for="import-split" class="items-start font-normal">
              <RadioGroupItem id="import-split" value="split" />{{ t("import.split") }}
            </Label>
          </RadioGroup>
          <Field v-if="importDraft.mode === 'bundle'">
            <FieldLabel for="import-bundle-path">{{ t("import.bundlePath") }}</FieldLabel>
            <Input
              id="import-bundle-path"
              v-model="importDraft.bundlePath"
              class="font-mono text-sm"
              placeholder="/data/local/tmp/dice-bundle.json"
              autocomplete="off"
              autocapitalize="off"
              spellcheck="false"
            />
          </Field>
          <template v-else>
            <Field>
              <FieldLabel for="import-chain-path">{{ t("import.chainPath") }}</FieldLabel>
              <Input
                id="import-chain-path"
                v-model="importDraft.chainPath"
                class="font-mono text-sm"
                placeholder="/data/local/tmp/dice-chain.cbor"
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
              />
            </Field>
            <Field>
              <FieldLabel for="import-signing-path">{{ t("import.signingPath") }}</FieldLabel>
              <Input
                id="import-signing-path"
                v-model="importDraft.signingPath"
                class="font-mono text-sm"
                placeholder="/data/local/tmp/signing.json"
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
              />
            </Field>
          </template>
          <Field>
            <FieldLabel for="import-config-path">{{ t("import.configPath") }}</FieldLabel>
            <Input
              id="import-config-path"
              v-model="importDraft.configPath"
              class="font-mono text-sm"
              placeholder="/data/local/tmp/request.json"
              autocomplete="off"
              autocapitalize="off"
              spellcheck="false"
            />
            <FieldDescription>{{ t("import.configHint") }}</FieldDescription>
          </Field>
          <p class="text-muted-foreground text-sm">{{ t("import.fileHint") }}</p>
        </fieldset>
        <div class="flex flex-wrap gap-3">
          <Button :disabled="!importComplete || importBusy" @click="validate.mutate()">
            <Spinner v-if="validate.isPending.value" /><FileCheck v-else />{{
              t("import.validate")
            }}
          </Button>
          <Button
            variant="outline"
            :disabled="!importComplete || importBusy"
            @click="diagnose.mutate()"
          >
            <Spinner v-if="diagnose.isPending.value" /><FlaskConical v-else />
            {{ t("import.diagnose") }}
          </Button>
        </div>
        <Button size="lg" :disabled="!importReady || importBusy" @click="issue.mutate()">
          <Spinner v-if="issue.isPending.value" /><CloudUpload v-else />{{ t("import.issue") }}
        </Button>
        <p v-if="importBusy" class="text-muted-foreground text-sm" role="status">
          {{ t("import.busy") }}
        </p>
      </div>
    </SectionCard>

    <template v-if="importResult">
      <Alert>
        <TriangleAlert />
        <AlertTitle>{{ t("import.trust") }}: {{ t("import.unverified") }}</AlertTitle>
        <AlertDescription>{{ t("import.trustHint") }}</AlertDescription>
      </Alert>
      <SectionCard :title="t('import.validate')">
        <KeyValueList :rows="materialRows" />
        <p class="mt-4 text-sm" role="status">
          {{
            importResult.stage === "validated"
              ? importResult.ready_for_issuance
                ? t("import.ready")
                : t("import.configRequired")
              : importResult.stage === "diagnostic"
                ? t("import.diagnostic")
                : t("import.exported")
          }}
        </p>
      </SectionCard>
      <SectionCard v-if="resultRows.length" :title="t('import.result')">
        <KeyValueList :rows="resultRows" />
        <div v-if="exported" class="mt-4 flex flex-col gap-2">
          <Button
            v-for="target in targets"
            :key="target.id"
            variant="secondary"
            :disabled="install.isPending.value || importBusy"
            @click="install.mutate(target)"
          >
            <HardDriveDownload />{{ t("keybox.install", { target: global.t(target.labelKey) }) }}
          </Button>
        </div>
      </SectionCard>
    </template>
    <KeyboxCompatibilityDialog :confirmation="compatibility" />
  </div>
</template>
