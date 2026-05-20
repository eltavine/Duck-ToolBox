<script setup lang="ts">
import { computed, ref } from "vue"
import {
  Check,
  Copy,
  FileKey2,
  Folder,
  FolderOpen,
  LoaderCircle,
  Plus,
  RefreshCcw,
  Save,
  Search,
  ShieldCheck,
  ShieldQuestion,
  SlidersHorizontal,
  Trash2,
} from "lucide-vue-next"

import { useI18n } from "@/i18n"
import type { TrickyStoreTargetMode } from "@/lib/types"

import type { TrickyStoreWorkbenchActions, TrickyStoreWorkbenchState } from "./types"

const props = defineProps<{
  state: TrickyStoreWorkbenchState
  actions: TrickyStoreWorkbenchActions
}>()

const { t } = useI18n()
const systemAppInput = ref("")

const visiblePackages = computed(() => {
  const query = props.state.search.trim().toLowerCase()
  if (!query) {
    return props.state.packages
  }

  return props.state.packages.filter((entry) => {
    const haystack = `${entry.app_label} ${entry.package_name}`.toLowerCase()
    return haystack.includes(query)
  })
})

const selectedCount = computed(() => props.state.targets.length)
const systemAppCount = computed(() => props.state.systemApps.length)
const trickyStoreStatus = computed(() =>
  props.state.status?.tricky_store.installed
    ? t("trickyStore.installed")
    : t("trickyStore.notInstalled"),
)
const teeSimulatorStatus = computed(() =>
  props.state.status?.tee_simulator.installed
    ? t("trickyStore.installed")
    : t("trickyStore.notInstalled"),
)
const keyboxStatus = computed(() =>
  props.state.status?.keybox.exists
    ? t("trickyStore.keyboxPresent")
    : t("trickyStore.keyboxMissing"),
)

const modeOptions = computed<Array<{ value: TrickyStoreTargetMode; label: string }>>(() => [
  { value: "auto", label: t("trickyStore.modeAuto") },
  { value: "generate", label: t("trickyStore.modeGenerate") },
  { value: "hack", label: t("trickyStore.modeHack") },
])

function modeClass(mode: TrickyStoreTargetMode) {
  return {
    "mode-generate": mode === "generate",
    "mode-hack": mode === "hack",
  }
}

function addSystemApp() {
  props.actions.addSystemApp(systemAppInput.value)
  systemAppInput.value = ""
}

function humanTime(unix?: number) {
  if (!unix) {
    return t("trickyStore.notAvailable")
  }
  return new Date(unix * 1000).toLocaleString()
}
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("tool.workspaceKicker") }}</div>
        <h2 class="panel-title">{{ t("tool.trickyStoreName") }}</h2>
        <p class="body-copy mt-3 max-w-3xl">{{ t("tool.trickyStoreDescription") }}</p>
      </div>
      <div class="status-stack compact">
        <div class="status-card">
          <span class="status-kicker">{{ t("trickyStore.selectedTargets") }}</span>
          <strong>{{ selectedCount }}</strong>
        </div>
        <div class="status-card">
          <span class="status-kicker">{{ t("trickyStore.systemApps") }}</span>
          <strong>{{ systemAppCount }}</strong>
        </div>
      </div>
    </div>

    <div class="toolbar">
      <button class="action-primary" :disabled="state.busy.save || state.bridge.mode === 'unavailable'" @click="actions.saveTargets()">
        <LoaderCircle v-if="state.busy.save" class="size-4 animate-spin" />
        <Save v-else class="size-4" />
        {{ t("actions.saveTrickyStoreTargets") }}
      </button>
      <button class="action-secondary" :disabled="state.busy.status || state.bridge.mode === 'unavailable'" @click="actions.refresh()">
        <LoaderCircle v-if="state.busy.status" class="size-4 animate-spin" />
        <RefreshCcw v-else class="size-4" />
        {{ t("actions.refreshTrickyStore") }}
      </button>
    </div>

    <div class="summary-grid wide">
      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">Tricky Store</span>
          <ShieldCheck :class="['icon-muted', state.status?.tricky_store.installed ? 'status-icon-online' : 'status-icon-offline']" />
        </div>
        <p class="mono-inline mt-2">{{ trickyStoreStatus }}</p>
        <p class="muted mt-2 break-all">{{ state.status?.tricky_store.module_dir ?? "/data/adb/modules/tricky_store" }}</p>
      </article>
      <article class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">TEE Simulator</span>
          <ShieldQuestion :class="['icon-muted', state.status?.tee_simulator.installed ? 'status-icon-online' : 'status-icon-offline']" />
        </div>
        <p class="mono-inline mt-2">{{ teeSimulatorStatus }}</p>
        <p class="muted mt-2 break-all">{{ state.status?.tee_simulator.module_dir ?? "/data/adb/modules/tee_simulator" }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("trickyStore.targetFile") }}</span>
        <p class="mono-inline mt-2 break-all">{{ state.status?.target_path ?? "/data/adb/tricky_store/target.txt" }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("trickyStore.keybox") }}</span>
        <p class="mono-inline mt-2">{{ keyboxStatus }}</p>
        <p class="muted mt-2 break-all">{{ state.status?.keybox.path ?? "/data/adb/tricky_store/keybox.xml" }}</p>
      </article>
    </div>

    <section class="panel-subsection">
      <div class="panel-heading compact">
        <div>
          <div class="section-kicker">{{ t("trickyStore.targetsKicker") }}</div>
          <h3 class="subheading">{{ t("trickyStore.targetsTitle") }}</h3>
        </div>
        <SlidersHorizontal class="icon-muted" />
      </div>

      <p class="security-note">
        <ShieldQuestion class="size-4" />
        {{ t("trickyStore.modeNote") }}
      </p>

      <div class="toolbar tricky-toolbar">
        <label class="search-field">
          <Search class="size-4 icon-muted" />
          <input v-model="state.search" class="search-input-native" :placeholder="t('trickyStore.searchPlaceholder')">
        </label>
        <button class="action-secondary" type="button" @click="actions.selectAllVisible(true)">
          <Check class="size-4" />
          {{ t("actions.selectAllVisible") }}
        </button>
        <button class="action-secondary" type="button" @click="actions.selectAllVisible(false)">
          <Trash2 class="size-4" />
          {{ t("actions.clearVisible") }}
        </button>
      </div>

      <label class="switch-row">
        <input v-model="state.autoAddNewApps" type="checkbox">
        <span>
          <strong>{{ t("trickyStore.autoAddNewApps") }}</strong>
          <small>{{ t("trickyStore.autoAddNewAppsDescription") }}</small>
        </span>
      </label>

      <div class="stack-list mt-4 app-target-list">
        <article
          v-for="entry in visiblePackages"
          :key="entry.package_name"
          class="list-row app-target-row"
          :class="{ 'is-selected': entry.selected }"
        >
          <button class="target-main" type="button" @click="actions.togglePackage(entry.package_name)">
            <span class="target-check" :class="{ 'is-on': entry.selected }">
              <Check v-if="entry.selected" class="size-4" />
            </span>
            <span>
              <span class="subheading">{{ entry.app_label }}</span>
              <span class="mono-inline muted target-package">{{ entry.package_name }}</span>
              <span class="chip-row mt-2">
                <span class="tool-pill">{{ entry.system ? t("trickyStore.system") : t("trickyStore.user") }}</span>
                <span v-if="entry.tracked_system" class="tool-pill">{{ t("trickyStore.managedSystem") }}</span>
              </span>
            </span>
          </button>

          <div class="segmented-control">
            <button
              v-for="option in modeOptions"
              :key="option.value"
              :class="['segment-button', modeClass(option.value), { 'is-active': entry.selected && entry.mode === option.value }]"
              type="button"
              @click="actions.setMode(entry.package_name, option.value)"
            >
              {{ option.label }}
            </button>
          </div>
        </article>
      </div>

      <p v-if="!visiblePackages.length" class="empty-copy mt-4">
        {{ t("trickyStore.noPackages") }}
      </p>
    </section>

    <section class="panel-subsection">
      <div class="panel-heading compact">
        <div>
          <div class="section-kicker">{{ t("trickyStore.systemAppsKicker") }}</div>
          <h3 class="subheading">{{ t("trickyStore.systemAppsTitle") }}</h3>
        </div>
        <Plus class="icon-muted" />
      </div>

      <div class="copy-row">
        <input
          v-model="systemAppInput"
          class="text-input"
          placeholder="com.android.vending"
          @keyup.enter="addSystemApp()"
        >
        <button class="action-secondary" type="button" @click="addSystemApp()">
          <Plus class="size-4" />
          {{ t("actions.addSystemApp") }}
        </button>
      </div>

      <div v-if="state.systemApps.length" class="chip-row mt-4">
        <span
          v-for="packageName in state.systemApps"
          :key="packageName"
          class="system-chip"
        >
          <span class="mono-inline">{{ packageName }}</span>
          <button class="icon-button compact-icon" type="button" @click="actions.removeSystemApp(packageName)">
            <Trash2 class="size-3" />
          </button>
        </span>
      </div>
      <p v-else class="empty-copy mt-4">{{ t("trickyStore.noSystemApps") }}</p>
    </section>

    <section class="panel-subsection">
      <div class="panel-heading compact">
        <div>
          <div class="section-kicker">{{ t("trickyStore.keyboxKicker") }}</div>
          <h3 class="subheading">{{ t("trickyStore.keyboxTitle") }}</h3>
        </div>
        <FileKey2 class="icon-muted" />
      </div>

      <div class="field-grid two-up">
        <div class="field-group">
          <label class="field-label" for="tricky-keybox-source">{{ t("trickyStore.keyboxSource") }}</label>
          <div class="copy-row">
            <input id="tricky-keybox-source" v-model="state.keyboxSourcePath" class="text-input">
            <button class="icon-button" type="button" :title="t('actions.chooseLocalKeybox')" @click="actions.openFileDialog()">
              <FolderOpen class="size-4" />
            </button>
          </div>
        </div>
        <div class="summary-tile keybox-meta">
          <span class="summary-label">{{ t("trickyStore.currentKeybox") }}</span>
          <p class="mono-inline mt-2 break-all">{{ state.status?.keybox.path ?? "/data/adb/tricky_store/keybox.xml" }}</p>
          <p class="muted mt-2">
            {{ state.status?.keybox.size ?? 0 }} bytes · {{ humanTime(state.status?.keybox.modified_unix) }}
          </p>
        </div>
      </div>

      <div class="toolbar mt-4">
        <button class="action-primary" :disabled="state.busy.keybox || state.bridge.mode === 'unavailable'" @click="actions.installKeybox()">
          <LoaderCircle v-if="state.busy.keybox" class="size-4 animate-spin" />
          <FileKey2 v-else class="size-4" />
          {{ t("actions.installLocalKeybox") }}
        </button>
        <button
          v-if="state.keyboxInstallResult"
          class="action-secondary"
          type="button"
          @click="actions.copyText(state.keyboxInstallResult.target_path)"
        >
          <Copy class="size-4" />
          {{ t("actions.copyReportPath") }}
        </button>
      </div>

      <article v-if="state.keyboxInstallResult" class="list-row">
        <div>
          <p class="mono-inline break-all">{{ state.keyboxInstallResult.target_path }}</p>
          <p v-if="state.keyboxInstallResult.backup_path" class="muted mt-1 break-all">
            {{ t("trickyStore.backupPath") }} {{ state.keyboxInstallResult.backup_path }}
          </p>
        </div>
      </article>
    </section>

    <Teleport to="body">
      <div
        v-if="state.fileDialogOpen"
        class="dialog-backdrop"
        @click.self="actions.closeFileDialog()"
      >
        <section class="dialog-card file-browser-dialog" role="dialog" aria-modal="true" :aria-label="t('trickyStore.chooseKeybox')">
          <div class="panel-heading compact">
            <div>
              <h3 class="panel-title dialog-title">{{ t("trickyStore.chooseKeybox") }}</h3>
              <p class="mono-inline mt-2 break-all">{{ state.fileList?.path ?? state.filePath }}</p>
            </div>
            <button class="icon-button" type="button" @click="actions.closeFileDialog()">
              <Trash2 class="size-4" />
            </button>
          </div>

          <div class="toolbar">
            <button
              class="action-secondary"
              :disabled="!state.fileList?.parent || state.busy.files"
              type="button"
              @click="state.fileList?.parent && actions.listFiles(state.fileList.parent)"
            >
              <Folder class="size-4" />
              ..
            </button>
            <button class="action-secondary" :disabled="state.busy.files" type="button" @click="actions.listFiles(state.filePath)">
              <LoaderCircle v-if="state.busy.files" class="size-4 animate-spin" />
              <RefreshCcw v-else class="size-4" />
              {{ t("actions.refreshTrickyStore") }}
            </button>
          </div>

          <div class="stack-list file-browser-list">
            <button
              v-for="entry in state.fileList?.entries ?? []"
              :key="entry.path"
              class="list-row file-browser-row"
              type="button"
              @click="entry.directory ? actions.listFiles(entry.path) : actions.chooseFile(entry.path)"
            >
              <span class="copy-row">
                <Folder v-if="entry.directory" class="size-4 icon-muted" />
                <FileKey2 v-else class="size-4 icon-muted" />
                <span>
                  <span class="subheading">{{ entry.name }}</span>
                  <span class="mono-inline muted target-package">{{ entry.path }}</span>
                </span>
              </span>
              <span v-if="!entry.directory" class="muted">{{ entry.size }} bytes</span>
            </button>
          </div>

          <p v-if="state.fileList && !state.fileList.entries.length" class="empty-copy mt-4">
            {{ t("trickyStore.noKeyboxFiles") }}
          </p>
        </section>
      </div>
    </Teleport>
  </section>
</template>
