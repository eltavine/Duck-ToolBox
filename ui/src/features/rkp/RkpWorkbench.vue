<script setup lang="ts">
import { computed } from "vue"
import { Boxes, FileBadge2, FolderSync, KeyRound, Radar } from "lucide-vue-next"

import { useI18n } from "@/i18n"
import ArtifactsPanel from "@/features/shared/ArtifactsPanel.vue"

import type { RkpWorkbenchActions, RkpWorkbenchState, RkpWorkspaceId } from "./types"
import RkpInfoPanel from "./panels/RkpInfoPanel.vue"
import RkpKeyboxPanel from "./panels/RkpKeyboxPanel.vue"
import RkpProfilePanel from "./panels/RkpProfilePanel.vue"
import RkpProvisionPanel from "./panels/RkpProvisionPanel.vue"
import RkpVerifyPanel from "./panels/RkpVerifyPanel.vue"

const props = defineProps<{
  state: RkpWorkbenchState
  actions: RkpWorkbenchActions
  moduleRoot: string
  outputDirectory: string
  secretPath: string
  historyCount: number
}>()

const { t } = useI18n()
const workspaceItems = computed<
  Array<{ id: RkpWorkspaceId; label: string; icon: unknown }>
>(() => [
  { id: "profile", label: t("workspace.profile"), icon: KeyRound },
  { id: "info", label: t("workspace.info"), icon: Radar },
  { id: "provision", label: t("workspace.provision"), icon: Boxes },
  { id: "keybox", label: t("workspace.keybox"), icon: FileBadge2 },
  { id: "verify", label: t("workspace.verify"), icon: Radar },
  { id: "artifacts", label: t("workspace.artifacts"), icon: FolderSync },
])
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("tool.workspaceKicker") }}</div>
        <h2 class="panel-title">{{ t("tool.rkpName") }}</h2>
        <p class="body-copy mt-3 max-w-3xl">{{ t("tool.rkpDescription") }}</p>
      </div>
      <div class="status-stack compact">
        <div class="status-card">
          <span class="status-kicker">{{ t("status.mode") }}</span>
          <strong>{{ t(state.profile.mode === "hw-key" ? "profile.hwKeyMode" : "profile.seedMode") }}</strong>
        </div>
        <div class="status-card">
          <span class="status-kicker">{{ t("status.commands") }}</span>
          <strong>{{ historyCount }}</strong>
        </div>
      </div>
    </div>

    <div class="summary-grid wide">
      <article class="summary-tile">
        <span class="summary-label">{{ t("status.moduleRoot") }}</span>
        <p class="mono-inline mt-2 break-all">{{ moduleRoot }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("shell.outputs") }}</span>
        <p class="mono-inline mt-2 break-all">{{ outputDirectory }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("status.secretsFile") }}</span>
        <p class="mono-inline mt-2 break-all">{{ secretPath }}</p>
      </article>
    </div>

    <nav class="workspace-nav">
      <button
        v-for="item in workspaceItems"
        :key="item.id"
        class="workspace-button"
        :class="{ 'is-active': state.activeWorkspace === item.id }"
        type="button"
        @click="actions.setWorkspace(item.id)"
      >
        <component :is="item.icon" class="size-4" />
        {{ item.label }}
      </button>
    </nav>

    <RkpProfilePanel
      v-if="state.activeWorkspace === 'profile'"
      :actions="actions"
      :secret-path="secretPath"
      :state="state"
    />
    <RkpInfoPanel
      v-else-if="state.activeWorkspace === 'info'"
      :actions="actions"
      :state="state"
    />
    <RkpProvisionPanel
      v-else-if="state.activeWorkspace === 'provision'"
      :actions="actions"
      :state="state"
    />
    <RkpKeyboxPanel
      v-else-if="state.activeWorkspace === 'keybox'"
      :actions="actions"
      :state="state"
    />
    <RkpVerifyPanel
      v-else-if="state.activeWorkspace === 'verify'"
      :actions="actions"
      :state="state"
    />
    <ArtifactsPanel
      v-else
      :actions="actions"
      :state="state"
    />
  </section>
</template>
