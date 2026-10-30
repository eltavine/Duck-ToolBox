<script setup lang="ts">
import { Button } from "@/components/ui/button"
import { ref } from "vue"
import { useQueryClient } from "@tanstack/vue-query"
import { Input } from "@/components/ui/input"
import { libraryApi } from "../library-api"
import { notifyError, notifySuccess } from "@/core/notify"
import PatchPlan from "./PatchPlan.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import type { SecurityPatchPreview } from "@/core/features"
import type { Preview } from "../api"
import { useIntegrityI18n } from "../i18n"
const props = defineProps<{
  preview: Preview
  patches: { target: { id: string }; preview: SecurityPatchPreview; selected: boolean }[]
  busy: boolean
}>()
const emit = defineEmits<{
  apply: []
  close: []
  select: [id: string, selected: boolean]
  patchPreview: [id: string, partitions: Record<string, string>]
}>()
const { t } = useIntegrityI18n()
const profileName = ref("")
const saving = ref(false)
const client = useQueryClient()
async function saveCandidate() {
  if (saving.value) return
  saving.value = true
  try {
    await libraryApi.saveProfile(profileName.value, props.preview.json, props.preview.provenance)
    await client.invalidateQueries({ queryKey: ["play-integrity.profiles"] })
    notifySuccess(t("profiles.saved"))
    profileName.value = ""
  } catch (error) {
    notifyError(error)
  } finally {
    saving.value = false
  }
}
</script>
<template>
  <SectionCard :title="t('preview')" :description="t('previewHelp')">
    <p v-if="preview.metadata" class="mb-3 text-sm">
      {{ t("dates") }}: {{ preview.metadata.released ?? "—" }} →
      {{ preview.metadata.estimated_expiry ?? "—" }}
    </p>
    <div
      v-if="preview.warnings.length"
      class="bg-muted mb-4 flex flex-col gap-2 rounded-lg p-4"
      role="status"
    >
      <p
        v-for="(warning, index) in preview.warnings"
        :key="index"
        class="text-muted-foreground text-sm break-words"
      >
        {{ t(`warnings.${warning.code}`)
        }}<span v-if="warning.fields.length" class="font-mono">
          · {{ warning.fields.join(", ") }}</span
        >
      </p>
    </div>
    <div
      tabindex="0"
      role="region"
      :aria-label="t('preview')"
      class="overflow-x-auto rounded-xl border border-border/70 focus-visible:outline-2 focus-visible:outline-offset-2"
    >
      <table class="w-full min-w-[32rem] text-start text-xs">
        <thead class="bg-muted/60">
          <tr>
            <th class="p-2 text-start">{{ t("field") }}</th>
            <th class="p-2 text-start">{{ t("before") }}</th>
            <th class="p-2 text-start">{{ t("after") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="change in preview.changes" :key="change.key" class="border-t">
            <td dir="ltr" class="p-2 font-mono">{{ change.key }}</td>
            <td dir="ltr" class="max-w-48 p-2 break-all">{{ change.before ?? "—" }}</td>
            <td dir="ltr" class="max-w-48 p-2 break-all">{{ change.after ?? "—" }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <PatchPlan
      v-for="patch in patches"
      :key="patch.target.id"
      :preview="patch.preview"
      :selected="patch.selected"
      :busy="busy"
      @select="emit('select', patch.target.id, $event)"
      @preview="emit('patchPreview', patch.target.id, $event)"
    />
    <div v-if="preview.provenance" class="mt-4 rounded-xl border p-4">
      <p dir="ltr" class="text-muted-foreground mb-3 text-sm [overflow-wrap:anywhere]">
        {{ preview.provenance.source }} · {{ preview.provenance.build }}
      </p>
      <p
        v-if="
          preview.provenance.source === 'https://flash.android.com/' &&
          preview.provenance.identity_bound !== true
        "
        role="status"
        class="text-muted-foreground mb-3 text-sm break-words"
      >
        {{ t("pixel.legacySource") }}
      </p>
      <p
        v-if="preview.provenance.source_revision"
        dir="ltr"
        class="text-muted-foreground mb-3 text-xs [overflow-wrap:anywhere]"
      >
        {{ preview.provenance.source_revision }} ·
        <bdi dir="auto">{{
          preview.provenance.channel ? t(`archive.${preview.provenance.channel}`) : ""
        }}</bdi>
        ·
        {{ preview.provenance.security_patch }}
      </p>
      <Input
        v-model="profileName"
        maxlength="64"
        :aria-label="t('profiles.name')"
        :placeholder="t('profiles.name')"
      />
      <Button
        variant="outline"
        class="mt-2"
        :disabled="busy || saving || !profileName.trim()"
        @click="saveCandidate"
        >{{ t("profiles.save") }}</Button
      >
    </div>
    <div class="mt-4 flex flex-wrap gap-2">
      <Button :disabled="busy" @click="emit('apply')">{{ t("apply") }}</Button
      ><Button variant="outline" :disabled="busy" @click="emit('close')">{{ t("dismiss") }}</Button>
    </div>
  </SectionCard>
</template>
