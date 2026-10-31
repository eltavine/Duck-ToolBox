<script setup lang="ts">
import { ref } from "vue"
import { Button } from "@/components/ui/button"
import { openExternal } from "@/core/links"
import { useIntegrityI18n } from "../i18n"
import licenseText from "../license.txt?raw"
const { t } = useIntegrityI18n()
const show = ref(false)
// CI embeds the immutable Git commit used to build the engine and the WebUI.
const sourceCommit = import.meta.env.VITE_PIF_SOURCE_COMMIT || "main"
const sourceUrl = `https://github.com/eltavine/Duck-ToolBox/tree/${sourceCommit}/components/play-integrity`
</script>
<template>
  <footer class="text-muted-foreground border-t px-4 py-5 text-center text-xs leading-6">
    <p>{{ t("attribution") }}</p>
    <div class="flex flex-wrap justify-center gap-1">
      <Button
        variant="link"
        size="sm"
        @click="
          openExternal(
            'https://github.com/osm0sis/PlayIntegrityFork/tree/23ec56c2440c4fd1a5e893a23ef7861d6366a828',
          )
        "
        >{{ t("upstream") }}</Button
      >
      <Button variant="link" size="sm" @click="show = !show">GPLv3</Button>
      <a
        class="inline-flex min-h-12 items-center px-3 underline"
        :href="sourceUrl"
        @click.prevent="openExternal(sourceUrl)"
        >{{ t("source") }}</a
      >
    </div>
    <p>{{ t("licenseScope") }}</p>
    <pre
      v-if="show"
      class="bg-muted mt-3 max-h-96 overflow-auto rounded-xl p-4 text-start whitespace-pre-wrap"
      >{{ licenseText }}</pre>
  </footer>
</template>
