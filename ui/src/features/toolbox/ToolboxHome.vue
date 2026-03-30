<script setup lang="ts">
import { computed } from "vue"
import { LayoutPanelTop, PackageOpen, ShieldCheck } from "lucide-vue-next"

import { useI18n } from "@/i18n"
import type { BridgeStatus } from "@/lib/types"

const props = defineProps<{
  bridge: BridgeStatus
  runtimeStatusLabel: string
  moduleRoot: string
  outputDirectory: string
}>()

const { t } = useI18n()
const bridgeAvailable = computed(() => props.bridge.mode === "kernelsu")

const cards = [
  { label: "shell.runtime", icon: ShieldCheck, value: "runtimeMode" },
  { label: "status.moduleRoot", icon: PackageOpen, value: "moduleRoot" },
  { label: "shell.outputs", icon: LayoutPanelTop, value: "outputDirectory" },
] as const
</script>

<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <div class="section-kicker">{{ t("shell.overviewKicker") }}</div>
        <h2 class="panel-title">{{ t("shell.overviewTitle") }}</h2>
      </div>
    </div>

    <p class="body-copy max-w-3xl">{{ t("shell.overviewBody") }}</p>
    <p class="muted mt-3">{{ t("shell.overviewHint") }}</p>

    <div class="summary-grid wide">
      <article v-for="card in cards" :key="card.label" class="summary-tile">
        <div class="panel-heading compact">
          <span class="summary-label">{{ t(card.label) }}</span>
          <component
            :is="card.icon"
            :class="[
              'icon-muted',
              card.value === 'runtimeMode'
                ? bridgeAvailable
                  ? 'status-icon-online'
                  : 'status-icon-offline'
                : '',
            ]"
          />
        </div>
        <p
          :class="[
            'mono-inline mt-2 break-all',
            card.value === 'runtimeMode'
              ? bridgeAvailable
                ? 'status-text-online'
                : 'status-text-offline'
              : '',
          ]"
        >
          {{
            card.value === "runtimeMode"
              ? runtimeStatusLabel
              : card.value === "moduleRoot"
                ? moduleRoot
                : outputDirectory
          }}
        </p>
      </article>
    </div>

    <p v-if="!bridgeAvailable" class="error-banner mt-4">
      {{ t("messages.ksuUnavailable") }}
    </p>
  </section>
</template>
