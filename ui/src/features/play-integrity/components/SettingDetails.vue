<script setup lang="ts">
import { ChevronDown } from "@lucide/vue"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
defineProps<{ title: string }>()
</script>
<template>
  <Collapsible class="min-w-0 rounded-xl border border-border/70 bg-muted/20 p-2">
    <CollapsibleTrigger as-child>
      <Button
        variant="ghost"
        class="group min-h-12 w-full justify-between px-3 text-start whitespace-normal"
      >
        <span>{{ title }}</span
        ><ChevronDown
          class="shrink-0 transition-transform group-data-[state=open]:rotate-180 motion-reduce:transition-none"
        />
      </Button>
    </CollapsibleTrigger>
    <CollapsibleContent class="px-2 pt-3 pb-2"><slot /></CollapsibleContent>
  </Collapsible>
</template>
