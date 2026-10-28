<script setup lang="ts">
import { computed } from 'vue'
import type { CategorySpending } from '../api/ledger'
import { formatMoney } from '../composables/money'

const props = defineProps<{
  items: CategorySpending[]
  totalMinor?: string
}>()

const rows = computed(() => {
  const sorted = [...props.items].sort((left, right) => {
    const a = BigInt(left.amountMinor)
    const b = BigInt(right.amountMinor)
    if (a === b) return left.category.localeCompare(right.category, 'zh-CN')
    return a > b ? -1 : 1
  })
  const peak = sorted.reduce((max, item) => {
    const value = BigInt(item.amountMinor)
    return value > max ? value : max
  }, 0n)
  const total = BigInt(props.totalMinor ?? '0')
  return sorted.map((item) => {
    const value = BigInt(item.amountMinor)
    const width = peak > 0n ? Number((value * 1000n) / peak) / 10 : 0
    const share = total > 0n ? Number((value * 1000n) / total) / 10 : 0
    return {
      ...item,
      width: `${Math.max(2, width)}%`,
      percent: `${share.toFixed(1)}%`,
    }
  })
})
</script>

<template>
  <ul class="category-bars">
    <li v-for="item in rows" :key="item.category" class="category-row">
      <span class="category-label">
        <span>{{ item.category }}</span>
        <strong>{{ formatMoney(item.amountMinor) }}</strong>
      </span>
      <span class="category-track" aria-hidden="true">
        <i class="category-fill" :style="{ width: item.width }" />
      </span>
      <span class="category-percent">{{ item.percent }}</span>
    </li>
  </ul>
</template>
