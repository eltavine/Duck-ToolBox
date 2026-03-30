<script setup lang="ts">
import { computed, toRefs } from "vue"
import { ArrowRightLeft, Copy, Eye, FileJson2, LoaderCircle } from "lucide-vue-next"

import ChoiceDialog from "@/features/shared/ChoiceDialog.vue"
import TextPreviewDialog from "@/features/shared/TextPreviewDialog.vue"
import { useI18n } from "@/i18n"
import type { RkpWorkbenchActions, RkpWorkbenchState } from "../types"

const props = defineProps<{
  state: RkpWorkbenchState
  actions: RkpWorkbenchActions
}>()
const { state, actions } = toRefs(props)

const { t } = useI18n()

const replaceOptions = computed(() => [
  {
    value: "replace",
    label: t("keybox.replacePromptReplaceLabel"),
    description: t("keybox.replacePromptReplaceDescription"),
  },
  {
    value: "skip",
    label: t("keybox.replacePromptSkipLabel"),
    description: t("keybox.replacePromptSkipDescription"),
  },
])

function handleReplacePrompt(value: string | number) {
  if (value === "replace") {
    void actions.value.replaceTrickyStoreKeybox()
    return
  }

  actions.value.closeTrickyStorePrompt()
}
</script>

<template>
  <section class="panel-subsection">
    <div class="toolbar">
      <button
        class="action-primary"
        :disabled="state.busy.keybox || state.busy.replaceTrickyStore || state.bridge.mode === 'unavailable'"
        @click="actions.runKeybox()"
      >
        <LoaderCircle v-if="state.busy.keybox" class="size-4 animate-spin" />
        <FileJson2 v-else class="size-4" />
        {{ t("actions.generateKeybox") }}
      </button>
      <button
        v-if="state.keyboxResult?.keybox_xml"
        class="action-secondary"
        type="button"
        :disabled="state.busy.replaceTrickyStore || state.bridge.mode === 'unavailable'"
        @click="actions.openTrickyStorePrompt()"
      >
        <LoaderCircle v-if="state.busy.replaceTrickyStore" class="size-4 animate-spin" />
        <ArrowRightLeft v-else class="size-4" />
        {{ t("actions.replaceTrickyStoreKeybox") }}
      </button>
      <button
        v-if="state.keyboxResult?.keybox_xml"
        class="action-secondary"
        type="button"
        @click="actions.openKeyboxPreview()"
      >
        <Eye class="size-4" />
        {{ t("actions.previewKeybox") }}
      </button>
    </div>

    <div v-if="state.keyboxResult" class="kv-grid">
      <div class="kv-item">
        <span class="summary-label">{{ t("keybox.keyboxPath") }}</span>
        <div class="copy-row">
          <p class="mono-inline break-all">{{ state.keyboxResult.keybox_path }}</p>
          <button class="icon-button" @click="actions.copyText(state.keyboxResult.keybox_path)">
            <Copy class="size-4" />
          </button>
        </div>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("keybox.deviceId") }}</span>
        <p class="mono-inline">{{ state.keyboxResult.device_id }}</p>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("keybox.csrPath") }}</span>
        <div class="copy-row">
          <p class="mono-inline break-all">{{ state.keyboxResult.csr_path }}</p>
          <button class="icon-button" @click="actions.copyText(state.keyboxResult.csr_path)">
            <Copy class="size-4" />
          </button>
        </div>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("status.certificateCount") }}</span>
        <strong>{{ state.keyboxResult.chain_summary.certificates }}</strong>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("keybox.comment") }}</span>
        <p class="mono-inline">{{ t("keybox.commentValue") }}</p>
      </div>
    </div>
    <p v-else class="empty-copy">
      {{ t("keybox.empty") }}
    </p>

    <TextPreviewDialog
      v-if="state.keyboxResult"
      :content="state.keyboxResult.keybox_xml"
      :copy-label="t('actions.copyKeyboxXml')"
      :close-label="t('dialog.close')"
      :description="t('keybox.previewDescription', { path: state.keyboxResult.keybox_path })"
      :open="state.keyboxPreviewOpen"
      :title="t('keybox.previewTitle')"
      @close="actions.closeKeyboxPreview()"
      @copy="actions.copyText(state.keyboxResult.keybox_xml)"
    />

    <ChoiceDialog
      :description="t('keybox.replacePromptDescription', { target: '/data/adb/tricky_store/keybox.xml' })"
      :open="state.trickyStorePromptOpen"
      :options="replaceOptions"
      :selected="null"
      :title="t('keybox.replacePromptTitle')"
      @close="actions.closeTrickyStorePrompt()"
      @select="handleReplacePrompt"
    />
  </section>
</template>
