<script setup lang="ts">
import { LoaderCircle, Radar } from "lucide-vue-next"

import { useI18n } from "@/i18n"
import type { RkpWorkbenchActions, RkpWorkbenchState } from "../types"

defineProps<{
  state: RkpWorkbenchState
  actions: RkpWorkbenchActions
}>()

const { t } = useI18n()
</script>

<template>
  <section class="panel-subsection">
    <div class="toolbar">
      <input
        v-model="state.verifyPath"
        class="text-input flex-1"
        :disabled="state.bridge.mode === 'unavailable'"
        :placeholder="t('verify.placeholder')"
      >
      <button class="action-primary" :disabled="state.busy.verify || state.bridge.mode === 'unavailable'" @click="actions.runVerify()">
        <LoaderCircle v-if="state.busy.verify" class="size-4 animate-spin" />
        <Radar v-else class="size-4" />
        {{ t("actions.verify") }}
      </button>
    </div>

    <div v-if="state.verifyResult" class="kv-grid">
      <div class="kv-item">
        <span class="summary-label">{{ t("status.path") }}</span>
        <p class="mono-inline break-all">{{ state.verifyResult.path }}</p>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("status.signature") }}</span>
        <strong>
          {{
            state.verifyResult.report.signature_valid
              ? t("status.valid")
              : t("status.invalid")
          }}
        </strong>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("verify.udsPub") }}</span>
        <p class="mono-inline break-all">{{ state.verifyResult.report.uds_pub_hex }}</p>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("status.keysToSign") }}</span>
        <strong>{{ state.verifyResult.report.keys_to_sign }}</strong>
      </div>
    </div>
    <p v-else class="empty-copy">
      {{ t("verify.empty") }}
    </p>
  </section>
</template>
