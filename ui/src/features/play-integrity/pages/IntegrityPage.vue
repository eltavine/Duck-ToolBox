<script setup lang="ts">
import { computed, nextTick, ref, watch, useTemplateRef } from "vue"
import { RefreshCw, FileSliders, FileCode } from "@lucide/vue"
import { useI18n } from "vue-i18n"
import { RouterLink } from "vue-router"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import type { PickedFile } from "@/core/components/FilePickerDialog.vue"
import FilePickerDialog from "@/core/components/FilePickerDialog.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { useCopy } from "@/core/links"
import { notifyError } from "@/core/notify"
import EnginePower from "../components/EnginePower.vue"
import PixelFingerprints from "../components/PixelFingerprints.vue"
import PixelArchive from "../components/PixelArchive.vue"
import RecoveryNotice from "../components/RecoveryNotice.vue"
import ConfigFields from "../components/ConfigFields.vue"
import SettingDetails from "../components/SettingDetails.vue"
import ProfileLibrary from "../components/ProfileLibrary.vue"
import ConfigInspection from "../components/ConfigInspection.vue"
import TestHistory from "../components/TestHistory.vue"
import RuleGuide from "../components/RuleGuide.vue"
import { integrityApi } from "../api"
import ConfigPreview from "../components/ConfigPreview.vue"
import LicenseFooter from "../components/LicenseFooter.vue"
import { useIntegrity } from "../use-integrity"
const props = withDefaults(defineProps<{ section?: string }>(), { section: "status" })
const s = useIntegrity()
const { t } = s
const global = useI18n()
const copy = useCopy()
const upload = useTemplateRef<HTMLInputElement>("upload")
const tabNav = useTemplateRef<HTMLElement>("tabNav")
watch(
  () => props.section,
  async () => {
    await nextTick()
    tabNav.value
      ?.querySelector('[aria-current="page"]')
      ?.scrollIntoView({ block: "nearest", inline: "nearest" })
  },
  { immediate: true },
)
const strong = ref(true),
  match = ref(false),
  list = ref(""),
  picker = ref(false)
const source = ref("")
const tabs = [
  "status",
  "config",
  "generate",
  "archive",
  "compatibility",
  "diagnostics",
  "license",
] as const
const fields = ref<Record<string, string> | null>(null)
const rawEditor = ref(false)
async function editFields() {
  await s.action(async () => {
    const parsed = await integrityApi.preview(s.content.value)
    fields.value = { ...parsed.entries }
    rawEditor.value = false
  })
}
function updateFields(value: Record<string, string>) {
  fields.value = value
  s.content.value = JSON.stringify(value, null, 2)
}
watch(
  [s.config.data, s.content],
  ([value, content]) => {
    if (value && content === value.content) fields.value = { ...value.entries }
  },
  { immediate: true },
)
function importSource() {
  rawEditor.value = true
  void s.importSource(source.value.trim())
}
const data = computed(() => s.status.data.value)
const pickerStart = computed(() => {
  const path = source.value.trim()
  return path.startsWith("/")
    ? path.slice(0, path.lastIndexOf("/")) || "/"
    : data.value?.data_dir || "/data/adb"
})
watch(
  data,
  (value) => {
    if (value) {
      list.value = value.app_replace_list
      if (!source.value) source.value = `${value.data_dir.replace(/\/+$/, "")}/custom.pif.json`
    }
  },
  { immediate: true },
)
function picked(file: PickedFile) {
  rawEditor.value = true
  if ("path" in file) {
    source.value = file.path
    void s.importSource(file.path)
  } else {
    s.content.value = file.content
    void s.previewContent(true)
  }
}
async function exportContent(json = false) {
  await s.action(async () => {
    const draft = await integrityApi.preview(s.content.value)
    const content = json ? draft.json : draft.content
    const url = URL.createObjectURL(new Blob([content], { type: "text/plain" }))
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = `custom.pif.${json || content.trimStart().startsWith("{") ? "json" : "prop"}`
    anchor.click()
    URL.revokeObjectURL(url)
  })
}
async function importFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  try {
    if (file.size > 65536) throw new Error(t("fileLimit"))
    rawEditor.value = true
    s.content.value = await file.text()
    await s.previewContent(true)
  } catch (error) {
    notifyError(error)
  } finally {
    input.value = ""
  }
}
</script>
<template>
  <div class="mx-auto flex min-h-full w-full max-w-4xl flex-col">
    <nav
      ref="tabNav"
      :aria-label="t('meta.title')"
      class="mx-4 mt-4 flex min-w-0 gap-1 overflow-x-auto rounded-2xl bg-muted/70 p-1.5"
    >
      <RouterLink
        v-for="tab in tabs"
        :key="tab"
        replace
        :to="tab === 'status' ? '/play-integrity' : `/play-integrity/${tab}`"
        class="state-layer focus-visible:ring-ring/50 relative inline-flex min-h-12 shrink-0 items-center justify-center rounded-xl px-4 text-sm font-medium whitespace-nowrap outline-none transition-[color,background-color,box-shadow] focus-visible:ring-3"
        :class="
          section === tab
            ? 'bg-card text-foreground shadow-sm ring-1 ring-border/60'
            : 'text-muted-foreground hover:text-foreground'
        "
        :aria-current="section === tab ? 'page' : undefined"
        >{{ t(`nav.${tab}`) }}</RouterLink
      >
    </nav>
    <div class="px-4 pt-5">
      <EnginePower :stop-only="section !== 'status'" />
    </div>
    <fieldset :disabled="s.busy.value" class="flex min-w-0 flex-1 flex-col gap-5 px-4 pt-5 pb-6">
      <RecoveryNotice />
      <p
        v-if="data?.reboot_required"
        role="status"
        class="rounded-xl border bg-muted/50 p-4 text-sm"
      >
        {{ t("activation.reboot") }}
      </p>
      <p
        v-else-if="data?.process_restart_required"
        role="status"
        class="rounded-xl border bg-muted/50 p-4 text-sm"
      >
        {{ t("activation.process") }}
      </p>
      <QueryState
        :loading="s.status.isPending.value"
        :error="s.status.error.value"
        :skeleton-rows="2"
        @retry="s.refresh()"
      >
        <SectionCard
          v-if="props.section === 'status'"
          :title="t('nav.status')"
          :description="t('limits')"
        >
          <p
            role="status"
            aria-live="polite"
            class="bg-muted/70 rounded-xl px-4 py-3 text-xl leading-7 font-semibold break-words"
          >
            {{
              s.status.data.value
                ? global.t(`playIntegrity.states.${s.status.data.value.state}`)
                : "—"
            }}
          </p>
          <p class="text-muted-foreground mt-4 text-sm leading-6">{{ t("activationHelp") }}</p>
          <p v-if="data?.conflicts.length" class="text-destructive mt-2 text-sm">
            {{ data.conflicts.join(", ") }}
          </p>
          <Button variant="outline" class="mt-4" :disabled="s.busy.value" @click="s.refresh()"
            ><RefreshCw />{{ global.t("core.actions.refresh") }}</Button
          >
        </SectionCard>
        <SectionCard
          v-if="section === 'config'"
          :title="t('nav.config')"
          :description="t('configHelp')"
        >
          <QueryState
            :loading="s.config.isPending.value"
            :error="s.config.error.value"
            @retry="s.refresh()"
          >
            <p
              v-if="s.config.data.value && s.content.value !== s.config.data.value.content"
              role="status"
              class="text-muted-foreground mb-3 text-sm"
            >
              {{ t("editing.draft") }}
            </p>
            <div class="mb-4 flex flex-wrap gap-2">
              <Button
                variant="secondary"
                :disabled="s.busy.value"
                @click="rawEditor ? editFields() : (rawEditor = true)"
              >
                <FileSliders v-if="rawEditor" /><FileCode v-else />{{
                  t(rawEditor ? "fieldEditor" : "rawEditor")
                }}
              </Button>
            </div>
            <ConfigFields
              v-if="!rawEditor && fields"
              :model-value="fields"
              @update:model-value="updateFields"
            />
            <Textarea
              v-else
              v-model="s.content.value"
              :aria-label="t('nav.config')"
              class="min-h-72 w-full min-w-0 font-mono text-sm"
              spellcheck="false"
            />
            <div class="mt-4 flex flex-wrap gap-2">
              <Button :disabled="s.busy.value" @click="s.previewContent(false)">{{
                t("preview")
              }}</Button>
              <Button variant="outline" :disabled="s.busy.value" @click="s.previewContent(true)">{{
                t("migrate")
              }}</Button>
            </div>
            <SettingDetails :title="t('editing.files')" class="mt-4">
              <div class="flex flex-wrap gap-2">
                <Button variant="outline" @click="copy(s.content.value)">{{
                  global.t("core.actions.copy")
                }}</Button>
                <Button variant="outline" @click="exportContent(false)">{{
                  global.t("core.actions.export")
                }}</Button>
                <Button variant="outline" @click="exportContent(true)"
                  >{{ global.t("core.actions.export") }} .json</Button
                >
              </div>
              <Button variant="outline" class="mt-4" @click="upload?.click()">{{
                global.t("core.actions.import")
              }}</Button>
              <input
                ref="upload"
                type="file"
                accept=".json,.prop,.txt"
                class="hidden"
                :aria-label="global.t('core.actions.import')"
                @change="importFile"
              />
              <div class="mt-4 flex flex-col gap-2">
                <Input
                  v-model="source"
                  :aria-label="t('deviceFile')"
                  class="min-w-0 font-mono text-xs"
                />
                <div class="flex flex-wrap gap-2">
                  <Button variant="outline" @click="picker = true">{{
                    global.t("core.actions.browse")
                  }}</Button
                  ><Button variant="outline" :disabled="s.busy.value" @click="importSource">{{
                    t("deviceFile")
                  }}</Button>
                </div>
              </div>
            </SettingDetails>
            <SettingDetails :title="t('profiles.title')" class="mt-4">
              <ProfileLibrary
                :content="s.content.value"
                @preview="(value) => s.action(() => s.prepare(value))"
              />
            </SettingDetails>
            <Button
              v-if="s.backup.value"
              variant="ghost"
              class="mt-3"
              :disabled="s.busy.value"
              @click="s.restore()"
              >{{ t("restore") }}</Button
            >
          </QueryState>
        </SectionCard>
        <PixelFingerprints
          v-if="section === 'generate'"
          @preview="(value) => s.action(() => s.prepare(value))"
          @invalidate="s.clearPreview()"
        />
        <PixelArchive
          v-if="section === 'archive'"
          @preview="(value) => s.action(() => s.prepare(value))"
          @invalidate="s.clearPreview()"
        />
        <SectionCard
          v-if="section === 'generate'"
          :title="t('nav.generate')"
          :description="t('generateHelp')"
        >
          <p class="bg-muted/70 mb-3 rounded-xl p-4 text-sm leading-6">{{ t("guide.canary") }}</p>
          <RouterLink
            to="/play-integrity/config"
            class="text-primary mb-4 inline-flex min-h-11 items-center text-sm underline underline-offset-4 focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2"
            >{{ t("guide.import") }}</RouterLink
          >
          <SettingDetails :title="t('editing.advanced')">
            <div class="flex flex-col gap-5">
              <Field orientation="horizontal"
                ><FieldContent
                  ><FieldLabel for="pif-strong">{{ t("strong") }}</FieldLabel
                  ><FieldDescription>{{ t("strongHelp") }}</FieldDescription></FieldContent
                ><Switch id="pif-strong" v-model="strong"
              /></Field>
              <Field orientation="horizontal"
                ><FieldLabel for="pif-match">{{ t("match") }}</FieldLabel
                ><Switch id="pif-match" v-model="match"
              /></Field>
            </div>
          </SettingDetails>
          <div class="mt-4 flex flex-wrap gap-2">
            <Button
              :disabled="s.busy.value || !!s.taskId.value || data?.options.enabled === false"
              @click="s.generate(strong, match)"
              >{{ t("generate") }}</Button
            ><Button
              v-if="s.taskId.value"
              variant="outline"
              :disabled="s.busy.value"
              @click="s.cancel()"
              >{{ global.t("core.actions.cancel") }}</Button
            >
            <Button
              v-if="s.task.value?.state === 'completed' && s.task.value.result"
              variant="outline"
              :disabled="s.busy.value"
              @click="s.action(() => s.prepare(s.task.value!.result!))"
              >{{ t("preview") }}</Button
            >
          </div>
          <p v-if="s.task.value" role="status" class="mt-3 text-sm">
            {{ global.t(`playIntegrity.tasks.${s.task.value.state}`) }}
          </p>
          <p
            v-if="s.task.value?.error"
            role="alert"
            class="text-destructive mt-3 break-words text-sm"
          >
            {{ s.task.value.error }}
          </p>
          <pre
            v-if="s.task.value?.output"
            class="bg-muted/70 mt-3 max-h-72 overflow-auto rounded-xl p-4 font-mono text-xs whitespace-pre-wrap"
            >{{ s.task.value.output }}</pre>
        </SectionCard>
        <SectionCard
          v-if="section === 'compatibility'"
          :title="t('nav.compatibility')"
          :description="t('compatibilityHelp')"
        >
          <div class="flex flex-col gap-5">
            <Field>
              <FieldLabel for="pif-mode">{{ t("mode") }}</FieldLabel>
              <Select v-model="s.options.value.mode"
                ><SelectTrigger id="pif-mode" class="w-full"><SelectValue /></SelectTrigger
                ><SelectContent
                  ><SelectItem value="full">{{ t("full") }}</SelectItem
                  ><SelectItem value="scripts-only">{{
                    t("scriptsOnly")
                  }}</SelectItem></SelectContent
                ></Select
              >
            </Field>
            <Field
              v-for="item in [
                ['skipdelprop', 'skipDelete'],
                ['skippersistprop', 'skipPersist'],
                ['skipmatchdevice', 'skipMatch'],
              ] as const"
              :key="item[0]"
              orientation="horizontal"
            >
              <FieldLabel :for="`pif-${item[0]}`">{{ t(item[1]) }}</FieldLabel
              ><Switch :id="`pif-${item[0]}`" v-model="s.options.value[item[0]]" />
            </Field>
            <p class="text-muted-foreground text-sm">{{ t("denylistHelp") }}</p>
            <p class="text-muted-foreground text-sm">{{ t("selinuxHelp") }}</p>
          </div>
          <p class="text-muted-foreground mt-5 text-sm leading-6">{{ t("replaceHelp") }}</p>
          <label class="mt-3 block text-sm"
            >{{ t("replaceList")
            }}<Textarea
              v-model="list"
              class="mt-2 min-h-40 font-mono text-sm"
              :aria-label="t('replaceList')"
          /></label>
          <Button class="mt-3" :disabled="s.busy.value" @click="s.saveOptions(list)">{{
            global.t("core.actions.save")
          }}</Button>
        </SectionCard>
        <SectionCard
          v-if="section === 'diagnostics'"
          :title="t('nav.diagnostics')"
          :description="t('limits')"
        >
          <SettingDetails :title="t('editing.logs')">
            <div class="flex flex-wrap gap-2">
              <Button :disabled="s.busy.value" @click="s.diagnose()">{{ t("readLogs") }}</Button
              ><Button variant="outline" :disabled="s.busy.value" @click="s.restart()">{{
                t("restart")
              }}</Button>
            </div>
            <pre
              class="bg-muted/70 mt-4 max-h-[60vh] overflow-auto rounded-xl p-4 font-mono text-xs whitespace-pre-wrap"
              >{{ s.log.value || "—" }}</pre>
          </SettingDetails>
        </SectionCard>
        <ConfigInspection v-if="section === 'diagnostics'" />
        <SectionCard v-if="section === 'diagnostics'" :title="t('history.title')">
          <SettingDetails :title="t('history.title')"
            ><TestHistory :revision="s.config.data.value?.revision"
          /></SettingDetails>
        </SectionCard>
        <RuleGuide v-if="section === 'license'" />
        <SectionCard
          v-if="section === 'license'"
          :title="t('nav.license')"
          :description="t('licenseScope')"
          ><p class="text-sm">{{ t("attribution") }}</p>
          <p class="text-muted-foreground mt-2 break-all font-mono text-xs">
            {{ data?.upstream_revision }}
          </p></SectionCard
        >
      </QueryState>
      <ConfigPreview
        v-if="s.preview.value && ['config', 'generate', 'archive'].includes(section)"
        :preview="s.preview.value"
        :patches="s.patches.value"
        :busy="
          s.busy.value ||
          s.transaction.isPending.value ||
          !!s.transaction.error.value ||
          !!s.transaction.data.value?.pending
        "
        @apply="s.action(s.apply)"
        @patch-preview="s.patchPreview"
        @select="
          (id, selected) => {
            const patch = s.patches.value.find((item) => item.target.id === id)
            if (patch) patch.selected = selected
          }
        "
        @close="s.clearPreview()"
      />
      <div v-if="s.backup.value" class="flex gap-2">
        <Button variant="outline" :disabled="s.busy.value" @click="s.restart()">{{
          t("restart")
        }}</Button>
      </div>
    </fieldset>
    <LicenseFooter />
    <FilePickerDialog
      v-model:open="picker"
      extension=""
      :start="pickerStart"
      root="/"
      @pick="picked"
    />
  </div>
</template>
