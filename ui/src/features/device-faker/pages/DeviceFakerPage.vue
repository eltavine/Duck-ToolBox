<script setup lang="ts">
import { computed, ref, type ComponentPublicInstance } from "vue"
import { Power, PowerOff, Smartphone, RefreshCw, ArrowUpRight } from "@lucide/vue"
import { useI18n } from "vue-i18n"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { openExternal } from "@/core/links"
import { useFakerI18n } from "../i18n"
import { useFakerEditor } from "../useFakerEditor"
import ProfileBuilder from "./ProfileBuilder.vue"
import ConfigSummary from "./ConfigSummary.vue"
import { MAX_DRAFT_CHARACTERS } from "../draft"

const { t } = useFakerI18n()
const global = useI18n()
const editor = useFakerEditor()
const {
  data,
  draft,
  dirty,
  draftTooLarge,
  changed,
  preview,
  busy,
  stopping,
  error,
  errorDetails,
  notice,
  confirm,
  discard,
  importConfirm,
} = editor
function setPreviewRegion(value: Element | ComponentPublicInstance | null) {
  editor.previewRegion.value = value instanceof HTMLElement ? value : undefined
}
const fileInput = ref<HTMLInputElement>()
const sourceCommit = import.meta.env.VITE_DEVICE_FAKER_SOURCE_COMMIT || "main"
const sourceUrl = `https://github.com/eltavine/Duck-ToolBox/tree/${sourceCommit}/components/device-faker`
const prerequisitesReady = computed(
  () =>
    !!data.value &&
    !data.value.config.error &&
    !data.value.conflict &&
    !data.value.payload_pending &&
    data.value.parent_active &&
    data.value.payload_installed &&
    data.value.startup_ready &&
    !data.value.cleanup_pending &&
    !data.value.global_recovery_requires_reboot &&
    !data.value.startup_failed &&
    !!data.value.config.summary?.targets.length,
)
const canEnable = computed(
  () => prerequisitesReady.value && !data.value?.enabled && !busy.value && !stopping.value,
)
const state = computed(() => {
  if (editor.status.isPending.value) return t("loading")
  if (!data.value) return t("unavailable")
  if (data.value.enabled && !prerequisitesReady.value) return t("blocked")
  return data.value.enabled ? t("enabled") : t("disabled")
})
</script>
<template>
  <div
    class="mx-auto flex w-full max-w-3xl flex-col gap-6 p-[16px] pb-8 [&_section>header]:px-[16px]"
  >
    <section class="bg-primary/8 flex flex-col gap-5 rounded-3xl p-[16px]" :aria-label="t('power')">
      <div class="flex items-start gap-4">
        <div
          class="bg-primary/12 text-primary hidden min-[480px]:flex size-12 shrink-0 items-center justify-center rounded-2xl"
        >
          <Smartphone class="size-6" aria-hidden="true" />
        </div>
        <div class="min-w-0 flex-1">
          <h2 class="text-muted-foreground mb-1 text-sm font-medium">{{ t("power") }}</h2>
          <p role="status" class="text-xl leading-7 font-semibold [overflow-wrap:anywhere]">
            {{ state }}
          </p>
        </div>
      </div>
      <div class="flex flex-wrap gap-3">
        <Button :disabled="!canEnable" @click="editor.action(editor.enable)"
          ><Power aria-hidden="true" />{{ t("enable") }}</Button
        >
        <Button variant="outline" :disabled="stopping" @click="editor.disable"
          ><PowerOff aria-hidden="true" />{{ t("disable") }}</Button
        >
        <Button variant="ghost" :disabled="busy" @click="editor.refresh"
          ><RefreshCw aria-hidden="true" />{{ global.t("core.actions.refresh") }}</Button
        >
      </div>
      <p class="text-muted-foreground text-sm leading-6">{{ t("limits") }}</p>
    </section>
    <div
      v-if="error"
      role="alert"
      class="border-destructive/30 bg-destructive/5 rounded-2xl border p-4"
    >
      <p class="text-sm leading-6 [overflow-wrap:anywhere]">{{ error }}</p>
      <details v-if="errorDetails" class="mt-2">
        <summary class="flex min-h-12 cursor-pointer items-center text-sm underline">
          {{ global.t("core.actions.details") }}
        </summary>
        <pre
          dir="ltr"
          class="text-muted-foreground overflow-auto text-xs whitespace-pre-wrap [overflow-wrap:anywhere]"
          >{{ errorDetails }}</pre>
      </details>
    </div>
    <p
      v-if="notice"
      role="status"
      class="bg-secondary text-secondary-foreground rounded-2xl p-4 text-sm leading-6"
    >
      {{ notice }}
    </p>
    <div
      v-if="data?.startup_failed"
      role="alert"
      class="bg-muted rounded-2xl p-4 text-sm leading-6"
    >
      <p>{{ t("errors.startup_failed") }}</p>
      <details v-if="data.startup_error" class="mt-2">
        <summary class="flex min-h-12 cursor-pointer items-center text-sm underline">
          {{ global.t("core.actions.details") }}
        </summary>
        <pre
          dir="ltr"
          class="text-muted-foreground overflow-auto text-xs whitespace-pre-wrap [overflow-wrap:anywhere]"
          >{{ data.startup_error }}</pre>
      </details>
    </div>
    <QueryState
      :loading="editor.status.isPending.value"
      :error="editor.status.error.value ? editor.message(editor.status.error.value) : null"
      @retry="editor.refresh"
    >
      <template v-if="data">
        <p v-if="data.conflict" role="alert" class="bg-muted rounded-2xl p-4 text-sm leading-6">
          {{ t("conflict") }}
        </p>
        <p
          v-if="data.global_recovery_requires_reboot"
          role="alert"
          class="bg-muted rounded-2xl p-4 text-sm leading-6 [overflow-wrap:anywhere]"
        >
          {{ t("globalRecoveryReboot") }}
        </p>
        <p
          v-else-if="data.cleanup_pending"
          role="alert"
          class="bg-muted rounded-2xl p-4 text-sm leading-6"
        >
          {{ t("cleanupHelp") }}
        </p>
        <p
          v-if="
            !data.startup_failed &&
            !data.conflict &&
            !data.cleanup_pending &&
            (data.payload_pending || !data.payload_installed || !data.startup_ready)
          "
          role="status"
          class="bg-muted rounded-2xl p-4 text-sm leading-6"
        >
          {{ t("reboot") }}
        </p>
        <SectionCard
          plain
          content-class="bg-card text-card-foreground rounded-2xl p-[16px]"
          :title="t('profiles')"
          ><ConfigSummary :summary="data.config.summary"
        /></SectionCard>
        <div v-if="data.config.error" role="alert" class="text-destructive min-w-0 px-4 text-sm">
          <p>{{ t("errors.invalid_config") }}</p>
          <details class="mt-2">
            <summary class="flex min-h-12 cursor-pointer items-center underline">
              {{ global.t("core.actions.details") }}
            </summary>
            <pre
              dir="ltr"
              class="text-muted-foreground overflow-auto whitespace-pre-wrap [overflow-wrap:anywhere]"
              >{{ data.config.error }}</pre>
          </details>
        </div>
        <SectionCard
          plain
          content-class="bg-card text-card-foreground rounded-2xl p-[16px]"
          :title="t('config')"
          :description="t('configHelp')"
        >
          <div class="mb-4 flex flex-wrap items-center gap-2">
            <span
              v-if="dirty"
              role="status"
              class="bg-secondary text-secondary-foreground rounded-full px-3 py-1.5 text-sm"
              >{{ t("unsaved") }}</span
            >
            <Button variant="ghost" :disabled="busy || !dirty" @click="discard = true">{{
              t("reload")
            }}</Button>
          </div>
          <p v-if="changed" role="status" class="bg-muted mb-4 rounded-xl p-4 text-sm leading-6">
            {{ t("changed") }}
          </p>
          <fieldset :disabled="busy" class="flex min-w-0 flex-col gap-5">
            <ProfileBuilder @append="editor.appendProfile" />
            <p id="faker-property-help" class="text-muted-foreground text-sm leading-6">
              {{ t("globalPropsHelp", { prefix: "\u2066ro.*\u2069" }) }}
            </p>
            <label for="faker-config" class="text-sm font-medium">{{ t("tomlLabel") }}</label>
            <Textarea
              id="faker-config"
              v-model="draft"
              :maxlength="MAX_DRAFT_CHARACTERS"
              :aria-invalid="draftTooLarge"
              :aria-describedby="
                draftTooLarge ? 'faker-property-help faker-config-size' : 'faker-property-help'
              "
              dir="ltr"
              class="min-h-64 font-mono text-sm leading-6"
              spellcheck="false"
              autocapitalize="off"
              autocomplete="off"
            />
            <p
              v-if="draftTooLarge"
              id="faker-config-size"
              role="alert"
              class="text-destructive text-sm leading-6"
            >
              {{ t("tooLarge") }}
            </p>
            <div class="flex flex-wrap gap-3">
              <Button :disabled="draftTooLarge" @click="editor.action(editor.prepare)">{{
                t("preview")
              }}</Button>
              <Button variant="outline" @click="fileInput?.click()">{{ t("importFile") }}</Button>
              <Button
                v-if="data.legacy_available"
                variant="outline"
                @click="editor.action(editor.importLegacy)"
                >{{ t("importLegacy") }}</Button
              >
              <Button variant="ghost" @click="editor.action(editor.restore)">{{
                t("restore")
              }}</Button>
            </div>
            <input
              ref="fileInput"
              type="file"
              accept=".toml,text/plain"
              class="hidden"
              :aria-label="t('importFile')"
              @change="editor.loadFile"
            />
          </fieldset>
        </SectionCard>
        <div
          v-if="preview"
          :ref="setPreviewRegion"
          tabindex="-1"
          role="region"
          :aria-label="t('preview')"
          class="rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <SectionCard :title="t('preview')">
            <ConfigSummary :summary="preview.summary" />
            <p
              v-if="preview.summary.global_effects"
              class="bg-muted my-4 rounded-xl p-4 text-sm leading-6"
            >
              {{ t("globalWarning") }}
            </p>
            <details class="my-4">
              <summary
                class="flex min-h-12 cursor-pointer items-center text-sm font-medium underline"
              >
                {{ t("saved") }}
              </summary>
              <pre
                dir="ltr"
                class="bg-muted mt-2 max-h-64 overflow-auto rounded-xl p-4 font-mono text-sm whitespace-pre-wrap [overflow-wrap:anywhere]"
                >{{ data.config.content }}</pre>
            </details>
            <pre
              dir="ltr"
              class="bg-muted mt-4 max-h-80 overflow-auto rounded-xl p-4 font-mono text-sm leading-6 whitespace-pre-wrap [overflow-wrap:anywhere]"
              >{{ preview.content }}</pre>
            <Button class="mt-4" :disabled="busy" @click="confirm = true">{{ t("apply") }}</Button>
          </SectionCard>
        </div>
      </template>
    </QueryState>
    <details class="border-t pt-3">
      <summary
        class="flex min-h-12 cursor-pointer items-center rounded-xl px-4 text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {{ t("about") }}
      </summary>
      <div class="text-muted-foreground mt-3 space-y-4 px-4 text-sm leading-6">
        <p>{{ t("stopHelp") }}</p>
        <p>{{ t("packageHelp") }}</p>
        <p>{{ t("attribution") }}</p>
        <div class="flex flex-wrap gap-2">
          <a
            class="inline-flex min-h-12 items-center px-4 text-sm underline"
            :href="sourceUrl"
            @click.prevent="openExternal(sourceUrl)"
            >{{ t("source") }}<ArrowUpRight aria-hidden="true"
          /></a>
          <Button
            variant="link"
            class="min-h-12 underline"
            @click="
              openExternal(
                'https://github.com/Seyud/device_faker/tree/71e778404a8b1ded3271aa09f936f688141d6b31',
              )
            "
            >{{ t("upstream") }}<ArrowUpRight aria-hidden="true"
          /></Button>
          <Button
            variant="link"
            class="min-h-12 underline"
            @click="openExternal('https://www.gnu.org/licenses/gpl-3.0.html')"
            >GPLv3<ArrowUpRight aria-hidden="true"
          /></Button>
        </div>
      </div>
    </details>
    <ConfirmDialog
      v-model:open="confirm"
      :title="t('apply')"
      :confirm-label="t('apply')"
      :description="preview?.summary.global_effects ? t('globalWarning') : t('restartHelp')"
      @confirm="editor.action(editor.apply)"
    />
    <ConfirmDialog
      v-model:open="discard"
      :title="t('reload')"
      :description="t('discard')"
      @confirm="editor.reload"
    />
    <ConfirmDialog
      v-model:open="importConfirm"
      :title="t('importFile')"
      :description="t('discard')"
      @confirm="editor.importFileDraft"
    />
  </div>
</template>
