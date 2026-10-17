<script setup lang="ts">
import { useQuery } from "@tanstack/vue-query"

import { Badge } from "@/components/ui/badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import QueryState from "@/core/components/QueryState.vue"

import { attestationApi } from "../api"
import KeyboxPanel from "../components/KeyboxPanel.vue"
import SeedPanel from "../components/SeedPanel.vue"
import { useAttestationI18n } from "../i18n"

const { t } = useAttestationI18n()
const status = useQuery({ queryKey: ["tee-attestation.status"], queryFn: attestationApi.status })
</script>

<template>
  <div class="flex flex-col gap-5 p-4">
    <Badge class="self-start" variant="secondary">{{ t("scope.title") }}</Badge>
    <p class="text-muted-foreground text-sm">{{ t("scope.description") }}</p>
    <QueryState
      :loading="status.isPending.value"
      :error="status.error.value"
      @retry="status.refetch()"
    >
      <Tabs v-if="status.data.value" default-value="keybox">
        <TabsList class="mb-4 w-full"
          ><TabsTrigger value="keybox" class="flex-1">Keybox</TabsTrigger
          ><TabsTrigger value="seed" class="flex-1">{{ t("seed.title") }}</TabsTrigger></TabsList
        >
        <TabsContent value="keybox"><KeyboxPanel /></TabsContent>
        <TabsContent value="seed"><SeedPanel /></TabsContent>
      </Tabs>
    </QueryState>
  </div>
</template>
