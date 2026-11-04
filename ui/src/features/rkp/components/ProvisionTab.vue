<script setup lang="ts">
import { useMutation } from "@tanstack/vue-query"
import { CloudUpload, TriangleAlert } from "@lucide/vue"
import { storeToRefs } from "pinia"
import { computed } from "vue"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import KeyValueList from "@/core/components/KeyValueList.vue"
import SectionCard from "@/core/components/SectionCard.vue"
import { notifySuccess } from "@/core/notify"

import { useRkpI18n } from "../i18n"
import { useRkpStore } from "../store"

const { t } = useRkpI18n()
const store = useRkpStore()
const { provision } = storeToRefs(store)
const run = useMutation({
  mutationFn: store.runProvision,
  onSuccess: () => notifySuccess(t("provision.done")),
})

const rows = computed(() => {
  const data = provision.value
  if (!data) return []
  return [
    {
      label: t("provision.localVerify"),
      value: data.local_verify.signature_valid ? t("provision.valid") : t("provision.invalid"),
    },
    { label: t("provision.challenge"), value: data.challenge_hex, mono: true, copy: true },
    {
      label: t("verify.unverifiedFingerprint"),
      value: data.local_verify.unverified_fingerprint,
      mono: true,
    },
    { label: t("provision.csr"), value: data.csr_path, mono: true, copy: true },
    {
      label: t("provision.protectedData"),
      value: t("provision.bytes", { count: data.protected_data_len }),
    },
  ]
})
</script>

<template>
  <div class="flex flex-col gap-6">
    <p v-if="!provision" class="text-muted-foreground px-4 text-sm">{{ t("provision.empty") }}</p>
    <Button size="lg" :disabled="store.profileBusy || !store.draft" @click="run.mutate()">
      <Spinner v-if="run.isPending.value" class="size-6" />
      <CloudUpload v-else />
      {{ t("provision.run") }}
    </Button>

    <template v-if="provision">
      <Alert v-if="provision.local_test_mode || provision.server_submission_error">
        <TriangleAlert />
        <AlertTitle>{{
          provision.local_test_mode ? t("provision.localTestMode") : t("provision.submitError")
        }}</AlertTitle>
        <AlertDescription class="font-mono text-sm break-all">
          {{ provision.fetch_eek_error || provision.server_submission_error }}
        </AlertDescription>
      </Alert>

      <SectionCard :title="t('provision.csr')">
        <KeyValueList :rows="rows" />
      </SectionCard>

      <SectionCard :title="t('provision.chains')">
        <p v-if="!provision.cert_chains.length" class="text-muted-foreground text-sm">
          {{ t("provision.noChains") }}
        </p>
        <div v-else class="flex flex-col gap-3">
          <div
            v-for="chain in provision.cert_chains"
            :key="chain.path"
            class="bg-muted rounded-md p-4"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="font-mono text-sm">cert_chain_{{ chain.index }}</span>
              <Badge variant="secondary">
                {{ t("provision.certificates", { count: chain.summary.certificates }) }}
              </Badge>
            </div>
            <ul class="text-muted-foreground mt-2 flex flex-col gap-1 font-mono text-sm break-all">
              <li v-for="subject in chain.summary.subjects" :key="subject">{{ subject }}</li>
            </ul>
          </div>
        </div>
      </SectionCard>
    </template>
  </div>
</template>
