<script setup lang="ts">
import { computed, onMounted, ref } from "vue"
import { Boxes, HardDrive, MoonStar, ShieldCheck, SunMedium } from "lucide-vue-next"

import AboutCard from "@/features/shared/AboutCard.vue"
import NoticeDialog from "@/features/shared/NoticeDialog.vue"
import { useI18n } from "@/i18n"
import { APP_META } from "@/lib/meta"
import { useTheme } from "@/lib/theme"
import CommandLogPanel from "@/features/shared/CommandLogPanel.vue"
import TextPreviewDialog from "@/features/shared/TextPreviewDialog.vue"
import RkpWorkbench from "@/features/rkp/RkpWorkbench.vue"
import { useRkpWorkbench } from "@/features/rkp/useRkpWorkbench"

import ToolboxHome from "./ToolboxHome.vue"
import ToolLibrary from "./ToolLibrary.vue"

const OPEN_SOURCE_NOTICE_KEY = `duck-toolbox/open-source-notice/${APP_META.version}`
const activeTool = ref("home")
const openSourceNoticeOpen = ref(false)
const rkp = useRkpWorkbench()
const { locale, locales, t } = useI18n()
const { theme } = useTheme()
const historyCount = computed(() => rkp.historyCount.value)
const moduleRoot = computed(() => rkp.moduleRoot.value)
const outputDirectory = computed(() => rkp.outputDirectory.value)
const secretPath = computed(() => rkp.secretPath.value)
const bridgeUnavailable = computed(() => rkp.state.bridge.mode === "unavailable")
const bridgeAvailable = computed(() => rkp.state.bridge.mode === "kernelsu")
const projectAddress = computed(() => rkp.state.paths?.root ?? moduleRoot.value)
const themeOptions = computed(() => [
  {
    value: "light" as const,
    label: t("shell.themeLight"),
    icon: SunMedium,
  },
  {
    value: "dark" as const,
    label: t("shell.themeDark"),
    icon: MoonStar,
  },
])

const tools = computed(() => [
  {
    id: "rkp",
    name: t("tool.rkpName"),
    category: t("tool.rkpCategory"),
    summary: t("tool.rkpSummary"),
    capabilities: [
      t("workspace.profile"),
      t("workspace.provision"),
      t("workspace.verify"),
    ],
  },
])

const runtimeStatusLabel = computed(() =>
  bridgeAvailable.value
    ? t("shell.runtimeAvailable")
    : t("shell.runtimeUnavailable"),
)

const summaryCards = computed(() => [
  {
    label: t("shell.runtime"),
    value: runtimeStatusLabel.value,
    detail: t("shell.runtimeDetail"),
    icon: ShieldCheck,
    iconClass: bridgeAvailable.value ? "status-icon-online" : "status-icon-offline",
    valueClass: bridgeAvailable.value ? "status-text-online" : "status-text-offline",
  },
  {
    label: t("shell.installedTools"),
    value: `${tools.value.length}`,
    detail: t("shell.installedDetail"),
    icon: Boxes,
    iconClass: "",
    valueClass: "",
  },
  {
    label: t("shell.outputs"),
    value: outputDirectory.value,
    detail: t("shell.outputsDetail"),
    icon: HardDrive,
    iconClass: "",
    valueClass: "",
  },
])

function toggleTool(toolId: string) {
  activeTool.value = activeTool.value === toolId ? "home" : toolId
}

function dismissOpenSourceNotice() {
  openSourceNoticeOpen.value = false

  try {
    globalThis.localStorage?.setItem(OPEN_SOURCE_NOTICE_KEY, "1")
  } catch {
    // Ignore localStorage failures and fall back to per-load prompting.
  }
}

onMounted(() => {
  try {
    openSourceNoticeOpen.value =
      globalThis.localStorage?.getItem(OPEN_SOURCE_NOTICE_KEY) !== "1"
  } catch {
    openSourceNoticeOpen.value = true
  }
})
</script>

<template>
  <div class="min-h-screen bg-background text-foreground">
    <div class="app-noise" />

    <main class="shell">
      <header class="panel hero-panel">
        <div class="hero-grid">
          <div class="space-y-4">
            <div class="flex flex-wrap items-center gap-3">
              <div class="brand-lockup">
                <img alt="Duck ToolBox logo" class="brand-logo" src="/duck-logo.svg">
                <div class="mono-chip brand-badge">{{ t("shell.badge") }}</div>
              </div>
              <div class="preference-switch language-switch">
                <span class="summary-label">{{ t("shell.language") }}</span>
                <div class="mode-picker">
                  <label
                    v-for="entry in locales"
                    :key="entry.value"
                    class="mode-option"
                  >
                    <input
                      v-model="locale"
                      class="sr-only"
                      type="radio"
                      :value="entry.value"
                    >
                    {{ entry.label }}
                  </label>
                </div>
              </div>
              <div class="preference-switch theme-switch">
                <span class="summary-label">{{ t("shell.theme") }}</span>
                <div class="mode-picker">
                  <label
                    v-for="entry in themeOptions"
                    :key="entry.value"
                    class="mode-option"
                  >
                    <input
                      v-model="theme"
                      class="sr-only"
                      type="radio"
                      :value="entry.value"
                    >
                    <component :is="entry.icon" class="size-4" />
                    {{ entry.label }}
                  </label>
                </div>
              </div>
            </div>
            <div class="space-y-3">
              <h1 class="headline">{{ t("shell.title") }}</h1>
              <p class="body-copy max-w-3xl">{{ t("shell.description") }}</p>
            </div>
          </div>

          <div class="summary-grid">
            <article
              v-for="card in summaryCards"
              :key="card.label"
              class="summary-tile"
            >
              <div class="panel-heading compact">
                <span class="summary-label">{{ card.label }}</span>
                <component :is="card.icon" :class="['icon-muted', card.iconClass]" />
              </div>
              <p :class="['mono-inline mt-3 break-all', card.valueClass]">{{ card.value }}</p>
              <p class="muted mt-2">{{ card.detail }}</p>
            </article>
          </div>
        </div>
      </header>

      <section class="shell-grid">
        <aside class="stack">
          <ToolLibrary
            :active-tool="activeTool"
            :tools="tools"
            @select="toggleTool"
          />
        </aside>

        <section class="stack">
          <p v-if="bridgeUnavailable" class="error-banner runtime-alert">
            {{ t("messages.ksuUnavailable") }}
          </p>
          <ToolboxHome
            v-if="activeTool === 'home'"
            :bridge="rkp.state.bridge"
            :module-root="moduleRoot"
            :output-directory="outputDirectory"
            :runtime-status-label="runtimeStatusLabel"
          />
          <RkpWorkbench
            v-else
            :actions="rkp.actions"
            :history-count="historyCount"
            :module-root="moduleRoot"
            :output-directory="outputDirectory"
            :secret-path="secretPath"
            :state="rkp.state"
          />
        </section>

        <CommandLogPanel
          :history="rkp.state.history"
          :last-error="rkp.state.lastError"
        />
      </section>

      <section class="stack mt-4">
        <AboutCard
          :authors="APP_META.authors"
          :project-address="projectAddress"
          :repository-url="APP_META.repositoryUrl"
          :version="APP_META.version"
        />
      </section>
    </main>

    <TextPreviewDialog
      :content="rkp.state.errorDialogText"
      :copy-label="t('actions.copyError')"
      :close-label="t('dialog.close')"
      :description="t('dialog.errorDescription')"
      :open="rkp.state.errorDialogOpen"
      :title="t('dialog.errorTitle')"
      @close="rkp.actions.dismissErrorDialog()"
      @copy="rkp.actions.copyText(rkp.state.errorDialogText)"
    />

    <NoticeDialog
      :body="t('notice.openSourceBody')"
      :close-label="t('notice.dismiss')"
      :description="t('notice.openSourceDescription')"
      :link-href="APP_META.repositoryUrl"
      :link-label="t('notice.openRepository')"
      :link-text="APP_META.repositoryUrl"
      :open="openSourceNoticeOpen"
      :title="t('notice.openSourceTitle')"
      @close="dismissOpenSourceNotice()"
    />
  </div>
</template>
