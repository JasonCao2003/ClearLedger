<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton } from 'naive-ui'
import { getOverview, type Overview } from '../api/ledger'
import { formatMoney, todayShanghai } from '../composables/money'
import {
  ArrowDownLeft,
  ArrowUpRight,
  RotateCcw,
  Scale,
  ReceiptText,
  ShieldCheck,
} from 'lucide-vue-next'
const month = ref(todayShanghai().slice(0, 7))
const report = ref<Overview | null>(null)
const busy = ref(false)
const error = ref('')
async function refresh() {
  if (busy.value) return
  busy.value = true
  report.value = null
  error.value = ''
  try {
    const result = await getOverview(month.value)
    if (result.ok) report.value = result.data
    else error.value = result.error.message
  } finally {
    busy.value = false
  }
}
onMounted(() => {
  void refresh()
})
const metrics = computed(() => [
  {
    name: '收入',
    value: report.value?.incomeMinor,
    caption: '期间确认收入',
    icon: ArrowDownLeft,
    tone: 'green',
  },
  {
    name: '消费支出',
    value: report.value?.expenseMinor,
    caption: '退款前的消费金额',
    icon: ArrowUpRight,
    tone: 'blue',
  },
  {
    name: '退款',
    value: report.value?.refundMinor,
    caption: '按退款发生日计入',
    icon: RotateCcw,
    tone: 'orange',
  },
  {
    name: '期间结余',
    value: report.value?.balanceMinor,
    caption: '收入减去净支出',
    icon: Scale,
    tone: 'purple',
  },
])
</script>
<template>
  <section class="summary-strip">
    <div>
      <span class="section-kicker">账本概况</span>
      <p>已确认手工交易 · {{ month || '全部日期' }}</p>
    </div>
    <form class="filter-bar" @submit.prevent="refresh">
      <label
        >月份<input
          v-model="month"
          type="month"
          min="0001-01"
          max="9999-12"
          :disabled="busy"
          @change="refresh" /></label
      ><n-button attr-type="submit" :loading="busy">刷新</n-button>
    </form>
  </section>
  <p v-if="error" class="form-error" role="alert">{{ error }}</p>
  <section class="metric-grid" aria-label="收支概况">
    <article v-for="metric in metrics" :key="metric.name" class="metric-card">
      <div class="metric-top">
        <span>{{ metric.name }}</span
        ><span class="metric-icon" :class="metric.tone"
          ><component :is="metric.icon" :size="18" aria-hidden="true"
        /></span>
      </div>
      <strong class="metric-value"
        >{{ metric.value === undefined ? '—' : formatMoney(metric.value)
        }}<small>CNY</small></strong
      >
      <p>{{ metric.caption }}</p>
    </article>
  </section>
  <p v-if="report" class="report-note" role="status">
    已确认 {{ report.confirmedCount }} 笔 · 待核实
    {{ report.pendingCount }} 笔（不计收支） · 净支出
    {{ formatMoney(report.netExpenseMinor) }}
    元。仅反映手工记录，不表示账单覆盖完整。<router-link
      :to="{ path: '/transactions', query: { month } }"
      class="text-link"
      >查看当期明细 →</router-link
    >
  </p>
  <div class="overview-grid">
    <section class="panel">
      <div class="panel-heading">
        <h2>收支趋势</h2>
        <span class="muted">月度视图</span>
      </div>
      <div class="chart-empty">
        <div class="empty-chart" aria-hidden="true">
          <i /><i /><i /><i /><i /><i /><i />
        </div>
        <h3>趋势图待接入</h3>
        <p>上方指标已接入真实汇总，月度趋势将在后续提供。</p>
      </div>
    </section>
    <section class="panel">
      <div class="panel-heading">
        <h2>开始使用</h2>
        <span class="subtle-tag">开发中</span>
      </div>
      <ol class="getting-started">
        <li>
          <span>01</span>
          <div>
            <h3>建立基础账本</h3>
            <p>桌面启动时创建本地 SQLite 数据库。</p>
          </div>
        </li>
        <li>
          <span>02</span>
          <div>
            <h3>开始手工记账</h3>
            <p>
              <router-link to="/transactions" class="text-link"
                >新增、编辑或核实一笔交易 →</router-link
              >
            </p>
          </div>
        </li>
        <li>
          <span>03</span>
          <div>
            <h3>核对后查看统计</h3>
            <p>保留来源凭证，不确定记录单独核实。</p>
          </div>
        </li>
      </ol>
      <router-link to="/settings" class="text-link"
        >查看本地服务状态 →</router-link
      >
    </section>
  </div>
  <section class="panel principles">
    <div>
      <ShieldCheck :size="22" aria-hidden="true" />
      <div>
        <h3>数据有来源，统计有口径</h3>
        <p>原始凭证与记账交易分开保存，跨渠道同一消费只计一次。</p>
      </div>
    </div>
    <ReceiptText :size="26" class="muted" aria-hidden="true" />
  </section>
</template>
