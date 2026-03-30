<script setup lang="ts">
import { Copy, HardDriveDownload, LoaderCircle } from "lucide-vue-next"

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
      <button class="action-primary" :disabled="state.busy.provision || state.bridge.mode === 'unavailable'" @click="actions.runProvision()">
        <LoaderCircle v-if="state.busy.provision" class="size-4 animate-spin" />
        <HardDriveDownload v-else class="size-4" />
        {{ t("actions.runProvision") }}
      </button>
    </div>

    <div v-if="state.provisionResult" class="space-y-4">
      <div class="kv-grid">
        <div class="kv-item">
          <span class="summary-label">{{ t("provision.challenge") }}</span>
          <p class="mono-inline break-all">{{ state.provisionResult.challenge_hex }}</p>
        </div>
        <div class="kv-item">
          <span class="summary-label">{{ t("provision.csr") }}</span>
          <div class="copy-row">
            <p class="mono-inline break-all">{{ state.provisionResult.csr_path }}</p>
            <button class="icon-button" @click="actions.copyText(state.provisionResult.csr_path)">
              <Copy class="size-4" />
            </button>
          </div>
        </div>
        <div class="kv-item">
          <span class="summary-label">{{ t("status.localVerify") }}</span>
          <strong>
            {{
              state.provisionResult.local_verify.signature_valid
                ? t("status.valid")
                : t("status.invalid")
            }}
          </strong>
        </div>
        <div class="kv-item">
          <span class="summary-label">{{ t("status.protectedData") }}</span>
          <strong>{{ state.provisionResult.protected_data_len }} bytes</strong>
        </div>
      </div>

      <div class="space-y-3">
        <h3 class="subheading">{{ t("status.certificateChains") }}</h3>
        <article
          v-for="chain in state.provisionResult.cert_chains"
          :key="chain.path"
          class="list-row"
        >
          <div>
            <p class="mono-inline">cert_chain_{{ chain.index }}</p>
            <p class="muted mt-1">{{ chain.summary.certificates }} certificates</p>
            <p
              v-for="subject in chain.summary.subjects"
              :key="subject"
              class="muted mt-1 text-xs"
            >
              {{ subject }}
            </p>
          </div>
          <button class="icon-button" @click="actions.copyText(chain.path)">
            <Copy class="size-4" />
          </button>
        </article>
      </div>
    </div>
    <p v-else class="empty-copy">
      {{ t("provision.empty") }}
    </p>
  </section>
</template>
