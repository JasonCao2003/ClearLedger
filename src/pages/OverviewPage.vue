<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { NButton } from 'naive-ui'
import { getOverview, listTransactionYears, type Overview } from '../api/ledger'
import { formatMoney, todayShanghai } from '../composables/money'
import { ArrowUpRight, RotateCcw, Scale, ShieldCheck } from 'lucide-vue-next'
import MonthlyExpenseChart from '../components/MonthlyExpenseChart.vue'
import CategorySpendingChart from '../components/CategorySpendingChart.vue'

const router = useRouter()
const today = todayShanghai()
const currentYear = today.slice(0, 4)
const currentMonth = today.slice(5, 7)
const year = ref(currentYear)
const month = ref(currentMonth)
const years = ref<string[]>([])
const report = ref<Overview | null>(null)
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
  busy.value = true
  error.value = ''
  try {
    const result = await getOverview(`${year.value}-${month.value}`)
    if (version !== requestVersion) return
    if (result.ok) report.value = result.data
    else error.value = result.error.message
  } finally {
    if (version === requestVersion) busy.value = false
  }
}

function changeYear(value: string) {
  month.value = value === currentYear ? currentMonth : '12'
}

watch([year, month], () => {
  void refresh()
})

onMounted(() => {
  void loadYears()
  void refresh()
})

const annual = computed(() => {
  const initial = {
    income: 0n,
    expense: 0n,
    refund: 0n,
    netExpense: 0n,
    balance: 0n,
    confirmed: 0,
    pending: 0,
  }
  return (report.value?.monthly ?? []).reduce((total, item) => {
    total.income += BigInt(item.incomeMinor)
    total.expense += BigInt(item.expenseMinor)
    total.refund += BigInt(item.refundMinor)
    total.netExpense += BigInt(item.netExpenseMinor)
    total.balance += BigInt(item.balanceMinor)
    total.confirmed += item.confirmedCount
    total.pending += item.pendingCount
    return total
  }, initial)
})

const metrics = computed(() => [
  {
    name: '总支出',
    value: annual.value.netExpense.toString(),
    caption: '消费扣除退款后的净支出',
    tone: 'blue',
    icon: ArrowUpRight,
  },
  {
    name: '消费支出',
    value: annual.value.expense.toString(),
    caption: '退款前消费金额',
    tone: 'orange',
    icon: RotateCcw,
  },
  {
    name: '退款',
    value: annual.value.refund.toString(),
    caption: '按退款发生日冲减',
    tone: 'green',
    icon: RotateCcw,
  },
  {
    name: '期间结余',
    value: annual.value.balance.toString(),
    caption: '收入减去净支出',
    tone: 'purple',
    icon: Scale,
  },
])

const hasCategoryData = computed(
  () => (report.value?.categorySpending.length ?? 0) > 0,
)

function openMonth(value: string) {
  void router.push({ path: '/monthly', query: { month: value } })
}
</script>

<template>
  <section class="summary-strip overview-heading">
    <div>
      <span class="section-kicker">年度账本</span>
      <p>{{ year || '—' }} 年支出总览</p>
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
        >分类月份<select v-model="month">
          <option v-for="option in months" :key="option" :value="option">
            {{ Number(option) }} 月
          </option>
        </select></label
      >
      <span class="muted small-text" role="status" aria-live="polite">{{
        busy ? '正在汇总…' : '更换条件后自动更新'
      }}</span>
    </div>
  </section>

  <p v-if="error" class="form-error" role="alert">{{ error }}</p>

  <section class="metric-grid" aria-label="年度支出概况" :aria-busy="busy">
    <article v-for="metric in metrics" :key="metric.name" class="metric-card">
      <div class="metric-top">
        <span>{{ metric.name }}</span>
        <span class="metric-icon" :class="metric.tone">
          <component :is="metric.icon" :size="18" aria-hidden="true" />
        </span>
      </div>
      <strong class="metric-value">
        {{ report ? formatMoney(metric.value) : '—' }}
        <small>CNY</small>
      </strong>
      <p>{{ metric.caption }}</p>
    </article>
  </section>

  <section class="panel chart-panel monthly-chart-panel">
    <div class="panel-heading">
      <div>
        <h2>月度支出</h2>
        <p class="muted small-text">
          {{ year || '—' }} 年各月净支出，点击柱形查看当月明细
        </p>
      </div>
      <button
        type="button"
        class="text-link compact-link detail-link"
        @click="openMonth(selectedMonth)"
      >
        查看月度支出页
      </button>
    </div>
    <MonthlyExpenseChart
      v-if="report"
      :items="report.monthly"
      :year="year"
      :selected="selectedMonth"
      :busy="busy"
      @select="openMonth"
    />
    <div v-else class="chart-empty">
      <p>{{ busy ? '正在汇总月度支出…' : '暂无可展示的月度支出。' }}</p>
    </div>
  </section>

  <div class="overview-grid">
    <section class="panel category-panel">
      <div class="panel-heading">
        <div>
          <h2>分类支出</h2>
          <p class="muted small-text">{{ selectedMonth }} · 消费退款前金额</p>
        </div>
        <button
          type="button"
          class="text-link compact-link detail-link"
          @click="openMonth(selectedMonth)"
        >
          查看月明细
        </button>
      </div>
      <CategorySpendingChart
        v-if="hasCategoryData"
        :items="report!.categorySpending"
        :total-minor="report!.expenseMinor"
      />
      <div v-else class="compact-empty">
        <p>{{ busy ? '正在汇总分类支出…' : '当月暂无已确认消费。' }}</p>
        <router-link to="/transactions" class="text-link"
          >前往交易明细记录一笔</router-link
        >
      </div>
    </section>

    <section class="panel chart-panel facts-panel">
      <div class="panel-heading">
        <div>
          <h2>本月概况</h2>
          <p class="muted small-text">{{ selectedMonth }}</p>
        </div>
      </div>
      <dl class="month-facts">
        <div>
          <dt>消费支出</dt>
          <dd>{{ report ? formatMoney(report.expenseMinor) : '—' }}</dd>
        </div>
        <div>
          <dt>退款</dt>
          <dd>{{ report ? formatMoney(report.refundMinor) : '—' }}</dd>
        </div>
        <div>
          <dt>净支出</dt>
          <dd>{{ report ? formatMoney(report.netExpenseMinor) : '—' }}</dd>
        </div>
        <div>
          <dt>已确认 / 待核实</dt>
          <dd>
            {{
              report
                ? `${report.confirmedCount} / ${report.pendingCount} 笔`
                : '—'
            }}
          </dd>
        </div>
      </dl>
      <n-button block :disabled="busy" @click="openMonth(selectedMonth)"
        >打开月度支出</n-button
      >
    </section>
  </div>

  <p v-if="report" class="report-note" role="status">
    {{ year }} 年已确认 {{ annual.confirmed }} 笔，待核实 {{ annual.pending }}
    笔；待核实交易不计入收支。净支出
    {{ formatMoney(annual.netExpense.toString()) }} 元。
  </p>

  <section class="panel principles">
    <div>
      <ShieldCheck :size="22" aria-hidden="true" />
      <div>
        <h3>统计口径</h3>
        <p>只汇总已确认交易；退款按发生日冲减，转账及借贷本金不计收支。</p>
      </div>
    </div>
  </section>
</template>
