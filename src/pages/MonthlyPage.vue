<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  getMonthDetail,
  getOverview,
  listTransactionYears,
  type MonthDetail,
  type Overview,
} from '../api/ledger'
import { formatMoney, todayShanghai } from '../composables/money'
import { ShieldCheck } from 'lucide-vue-next'

const route = useRoute()
const router = useRouter()
const today = todayShanghai()
const currentYear = today.slice(0, 4)
const currentMonth = today.slice(0, 7)
const routeMonth =
  typeof route.query.month === 'string' &&
  /^\d{4}-(0[1-9]|1[0-2])$/.test(route.query.month)
    ? route.query.month
    : currentMonth
const year = ref(routeMonth.slice(0, 4))
const month = ref(routeMonth.slice(5, 7))
const years = ref<string[]>([])
const report = ref<Overview | null>(null)
const detail = ref<MonthDetail | null>(null)
const busy = ref(false)
const error = ref('')
let requestVersion = 0

const months = Array.from({ length: 12 }, (_, index) =>
  String(index + 1).padStart(2, '0'),
)
const yearOptions = computed(() =>
  [...new Set([year.value, currentYear, ...years.value])]
    .filter(Boolean)
    .sort()
    .reverse(),
)
const selectedMonth = computed(() => `${year.value}-${month.value}`)

async function loadYears() {
  const result = await listTransactionYears()
  if (result.ok) years.value = result.data
}

async function refresh() {
  const version = ++requestVersion
  const value = selectedMonth.value
  busy.value = true
  error.value = ''
  try {
    const [overview, monthDetail] = await Promise.all([
      getOverview(value),
      getMonthDetail(value),
    ])
    if (version !== requestVersion) return
    if (overview.ok && monthDetail.ok) {
      report.value = overview.data
      detail.value = monthDetail.data
    } else {
      error.value = overview.ok
        ? monthDetail.ok
          ? ''
          : monthDetail.error.message
        : overview.error.message
    }
  } finally {
    if (version === requestVersion) busy.value = false
  }
}

function changeYear(value: string) {
  year.value = value
  month.value = value === currentYear ? currentMonth.slice(5, 7) : '12'
  syncRoute()
}

function changeMonth() {
  syncRoute()
}

function syncRoute() {
  void router.replace({
    path: '/monthly',
    query: { month: selectedMonth.value },
  })
}

watch(selectedMonth, () => {
  void refresh()
})

watch(
  () => route.query.month,
  (value) => {
    if (
      typeof value === 'string' &&
      /^\d{4}-(0[1-9]|1[0-2])$/.test(value) &&
      value !== selectedMonth.value
    ) {
      year.value = value.slice(0, 4)
      month.value = value.slice(5, 7)
    }
  },
)

onMounted(() => {
  void loadYears()
  void refresh()
})

const totals = computed(() => {
  const initial = { expense: 0n, refund: 0n, net: 0n }
  return (report.value?.monthly ?? []).reduce((total, item) => {
    total.expense += BigInt(item.expenseMinor)
    total.refund += BigInt(item.refundMinor)
    total.net += BigInt(item.netExpenseMinor)
    return total
  }, initial)
})

const maxRowNet = computed(() => {
  const days = detail.value?.days ?? []
  return days.reduce((max, day) => {
    const value = BigInt(day.netExpenseMinor)
    const positive = value < 0n ? -value : value
    return positive > max ? positive : max
  }, 0n)
})

function rowWidth(value: string) {
  const amount = BigInt(value)
  const positive = amount < 0n ? -amount : amount
  if (maxRowNet.value <= 0n) return '0%'
  return `${Math.max(1, Number((positive * 1000n) / maxRowNet.value) / 10)}%`
}

function dayNumber(value: string) {
  return value.slice(8, 10)
}

function weekday(value: string) {
  const date = new Date(`${value}T00:00:00Z`)
  return ['周日', '周一', '周二', '周三', '周四', '周五', '周六'][
    date.getUTCDay()
  ]
}

function hasDayData(day: MonthDetail['days'][number]) {
  return (
    day.incomeMinor !== '0' ||
    day.expenseMinor !== '0' ||
    day.refundMinor !== '0' ||
    day.pendingCount > 0 ||
    !!day.memo
  )
}

const activeDays = computed(() =>
  (detail.value?.days ?? []).filter((day) => hasDayData(day)),
)
</script>

<template>
  <section class="summary-strip overview-heading">
    <div>
      <span class="section-kicker">月度支出</span>
      <p>{{ selectedMonth }} 每日消费走势</p>
    </div>
    <div class="overview-controls">
      <label
        >年份<select v-model="year" @change="changeYear(year)">
          <option v-for="option in yearOptions" :key="option" :value="option">
            {{ option }} 年
          </option>
        </select></label
      >
      <label
        >月份<select v-model="month" @change="changeMonth">
          <option v-for="option in months" :key="option" :value="option">
            {{ Number(option) }} 月
          </option>
        </select></label
      >
      <span class="muted small-text" role="status" aria-live="polite">{{
        busy ? '正在读取月明细…' : '更换月份后自动更新'
      }}</span>
    </div>
  </section>

  <p v-if="error" class="form-error" role="alert">{{ error }}</p>

  <section class="metric-grid" aria-label="当月支出概况" :aria-busy="busy">
    <article class="metric-card">
      <div class="metric-top"><span>消费支出</span></div>
      <strong class="metric-value">{{
        detail ? formatMoney(detail.totals.expenseMinor) : '—'
      }}</strong>
      <p>退款前消费金额</p>
    </article>
    <article class="metric-card">
      <div class="metric-top"><span>退款</span></div>
      <strong class="metric-value">{{
        detail ? formatMoney(detail.totals.refundMinor) : '—'
      }}</strong>
      <p>按退款发生日冲减</p>
    </article>
    <article class="metric-card">
      <div class="metric-top"><span>净支出</span></div>
      <strong class="metric-value">{{
        detail ? formatMoney(detail.totals.netExpenseMinor) : '—'
      }}</strong>
      <p>消费扣除退款</p>
    </article>
    <article class="metric-card">
      <div class="metric-top"><span>待核实</span></div>
      <strong class="metric-value">{{
        detail ? `${detail.totals.pendingCount}` : '—'
      }}</strong>
      <p>不计入收支的笔数</p>
    </article>
  </section>

  <section class="panel chart-panel">
    <div class="panel-heading">
      <div>
        <h2>每日净支出</h2>
        <p class="muted small-text">{{ selectedMonth }} 有记录的日子</p>
      </div>
      <span v-if="detail" class="subtle-tag"
        >有记录 {{ activeDays.length }} 天</span
      >
    </div>
    <div v-if="activeDays.length" class="day-chart" role="list">
      <div
        v-for="day in activeDays"
        :key="day.date"
        class="day-row"
        role="listitem"
      >
        <span class="day-label">
          <strong>{{ dayNumber(day.date) }} 日</strong>
          <small>{{ weekday(day.date) }}</small>
        </span>
        <span class="day-track" aria-hidden="true">
          <i :style="{ width: rowWidth(day.netExpenseMinor) }" />
        </span>
        <span class="day-amount">{{ formatMoney(day.netExpenseMinor) }}</span>
        <span class="day-memo" :title="day.memo">
          {{ day.memo || '—' }}
          <small v-if="day.pendingCount"
            >待核实 {{ day.pendingCount }} 笔</small
          >
        </span>
      </div>
    </div>
    <div v-else class="chart-empty">
      <p>{{ busy ? '正在读取月明细…' : '当月暂无已确认交易。' }}</p>
    </div>
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>每日明细</h2>
        <p class="muted small-text">按日列出收入、消费、退款与分类</p>
      </div>
      <router-link
        :to="{ path: '/transactions', query: { month: selectedMonth } }"
        class="text-link compact-link"
        >查看原始交易</router-link
      >
    </div>
    <div class="table-scroll" :aria-busy="busy">
      <table v-if="detail" class="summary-table" aria-label="月度每日明细表">
        <thead>
          <tr>
            <th>日期</th>
            <th class="amount-cell">收入</th>
            <th class="amount-cell">消费</th>
            <th class="amount-cell">退款</th>
            <th class="amount-cell">净支出</th>
            <th>分类</th>
            <th>备注</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="day in activeDays" :key="day.date">
            <th scope="row">
              <strong>{{ day.date.slice(5) }}</strong>
              <span class="row-sub">{{ weekday(day.date) }}</span>
            </th>
            <td class="amount-cell">
              {{ day.incomeMinor === '0' ? '—' : formatMoney(day.incomeMinor) }}
            </td>
            <td class="amount-cell">
              {{
                day.expenseMinor === '0' ? '—' : formatMoney(day.expenseMinor)
              }}
            </td>
            <td class="amount-cell">
              {{ day.refundMinor === '0' ? '—' : formatMoney(day.refundMinor) }}
            </td>
            <td class="amount-cell">
              {{ formatMoney(day.netExpenseMinor) }}
            </td>
            <td class="category-cell">
              <span
                v-for="[name, amount] in Object.entries(day.categories)"
                :key="name"
                class="subtle-tag"
                >{{ name }} {{ formatMoney(amount) }}</span
              >
              <span v-if="!Object.keys(day.categories).length" class="muted"
                >—</span
              >
            </td>
            <td class="detail-memo" :title="day.memo">
              {{ day.memo || '—' }}
              <span v-if="day.pendingCount" class="pending-note"
                >待核实 {{ day.pendingCount }} 笔</span
              >
            </td>
          </tr>
        </tbody>
        <tfoot>
          <tr>
            <th scope="row">月合计</th>
            <td class="amount-cell">
              {{ formatMoney(detail.totals.incomeMinor) }}
            </td>
            <td class="amount-cell">
              {{ formatMoney(detail.totals.expenseMinor) }}
            </td>
            <td class="amount-cell">
              {{ formatMoney(detail.totals.refundMinor) }}
            </td>
            <td class="amount-cell">
              {{ formatMoney(detail.totals.netExpenseMinor) }}
            </td>
            <td colspan="2">
              {{ detail.totals.pendingCount ? '含待核实交易' : '已全部核实' }}
            </td>
          </tr>
        </tfoot>
      </table>
      <div v-else class="table-empty">
        {{ busy ? '正在读取月明细…' : '暂无月明细。' }}
      </div>
    </div>
  </section>

  <p v-if="report" class="report-note" role="status">
    {{ year }} 年消费 {{ formatMoney(totals.expense.toString()) }} 元，退款
    {{ formatMoney(totals.refund.toString()) }} 元，净支出
    {{ formatMoney(totals.net.toString()) }} 元。本月已确认
    {{ report.confirmedCount }} 笔，待核实 {{ report.pendingCount }} 笔。
  </p>

  <section class="panel principles">
    <div>
      <ShieldCheck :size="22" aria-hidden="true" />
      <div>
        <h3>统计口径</h3>
        <p>
          只汇总已确认交易；退款按发生日冲减，转账及借贷本金不计收支；待核实交易不进入上述合计。
        </p>
      </div>
    </div>
  </section>
</template>
