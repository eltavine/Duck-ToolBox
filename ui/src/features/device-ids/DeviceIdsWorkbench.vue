<script setup lang="ts">
import { computed } from "vue"
import {
  Copy,
  LoaderCircle,
  Radar,
  RefreshCcw,
  Save,
  ShieldAlert,
  Wrench,
} from "lucide-vue-next"

import { useI18n } from "@/i18n"

import type { DeviceIdsWorkbenchActions, DeviceIdsWorkbenchState } from "./types"

const props = defineProps<{
  state: DeviceIdsWorkbenchState
  actions: DeviceIdsWorkbenchActions
}>()

const { t } = useI18n()

const deviceTitle = computed(() => {
  const values = [
    props.state.profile.brand,
    props.state.profile.model,
  ].filter((value) => value.trim())
  return values.join(" ") || t("deviceIds.notDetected")
})

const deviceCode = computed(() => {
  const values = [
    props.state.profile.device,
    props.state.profile.product,
  ].filter((value) => value.trim())
  return values.join(" / ") || t("deviceIds.notDetected")
})

const serialSummary = computed(
  () => props.state.profile.serial.trim() || t("deviceIds.notDetected"),
)

const manufacturerSummary = computed(
  () => props.state.profile.manufacturer.trim() || t("deviceIds.notDetected"),
)

const resultMode = computed(() =>
  props.state.result?.dry_run
    ? t("deviceIds.dryRunEnabled")
    : t("deviceIds.writeMode"),
)

const runModeButtonClass = computed(() =>
  props.state.profile.dry_run ? "action-primary" : "action-secondary",
)

const runModeButtonLabel = computed(() =>
  props.state.profile.dry_run
    ? t("deviceIds.dryRunEnabled")
    : t("deviceIds.writeMode"),
)

function toggleDryRun() {
  props.state.profile.dry_run = !props.state.profile.dry_run
}
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("tool.workspaceKicker") }}</div>
        <h2 class="panel-title">{{ t("tool.deviceIdsName") }}</h2>
        <p class="body-copy mt-3 max-w-3xl">{{ t("tool.deviceIdsDescription") }}</p>
      </div>
    </div>

    <div class="toolbar">
      <button
        :aria-pressed="state.profile.dry_run"
        :class="runModeButtonClass"
        :title="t('deviceIds.dryRunDescription')"
        type="button"
        @click="toggleDryRun()"
      >
        <ShieldAlert class="size-4" />
        {{ runModeButtonLabel }}
      </button>
      <button class="action-primary" :disabled="state.busy.provision || state.bridge.mode === 'unavailable'" @click="actions.runProvision()">
        <LoaderCircle v-if="state.busy.provision" class="size-4 animate-spin" />
        <Save v-else class="size-4" />
        {{ t("actions.provisionDeviceIds") }}
      </button>
      <button class="action-secondary" :disabled="state.busy.defaults || state.bridge.mode === 'unavailable'" @click="actions.loadDefaults()">
        <LoaderCircle v-if="state.busy.defaults" class="size-4 animate-spin" />
        <RefreshCcw v-else class="size-4" />
        {{ t("actions.reloadDeviceIdsDefaults") }}
      </button>
    </div>

    <p class="security-note">
      <ShieldAlert class="size-4" />
      {{ t("deviceIds.autofillNote") }}
    </p>

    <div class="summary-grid wide">
      <article class="summary-tile">
        <span class="summary-label">{{ t("deviceIds.deviceSummary") }}</span>
        <p class="subheading mt-2">{{ deviceTitle }}</p>
        <p class="mono-inline mt-2">{{ deviceCode }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("deviceIds.serialSummary") }}</span>
        <p class="mono-inline mt-2">{{ serialSummary }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("deviceIds.manufacturerSummary") }}</span>
        <p class="mono-inline mt-2">{{ manufacturerSummary }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("deviceIds.targetSummary") }}</span>
        <p class="mono-inline mt-2">{{ state.profile.ta_name }}</p>
        <p class="mono-inline mt-2">{{ state.profile.ta_path }}</p>
      </article>
    </div>

    <section class="panel-subsection">
      <div class="panel-heading compact">
        <div>
          <div class="section-kicker">{{ t("deviceIds.mainKicker") }}</div>
          <h3 class="subheading">{{ t("deviceIds.mainTitle") }}</h3>
        </div>
        <Radar class="icon-muted" />
      </div>

      <div class="field-grid two-up">
        <div class="field-group">
          <label class="field-label" for="device-ids-brand">{{ t("deviceIds.brand") }}</label>
          <input id="device-ids-brand" v-model="state.profile.brand" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-device">{{ t("deviceIds.device") }}</label>
          <input id="device-ids-device" v-model="state.profile.device" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-product">{{ t("deviceIds.product") }}</label>
          <input id="device-ids-product" v-model="state.profile.product" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-serial">{{ t("deviceIds.serial") }}</label>
          <input id="device-ids-serial" v-model="state.profile.serial" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-manufacturer">{{ t("deviceIds.manufacturer") }}</label>
          <input id="device-ids-manufacturer" v-model="state.profile.manufacturer" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-model">{{ t("deviceIds.model") }}</label>
          <input id="device-ids-model" v-model="state.profile.model" class="text-input">
        </div>
      </div>
    </section>

    <details class="expand-card">
      <summary class="expand-summary">
        <div>
          <span class="section-kicker">{{ t("deviceIds.advancedKicker") }}</span>
          <strong class="subheading expand-title">{{ t("deviceIds.advancedTitle") }}</strong>
        </div>
        <Wrench class="size-4 icon-muted" />
      </summary>

      <p class="body-copy mt-3">{{ t("deviceIds.advancedDescription") }}</p>

      <div class="field-grid two-up mt-4">
        <div class="field-group">
          <label class="field-label" for="device-ids-imei">{{ t("deviceIds.imei") }}</label>
          <input id="device-ids-imei" v-model="state.profile.imei" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-imei2">{{ t("deviceIds.imei2") }}</label>
          <input id="device-ids-imei2" v-model="state.profile.imei2" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-meid">{{ t("deviceIds.meid") }}</label>
          <input id="device-ids-meid" v-model="state.profile.meid" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-meid2">{{ t("deviceIds.meid2") }}</label>
          <input id="device-ids-meid2" v-model="state.profile.meid2" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-ta-name">{{ t("deviceIds.taName") }}</label>
          <input id="device-ids-ta-name" v-model="state.profile.ta_name" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="device-ids-ta-path">{{ t("deviceIds.taPath") }}</label>
          <input id="device-ids-ta-path" v-model="state.profile.ta_path" class="text-input">
        </div>
      </div>
    </details>

    <section v-if="state.result" class="panel-subsection">
      <div class="panel-heading compact">
        <div>
          <div class="section-kicker">{{ t("deviceIds.resultKicker") }}</div>
          <h3 class="subheading">{{ t("deviceIds.resultTitle") }}</h3>
        </div>
        <Radar class="icon-muted" />
      </div>

      <div class="summary-grid wide">
        <article class="summary-tile">
          <span class="summary-label">{{ t("deviceIds.resultMode") }}</span>
          <p class="mono-inline mt-2">{{ resultMode }}</p>
        </article>
        <article class="summary-tile">
          <span class="summary-label">{{ t("deviceIds.writtenCount") }}</span>
          <p class="mono-inline mt-2">{{ state.result.count }}</p>
        </article>
        <article class="summary-tile">
          <span class="summary-label">{{ t("deviceIds.taVersion") }}</span>
          <p class="mono-inline mt-2">{{ state.result.ta_api_version || t("deviceIds.notAvailable") }}</p>
          <p class="mono-inline mt-2">{{ state.result.ta_version || t("deviceIds.notAvailable") }}</p>
        </article>
        <article class="summary-tile">
          <span class="summary-label">{{ t("deviceIds.reportPath") }}</span>
          <p class="mono-inline mt-2 break-all">{{ state.result.report_path }}</p>
          <div class="about-link-row">
            <button class="action-secondary about-link" type="button" @click="actions.copyText(state.result.report_path)">
              <Copy class="size-4" />
              {{ t("actions.copyReportPath") }}
            </button>
          </div>
        </article>
      </div>

      <div class="stack-list mt-4">
        <article
          v-for="entry in state.result.ids"
          :key="`${entry.label}-${entry.value}`"
          class="list-row"
        >
          <div>
            <p class="mono-inline">{{ entry.label }}</p>
            <p class="muted mt-1 break-all">{{ entry.value }}</p>
          </div>
        </article>
      </div>
    </section>
  </section>
</template>
