<script setup lang="ts">
import { computed, ref } from 'vue'
import type { MonthlyOverview } from '../api/ledger'
import { formatMoney } from '../composables/money'

const props = defineProps<{
  items: MonthlyOverview[]
  year: string
  selected?: string
  busy?: boolean
}>()
const emit = defineEmits<{ select: [month: string] }>()
const active = ref('')

const values = computed(() =>
  props.items.map((item) => {
    const minor = BigInt(item.netExpenseMinor)
    return { item, minor, value: Number(minor) / 100 }
  }),
)
const activeItem = computed(() =>
  values.value.find((entry) => entry.item.month === active.value),
)

function niceStep(peak: number) {
  if (peak <= 0) return 100
  const rough = peak / 4
  const magnitude = Math.pow(10, Math.floor(Math.log10(rough)))
  const normalized = rough / magnitude
  const scale =
    normalized <= 1
      ? 1
      : normalized <= 2
        ? 2
        : normalized <= 2.5
          ? 2.5
          : normalized <= 5
            ? 5
            : 10
  return scale * magnitude
}

const scale = computed(() => {
  const peak = Math.max(0, ...values.value.map((entry) => entry.value))
  const trough = Math.max(0, ...values.value.map((entry) => -entry.value))
  if (peak === 0 && trough === 0)
    return { top: 100, bottom: 0, step: 25, span: 100 }
  const step = niceStep(Math.max(peak, trough))
  const top = Math.ceil(peak / step) * step || step
  const bottom = Math.ceil(trough / step) * step
  return { top, bottom, step, span: top + bottom || step }
})

function yuanLabel(value: number) {
  if (value >= 10000) {
    const wan = value / 10000
    return `${(Math.round(wan * 10) / 10).toString().replace(/\.0$/, '')}万`
  }
  return Math.round(value).toLocaleString('zh-CN')
}

const ticks = computed(() => {
  const { top, bottom, step, span } = scale.value
  const result: { key: string; label: string; top: string; zero: boolean }[] =
    []
  for (let value = -bottom; value <= top + step / 1000; value += step) {
    result.push({
      key: String(value),
      label: yuanLabel(Math.abs(value)),
      top: `${((top - value) / span) * 100}%`,
      zero: Math.abs(value) < step / 1000,
    })
  }
  return result
})

const columns = computed(() => {
  const { top, span } = scale.value
  return values.value.map((entry) => {
    const height = (Math.abs(entry.value) / span) * 100
    const zeroTop = (top / span) * 100
    return {
      ...entry,
      style:
        entry.value < 0
          ? { top: `${zeroTop}%`, height: `${height}%` }
          : { bottom: `${100 - zeroTop}%`, height: `${height}%` },
    }
  })
})

const zeroTop = computed(() => `${(scale.value.top / scale.value.span) * 100}%`)

const summary = computed(() => {
  const totals = props.items.reduce(
    (total, item) => ({
      expense: total.expense + BigInt(item.expenseMinor),
      refund: total.refund + BigInt(item.refundMinor),
      net: total.net + BigInt(item.netExpenseMinor),
    }),
    { expense: 0n, refund: 0n, net: 0n },
  )
  return `${props.year} 年净支出 ${formatMoney(totals.net.toString())} 元，消费 ${formatMoney(
    totals.expense.toString(),
  )} 元，退款 ${formatMoney(totals.refund.toString())} 元。`
})

function describe(item: MonthlyOverview) {
  return `${item.month} 净支出 ${formatMoney(item.netExpenseMinor)} 元，消费 ${formatMoney(
    item.expenseMinor,
  )} 元，退款 ${formatMoney(item.refundMinor)} 元${
    item.pendingCount ? `，待核实 ${item.pendingCount} 笔` : ''
  }`
}
</script>

<template>
  <div class="chart" :aria-busy="busy">
    <p class="chart-readout" role="status" aria-live="polite">
      <template v-if="activeItem">
        <strong
          >{{ activeItem.item.month.slice(0, 4) }} 年
          {{ Number(activeItem.item.month.slice(5, 7)) }} 月</strong
        >
        <span
          >净支出
          <strong>{{
            formatMoney(activeItem.item.netExpenseMinor)
          }}</strong></span
        >
        <span>消费 {{ formatMoney(activeItem.item.expenseMinor) }}</span>
        <span>退款 {{ formatMoney(activeItem.item.refundMinor) }}</span>
        <span v-if="activeItem.item.pendingCount"
          >待核实 {{ activeItem.item.pendingCount }} 笔</span
        >
      </template>
      <template v-else>{{ summary }}</template>
    </p>
    <div class="chart-body">
      <div class="chart-plot">
        <div class="chart-grid" aria-hidden="true">
          <div
            v-for="tick in ticks"
            :key="tick.key"
            class="chart-tick"
            :class="{ zero: tick.zero }"
            :style="{ top: tick.top }"
          >
            <i /><span>{{ tick.label }}</span>
          </div>
        </div>
        <div class="chart-columns">
          <button
            v-for="column in columns"
            :key="column.item.month"
            type="button"
            class="chart-col"
            :class="{
              selected: column.item.month === selected,
              active: column.item.month === active,
            }"
            :aria-label="describe(column.item)"
            :aria-pressed="column.item.month === selected"
            @pointerenter="active = column.item.month"
            @pointerleave="active = ''"
            @focus="active = column.item.month"
            @blur="active = ''"
            @click="emit('select', column.item.month)"
          >
            <span class="chart-track">
              <span
                class="chart-bar"
                :class="{
                  negative: column.value < 0,
                  empty: column.value === 0,
                }"
                :style="column.style"
              />
            </span>
          </button>
        </div>
        <span class="chart-zero" aria-hidden="true" :style="{ top: zeroTop }" />
      </div>
      <div class="chart-axis" aria-hidden="true">
        <span
          v-for="item in items"
          :key="item.month"
          :class="{ current: item.month === selected }"
          >{{ Number(item.month.slice(5, 7)) }}月</span
        >
      </div>
    </div>
  </div>
</template>
