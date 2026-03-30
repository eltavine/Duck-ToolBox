<script setup lang="ts">
import { computed, ref } from "vue"
import {
  ChevronDown,
  LoaderCircle,
  RefreshCcw,
  Save,
  ShieldAlert,
  Smartphone,
  Trash2,
  Wrench,
} from "lucide-vue-next"

import ChoiceDialog from "@/features/shared/ChoiceDialog.vue"
import { useI18n } from "@/i18n"
import type { UiMode } from "../types"
import type { RkpWorkbenchActions, RkpWorkbenchState } from "../types"
import {
  bootloaderStateOptions,
  choiceDescription,
  choiceLabel,
  keySourceOptions,
  numKeysOptions,
  securityLevelOptions,
  verifiedBootOptions,
} from "../profileChoices"
import { DEFAULT_KDF_LABEL } from "../profileState"

const props = defineProps<{
  state: RkpWorkbenchState
  actions: RkpWorkbenchActions
  secretPath: string
}>()

type DialogId =
  | "mode"
  | "security"
  | "verified-boot"
  | "bootloader"
  | "num-keys"
  | null

interface DialogOption {
  value: string | number
  label: string
  description: string
}

const deviceFields = [
  ["brand", "profile.brand"],
  ["model", "profile.model"],
  ["device", "profile.device"],
  ["product", "profile.product"],
  ["manufacturer", "profile.manufacturer"],
  ["os_version", "profile.osVersion"],
  ["fused", "profile.fused"],
  ["vb_state", "profile.vbState"],
  ["security_level", "profile.securityLevel"],
  ["bootloader_state", "profile.bootloaderState"],
  ["boot_patch_level", "profile.bootPatchLevel"],
  ["system_patch_level", "profile.systemPatchLevel"],
  ["vendor_patch_level", "profile.vendorPatchLevel"],
  ["vbmeta_digest", "profile.vbmetaDigest"],
  ["dice_issuer", "profile.diceIssuer"],
  ["dice_subject", "profile.diceSubject"],
] as const

const numericDeviceKeys = new Set([
  "fused",
  "boot_patch_level",
  "system_patch_level",
  "vendor_patch_level",
])
const dialogId = ref<DialogId>(null)
const { t } = useI18n()

const choiceOptions = computed<DialogOption[]>(() => {
  const options =
    dialogId.value === "mode"
      ? keySourceOptions
      : dialogId.value === "security"
        ? securityLevelOptions
        : dialogId.value === "verified-boot"
          ? verifiedBootOptions
          : dialogId.value === "bootloader"
            ? bootloaderStateOptions
            : dialogId.value === "num-keys"
              ? numKeysOptions
              : []

  return options.map((option) => ({
    value: option.value,
    label: t(option.labelKey),
    description: t(option.descriptionKey),
  }))
})

const dialogTitle = computed(() => {
  switch (dialogId.value) {
    case "mode":
      return t("profile.dialogKeySourceTitle")
    case "security":
      return t("profile.dialogSecurityTitle")
    case "verified-boot":
      return t("profile.dialogBootStateTitle")
    case "bootloader":
      return t("profile.dialogBootloaderTitle")
    case "num-keys":
      return t("profile.dialogNumKeysTitle")
    default:
      return ""
  }
})

const dialogDescription = computed(() => {
  switch (dialogId.value) {
    case "mode":
      return t("profile.dialogKeySourceDescription")
    case "security":
      return t("profile.dialogSecurityDescription")
    case "verified-boot":
      return t("profile.dialogBootStateDescription")
    case "bootloader":
      return t("profile.dialogBootloaderDescription")
    case "num-keys":
      return t("profile.dialogNumKeysDescription")
    default:
      return ""
  }
})

const dialogSelected = computed<string | number | null>(() => {
  switch (dialogId.value) {
    case "mode":
      return props.state.profile.mode
    case "security":
      return props.state.profile.device.security_level || "tee"
    case "verified-boot":
      return props.state.profile.device.vb_state || "green"
    case "bootloader":
      return props.state.profile.device.bootloader_state || "locked"
    case "num-keys":
      return props.state.profile.num_keys
    default:
      return null
  }
})

const keySourceLabel = computed(() =>
  choiceLabel(keySourceOptions, props.state.profile.mode, t),
)
const keySourceDescription = computed(() =>
  choiceDescription(keySourceOptions, props.state.profile.mode, t),
)
const securityLevelLabel = computed(() =>
  choiceLabel(
    securityLevelOptions,
    props.state.profile.device.security_level || "tee",
    t,
  ),
)
const securityLevelDescription = computed(() =>
  choiceDescription(
    securityLevelOptions,
    props.state.profile.device.security_level || "tee",
    t,
  ),
)
const verifiedBootLabel = computed(() =>
  choiceLabel(
    verifiedBootOptions,
    props.state.profile.device.vb_state || "green",
    t,
  ),
)
const verifiedBootDescription = computed(() =>
  choiceDescription(
    verifiedBootOptions,
    props.state.profile.device.vb_state || "green",
    t,
  ),
)
const bootloaderLabel = computed(() =>
  choiceLabel(
    bootloaderStateOptions,
    props.state.profile.device.bootloader_state || "locked",
    t,
  ),
)
const bootloaderDescription = computed(() =>
  choiceDescription(
    bootloaderStateOptions,
    props.state.profile.device.bootloader_state || "locked",
    t,
  ),
)
const numKeysLabel = computed(() =>
  choiceLabel(numKeysOptions, props.state.profile.num_keys, t),
)
const numKeysDescription = computed(() =>
  choiceDescription(numKeysOptions, props.state.profile.num_keys, t),
)
const deviceTitle = computed(() => {
  const values = [
    props.state.profile.device.brand,
    props.state.profile.device.model,
  ].filter((value) => value.trim())
  return values.join(" ") || t("profile.notDetected")
})
const deviceCode = computed(() => {
  const values = [
    props.state.profile.device.device,
    props.state.profile.device.product,
  ].filter((value) => value.trim())
  return values.join(" / ") || t("profile.notDetected")
})
const platformSummary = computed(() =>
  props.state.profile.device.os_version || t("profile.notDetected"),
)
const patchSummary = computed(() =>
  t("profile.patchSummary", {
    boot: formatNumeric(props.state.profile.device.boot_patch_level),
    system: formatNumeric(props.state.profile.device.system_patch_level),
    vendor: formatNumeric(props.state.profile.device.vendor_patch_level),
  }),
)

function formatNumeric(value: number) {
  return value > 0 ? String(value) : t("profile.notDetected")
}

function displayValue(value: string) {
  return value.trim() || t("profile.notDetected")
}

function openDialog(id: Exclude<DialogId, null>) {
  dialogId.value = id
}

function closeDialog() {
  dialogId.value = null
}

function updateMode(mode: UiMode) {
  props.state.profile.mode = mode
  if (mode === "hw-key" && !props.state.profile.kdf_label.trim()) {
    props.state.profile.kdf_label = DEFAULT_KDF_LABEL
  }
}

function applyDialogSelection(value: string | number) {
  switch (dialogId.value) {
    case "mode":
      updateMode(value as UiMode)
      break
    case "security":
      props.state.profile.device.security_level = String(value)
      break
    case "verified-boot":
      props.state.profile.device.vb_state = String(value)
      break
    case "bootloader":
      props.state.profile.device.bootloader_state = String(value)
      break
    case "num-keys":
      props.state.profile.num_keys = Number(value)
      break
  }

  closeDialog()
}

function updateDeviceField(key: string, value: string) {
  const record = props.state.profile.device as unknown as Record<
    string,
    string | number
  >
  record[key] = numericDeviceKeys.has(key) ? Number(value || 0) : value
}
</script>

<template>
  <section class="panel-subsection">
    <div class="toolbar">
      <button class="action-primary" :disabled="state.busy.save || state.bridge.mode === 'unavailable'" @click="actions.saveProfile()">
        <LoaderCircle v-if="state.busy.save" class="size-4 animate-spin" />
        <Save v-else class="size-4" />
        {{ t("actions.saveProfile") }}
      </button>
      <button class="action-secondary" :disabled="state.busy.device || state.bridge.mode === 'unavailable'" @click="actions.syncDeviceProfile()">
        <LoaderCircle v-if="state.busy.device" class="size-4 animate-spin" />
        <Smartphone v-else class="size-4" />
        {{ t("actions.readDevice") }}
      </button>
      <button class="action-secondary" :disabled="state.busy.load || state.bridge.mode === 'unavailable'" @click="actions.loadProfile()">
        <LoaderCircle v-if="state.busy.load" class="size-4 animate-spin" />
        <RefreshCcw v-else class="size-4" />
        {{ t("actions.reload") }}
      </button>
      <button class="action-danger" :disabled="state.busy.clear || state.bridge.mode === 'unavailable'" @click="actions.clearProfile()">
        <LoaderCircle v-if="state.busy.clear" class="size-4 animate-spin" />
        <Trash2 v-else class="size-4" />
        {{ t("actions.clearSensitive") }}
      </button>
    </div>

    <p class="security-note">
      <ShieldAlert class="size-4" />
      {{ t("profile.securityNote", { path: secretPath }) }}
    </p>

    <p class="body-copy profile-note">{{ t("profile.autofillNote") }}</p>

    <div class="summary-grid wide profile-summary-grid">
      <article class="summary-tile">
        <span class="summary-label">{{ t("profile.deviceSummary") }}</span>
        <p class="subheading mt-2">{{ deviceTitle }}</p>
        <p class="mono-inline mt-2">{{ deviceCode }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("profile.fingerprintSummary") }}</span>
        <p class="mono-inline mt-2">{{ displayValue(state.profile.fingerprint) }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("profile.platformSummary") }}</span>
        <p class="subheading mt-2">{{ platformSummary }}</p>
        <p class="mono-inline mt-2">{{ patchSummary }}</p>
      </article>
      <article class="summary-tile">
        <span class="summary-label">{{ t("profile.runtimeSummary") }}</span>
        <p class="mono-inline mt-2">{{ displayValue(state.profile.server_url) }}</p>
        <p class="mono-inline mt-2">{{ displayValue(state.profile.output_path) }}</p>
      </article>
    </div>

    <div class="profile-choice-grid">
      <button class="choice-card" type="button" :disabled="state.bridge.mode === 'unavailable'" @click="openDialog('mode')">
        <span class="field-label">{{ t("profile.keySource") }}</span>
        <strong class="choice-value">{{ keySourceLabel }}</strong>
        <p class="body-copy">{{ keySourceDescription }}</p>
        <span class="choice-action">
          {{ t("profile.changeChoice") }}
          <ChevronDown class="size-4" />
        </span>
      </button>

      <button class="choice-card" type="button" :disabled="state.bridge.mode === 'unavailable'" @click="openDialog('security')">
        <span class="field-label">{{ t("profile.securityLevel") }}</span>
        <strong class="choice-value">{{ securityLevelLabel }}</strong>
        <p class="body-copy">{{ securityLevelDescription }}</p>
        <span class="choice-action">
          {{ t("profile.changeChoice") }}
          <ChevronDown class="size-4" />
        </span>
      </button>

      <button class="choice-card" type="button" :disabled="state.bridge.mode === 'unavailable'" @click="openDialog('verified-boot')">
        <span class="field-label">{{ t("profile.vbState") }}</span>
        <strong class="choice-value">{{ verifiedBootLabel }}</strong>
        <p class="body-copy">{{ verifiedBootDescription }}</p>
        <span class="choice-action">
          {{ t("profile.changeChoice") }}
          <ChevronDown class="size-4" />
        </span>
      </button>

      <button class="choice-card" type="button" :disabled="state.bridge.mode === 'unavailable'" @click="openDialog('bootloader')">
        <span class="field-label">{{ t("profile.bootloaderState") }}</span>
        <strong class="choice-value">{{ bootloaderLabel }}</strong>
        <p class="body-copy">{{ bootloaderDescription }}</p>
        <span class="choice-action">
          {{ t("profile.changeChoice") }}
          <ChevronDown class="size-4" />
        </span>
      </button>

      <button class="choice-card" type="button" :disabled="state.bridge.mode === 'unavailable'" @click="openDialog('num-keys')">
        <span class="field-label">{{ t("profile.numKeys") }}</span>
        <strong class="choice-value">{{ numKeysLabel }}</strong>
        <p class="body-copy">{{ numKeysDescription }}</p>
        <span class="choice-action">
          {{ t("profile.changeChoice") }}
          <ChevronDown class="size-4" />
        </span>
      </button>
    </div>

    <div v-if="state.profile.mode === 'seed'" class="field-group mt-4">
      <label class="field-label" for="seed">{{ t("profile.seed") }}</label>
      <textarea
        id="seed"
        v-model="state.profile.seed_hex"
        class="text-input min-h-24"
        :placeholder="t('profile.seedPlaceholder')"
      />
    </div>

    <div v-else class="field-grid two-up mt-4">
      <div class="field-group">
        <label class="field-label" for="hw-key">{{ t("profile.hwKey") }}</label>
        <input id="hw-key" v-model="state.profile.hw_key_hex" class="text-input" :placeholder="t('profile.hwKeyPlaceholder')">
      </div>
      <div class="field-group">
        <label class="field-label" for="kdf-label">{{ t("profile.kdfLabel") }}</label>
        <input id="kdf-label" v-model="state.profile.kdf_label" class="text-input" :placeholder="t('profile.kdfLabelPlaceholder')">
      </div>
    </div>

    <details class="expand-card">
      <summary class="expand-summary">
        <div>
          <span class="section-kicker">{{ t("profile.advancedKicker") }}</span>
          <strong class="subheading expand-title">{{ t("profile.advancedTitle") }}</strong>
        </div>
        <Wrench class="size-4 icon-muted" />
      </summary>

      <p class="body-copy mt-3">{{ t("profile.advancedDescription") }}</p>

      <div class="field-grid two-up mt-4">
        <div class="field-group">
          <label class="field-label" for="fingerprint">{{ t("profile.fingerprint") }}</label>
          <input id="fingerprint" v-model="state.profile.fingerprint" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="server-url">{{ t("profile.serverUrl") }}</label>
          <input id="server-url" v-model="state.profile.server_url" class="text-input">
        </div>
        <div class="field-group">
          <label class="field-label" for="num-keys">{{ t("profile.numKeys") }}</label>
          <input id="num-keys" v-model.number="state.profile.num_keys" class="text-input" min="1" type="number">
        </div>
        <div class="field-group">
          <label class="field-label" for="output-path">{{ t("profile.outputPath") }}</label>
          <input id="output-path" v-model="state.profile.output_path" class="text-input">
        </div>
      </div>

      <div class="field-grid device-grid mt-4">
        <div v-for="[key, labelKey] in deviceFields" :key="key" class="field-group">
          <label class="field-label" :for="key">{{ t(labelKey) }}</label>
          <input
            :id="key"
            class="text-input"
            :type="numericDeviceKeys.has(key) ? 'number' : 'text'"
            :value="state.profile.device[key]"
            @input="updateDeviceField(key, ($event.target as HTMLInputElement).value)"
          >
        </div>
      </div>
    </details>

    <ChoiceDialog
      :description="dialogDescription"
      :open="dialogId !== null"
      :options="choiceOptions"
      :selected="dialogSelected"
      :title="dialogTitle"
      @close="closeDialog"
      @select="applyDialogSelection"
    />
  </section>
</template>
