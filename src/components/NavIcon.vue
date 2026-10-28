<script setup lang="ts">
import { computed } from 'vue'
import {
  LayoutDashboard,
  ReceiptText,
  Upload,
  ScanLine,
  Tags,
  WalletCards,
  ShieldCheck,
  Settings,
} from 'lucide-vue-next'
const props = defineProps<{ name: string }>()
const icons = {
  overview: LayoutDashboard,
  transactions: ReceiptText,
  imports: Upload,
  review: ScanLine,
  rules: Tags,
  accounts: WalletCards,
  quality: ShieldCheck,
  settings: Settings,
}
const icon = computed(
  () => icons[props.name as keyof typeof icons] ?? ReceiptText,
)
</script>
<template>
  <component :is="icon" :size="19" :stroke-width="1.7" aria-hidden="true" />
</template>
