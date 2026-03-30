<script setup lang="ts">
import { Copy, FolderSync, LoaderCircle, RefreshCcw } from "lucide-vue-next"

import { useI18n } from "@/i18n"
import type { RkpWorkbenchActions, RkpWorkbenchState } from "@/features/rkp/types"

defineProps<{
  state: RkpWorkbenchState
  actions: RkpWorkbenchActions
}>()

const { t } = useI18n()

function humanTime(unix: number) {
  return new Date(unix * 1000).toLocaleString()
}
</script>

<template>
  <section class="panel-subsection">
    <div class="panel-heading compact">
      <div>
        <div class="section-kicker">{{ t("artifacts.kicker") }}</div>
        <h3 class="subheading">{{ t("artifacts.title") }}</h3>
      </div>
      <FolderSync class="icon-muted" />
    </div>

    <div class="toolbar">
      <button class="action-secondary" :disabled="state.busy.artifacts || state.bridge.mode === 'unavailable'" @click="actions.reloadArtifacts()">
        <LoaderCircle v-if="state.busy.artifacts" class="size-4 animate-spin" />
        <RefreshCcw v-else class="size-4" />
        {{ t("actions.refreshArtifacts") }}
      </button>
    </div>

    <div v-if="state.artifacts" class="space-y-3">
      <article
        v-for="artifact in state.artifacts.outputs"
        :key="artifact.path"
        class="list-row"
      >
        <div>
          <p class="mono-inline">{{ artifact.name }}</p>
          <p class="muted mt-1 break-all">{{ artifact.path }}</p>
          <p class="muted mt-1 text-xs">
            {{ artifact.size }} bytes · {{ humanTime(artifact.modified_unix) }}
          </p>
        </div>
        <button class="icon-button" @click="actions.copyText(artifact.path)">
          <Copy class="size-4" />
        </button>
      </article>

      <div class="summary-grid wide">
        <article class="summary-tile">
          <span class="summary-label">{{ t("status.profileFile") }}</span>
          <p class="mono-inline mt-2 break-all">{{ state.artifacts.profile_path }}</p>
        </article>
        <article class="summary-tile">
          <span class="summary-label">{{ t("status.secretsFile") }}</span>
          <p class="mono-inline mt-2 break-all">{{ state.artifacts.profile_secrets_path }}</p>
        </article>
        <article class="summary-tile">
          <span class="summary-label">{{ t("status.logFile") }}</span>
          <p class="mono-inline mt-2 break-all">{{ state.artifacts.log_path }}</p>
        </article>
      </div>
    </div>
    <p v-else class="empty-copy">
      {{ t("artifacts.empty") }}
    </p>
  </section>
</template>
