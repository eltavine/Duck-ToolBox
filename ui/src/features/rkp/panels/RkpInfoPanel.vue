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
      <button class="action-primary" :disabled="state.busy.info || state.bridge.mode === 'unavailable'" @click="actions.runInfo()">
        <LoaderCircle v-if="state.busy.info" class="size-4 animate-spin" />
        <Radar v-else class="size-4" />
        {{ t("actions.runInfo") }}
      </button>
    </div>

    <div v-if="state.infoResult" class="kv-grid">
      <div class="kv-item">
        <span class="summary-label">{{ t("status.mode") }}</span>
        <strong>{{ state.infoResult.mode }}</strong>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("profile.fingerprint") }}</span>
        <p class="mono-inline break-all">{{ state.infoResult.fingerprint }}</p>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("info.seed") }}</span>
        <p class="mono-inline break-all">{{ state.infoResult.seed_hex }}</p>
      </div>
      <div class="kv-item">
        <span class="summary-label">{{ t("info.ed25519") }}</span>
        <p class="mono-inline break-all">{{ state.infoResult.ed25519_pubkey_hex }}</p>
      </div>
    </div>
    <p v-else class="empty-copy">
      {{ t("info.empty") }}
    </p>
  </section>
</template>
