<script setup lang="ts">
import { computed, ref } from "vue"
import { Languages, Plus, Power, PowerOff, RefreshCw, Pencil, Trash2, Save } from "@lucide/vue"
import { useI18n } from "vue-i18n"
import { Button } from "@/components/ui/button"
import AppIcon from "@/core/components/AppIcon.vue"
import QueryState from "@/core/components/QueryState.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import ConfirmDialog from "@/core/components/ConfirmDialog.vue"
import { displayName } from "@/core/i18n"
import { useRouteModal } from "@/core/modal"
import { notifyError } from "@/core/notify"
import { useMiscI18n } from "../i18n"
import type { LanguageRule } from "./api"
import { useLanguageEditor } from "./useLanguageEditor"
import RuleDialog from "./RuleDialog.vue"

const { t } = useMiscI18n()
const { t: coreT } = useI18n()
const editor = useLanguageEditor()
const { status, draft, dirty, changed, busy, stopping, confirmation } = editor
const data = computed(() => status.data.value)
const modal = useRouteModal("language-rule")
const editing = ref(-1)
const rule = ref<LanguageRule | null>(null)
const editRevision = ref("")
function edit(index: number) {
  editing.value = index
  const selected = draft.value[index]
  rule.value = selected ? { ...selected } : null
  editRevision.value = data.value?.revision ?? ""
  modal.show()
}
function upsert(rule: LanguageRule) {
  if (data.value?.revision !== editRevision.value) {
    notifyError(new Error(t("language.changed")), t("errors.stale_revision"))
    return
  }
  if (
    draft.value.some(
      (other, index) =>
        index !== editing.value &&
        other.package_name === rule.package_name &&
        other.user_id === rule.user_id,
    )
  ) {
    notifyError(new Error(t("errors.duplicate_rule")), t("errors.duplicate_rule"))
    return
  }
  if (editing.value < 0) draft.value.push(rule)
  else draft.value.splice(editing.value, 1, rule)
  modal.close()
}
async function remove(index: number) {
  const selected = draft.value[index]
  if (!selected) return
  if (
    await confirmation.confirm({
      title: t("language.removeRule"),
      description: `${selected.package_name} · ${t("language.userLabel", { id: selected.user_id })}`,
      destructive: true,
    })
  )
    draft.value = draft.value.filter(
      (rule) => rule.package_name !== selected.package_name || rule.user_id !== selected.user_id,
    )
}
</script>

<template>
  <div class="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 pb-8 [&_section>header]:px-0">
    <section
      class="bg-primary/8 flex flex-col gap-5 rounded-3xl p-4"
      :aria-label="t('language.title')"
    >
      <div class="flex items-start gap-4">
        <span
          class="bg-primary/12 text-primary hidden size-12 shrink-0 items-center justify-center rounded-2xl min-[480px]:flex"
          ><Languages class="size-6" aria-hidden="true"
        /></span>
        <div class="min-w-0 flex-1">
          <h2 class="text-muted-foreground mb-1 text-sm font-medium">{{ t("language.title") }}</h2>
          <p role="status" class="text-xl leading-7 font-semibold">
            {{
              !data
                ? coreT("core.home.checking")
                : data.enabled
                  ? t("language.enabled")
                  : t("language.disabled")
            }}
          </p>
        </div>
      </div>
      <div class="flex flex-wrap gap-3">
        <Button
          :disabled="
            busy ||
            stopping ||
            !data ||
            status.isError.value ||
            data.enabled ||
            !!data.config_error ||
            !data.config?.rules.length ||
            !data.payload_installed ||
            !data.parent_active ||
            dirty ||
            changed
          "
          @click="editor.enable"
          ><Power aria-hidden="true" />{{ t("language.enable") }}</Button
        >
        <Button variant="outline" :disabled="busy || stopping" @click="editor.disable"
          ><PowerOff aria-hidden="true" />{{ t("language.disable") }}</Button
        >
        <Button variant="ghost" :disabled="busy || stopping" @click="editor.refresh"
          ><RefreshCw aria-hidden="true" />{{ coreT("core.actions.refresh") }}</Button
        >
      </div>
      <p class="text-muted-foreground text-sm leading-6">{{ t("language.restart") }}</p>
    </section>
    <QueryState
      :loading="status.isPending.value"
      :error="status.error.value"
      @retry="editor.refresh"
    >
      <template v-if="data">
        <p
          v-if="!data.payload_installed || !data.parent_active"
          role="alert"
          class="bg-muted rounded-2xl p-4 text-sm leading-6"
        >
          {{ t("language.unavailable") }}
        </p>
        <p
          v-if="data.config_error"
          role="alert"
          class="bg-destructive/5 text-destructive rounded-2xl p-4 text-sm leading-6"
        >
          {{ t("errors.invalid_config") }}
        </p>
        <p v-if="changed" role="alert" class="bg-muted rounded-2xl p-4 text-sm leading-6">
          {{ t("language.changed") }}
        </p>
        <SectionCard :title="t('language.rules')" plain>
          <template #action
            ><Button
              variant="outline"
              size="sm"
              :disabled="busy || stopping || draft.length >= 256 || !!data.config_error"
              @click="edit(-1)"
              ><Plus aria-hidden="true" />{{ coreT("core.actions.add") }}</Button
            ></template
          >
          <p
            v-if="!draft.length"
            class="bg-card text-muted-foreground rounded-lg p-4 text-sm leading-6"
          >
            {{ t("language.empty") }}
          </p>
          <div v-else class="flex flex-col gap-1.5">
            <article
              v-for="(item, index) in draft"
              :key="`${item.user_id}:${item.package_name}`"
              class="bg-card flex flex-wrap items-center gap-3 rounded-lg p-4"
            >
              <AppIcon :package-name="item.package_name" />
              <div class="min-w-0 flex-[1_1_10rem]">
                <p dir="ltr" class="break-all text-sm font-medium">{{ item.package_name }}</p>
                <p class="text-muted-foreground text-sm leading-6">
                  {{ displayName(item.locale) }} ·
                  {{ t("language.userLabel", { id: item.user_id }) }}
                </p>
              </div>
              <div class="ms-auto flex shrink-0 gap-1">
                <Button
                  variant="ghost"
                  size="icon"
                  :aria-label="`${coreT('core.actions.edit')} ${item.package_name}`"
                  :disabled="busy || stopping"
                  @click="edit(index)"
                  ><Pencil aria-hidden="true"
                /></Button>
                <Button
                  variant="ghost"
                  size="icon"
                  :aria-label="`${coreT('core.actions.remove')} ${item.package_name}`"
                  :disabled="busy || stopping"
                  @click="remove(index)"
                  ><Trash2 aria-hidden="true"
                /></Button>
              </div>
            </article>
          </div>
          <div class="mt-4 flex flex-wrap items-center gap-3">
            <Button
              :disabled="
                busy || stopping || !dirty || changed || !!data.config_error || status.isError.value
              "
              @click="editor.save"
              ><Save aria-hidden="true" />{{ coreT("core.actions.save") }}</Button
            >
            <span v-if="dirty" role="status" class="text-muted-foreground text-sm">{{
              t("language.unsaved")
            }}</span>
          </div>
        </SectionCard>
      </template>
    </QueryState>
    <SectionCard :title="t('language.scope')"
      ><p class="text-muted-foreground text-sm leading-6">
        {{ t("language.limits") }}
      </p></SectionCard
    >
    <RuleDialog v-model:open="modal.open.value" :rule="rule" @save="upsert" />
    <ConfirmDialog
      v-model:open="confirmation.open.value"
      :title="confirmation.pending.value?.title ?? ''"
      :description="confirmation.pending.value?.description"
      :destructive="confirmation.pending.value?.destructive"
      :confirm-label="confirmation.pending.value?.confirmLabel"
      @confirm="confirmation.accept"
    />
  </div>
</template>
