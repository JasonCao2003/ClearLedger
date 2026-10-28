<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
import { NButton, NPagination } from 'naive-ui'
import { desktopAvailable } from '../api/client'
import {
  getRefundReport,
  scanRefunds,
  rejectRefundLink,
  stateLabels,
  modeLabels,
  type RefundLink,
  type RefundReport,
} from '../api/refunds'
import { formatMoney } from '../composables/money'
const report = ref<RefundReport>({
  links: [],
  checks: [],
  evidenceAvailable: false,
})
const busy = ref(false)
const ready = ref(false)
const error = ref('')
const errorBox = ref<HTMLElement | null>(null)
const page = ref(1)
const checkPage = ref(1)
const filter = ref('all')
const mode = ref('all')
const links = computed(() =>
  report.value.links.filter(
    (link) => filter.value === 'all' || link.state === filter.value,
  ),
)
const checks = computed(() =>
  report.value.checks.filter(
    (check) => mode.value === 'all' || check.mode === mode.value,
  ),
)
const shown = computed(() =>
  links.value.slice((page.value - 1) * 20, page.value * 20),
)
const shownChecks = computed(() =>
  checks.value.slice((checkPage.value - 1) * 20, checkPage.value * 20),
)
const linkedCount = computed(
  () => report.value.links.filter((link) => link.state === 'linked').length,
)
const verifiedCount = computed(
  () =>
    report.value.checks.filter((check) => ['gross', 'net'].includes(check.mode))
      .length,
)
async function load(scan = false, link?: RefundLink) {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const result = link
      ? await rejectRefundLink(link, link.state === 'rejected')
      : scan
        ? await scanRefunds()
        : await getRefundReport()
    if (result.ok) {
      ready.value = true
      report.value = result.data
      page.value = 1
      checkPage.value = 1
    } else {
      error.value = `${result.error.message}（${result.error.code}）`
      await nextTick()
      errorBox.value?.focus()
    }
  } finally {
    busy.value = false
  }
}
function decide(link: RefundLink) {
  if (
    window.confirm(
      link.state === 'rejected'
        ? '撤销驳回并重新按证据校验此关联？'
        : '驳回此退款与原订单的关联？该退款将保留待核实，并重新计算相关金额口径。',
    )
  )
    void load(false, link)
}
const transactionUrl = (id: string, date: string) => ({
  path: '/transactions',
  query: { keyword: id, month: date.slice(0, 7) },
})
onMounted(() => {
  if (desktopAvailable) void load()
})
</script>
<template>
  <section class="panel refund-intro">
    <div class="panel-heading">
      <div>
        <h2>退款与原订单</h2>
        <p class="muted small-text">
          按渠道和带类型的订单标识关联，保留部分退款与独立退款事件。
        </p>
      </div>
      <div class="row-actions">
        <n-button :disabled="!desktopAvailable || busy" @click="load()"
          >刷新</n-button
        ><n-button
          type="primary"
          :loading="busy"
          :disabled="!desktopAvailable"
          @click="load(true)"
          >扫描退款与净额</n-button
        >
      </div>
    </div>
    <p class="muted small-text">
      扫描只保存关联和校验结果，不修改原始流水或自动确认交易。原订单及所有关联退款均确认、且金额口径验证通过后才进入正式统计。原消费行的“已退款”状态不会额外生成退款。
    </p>
    <p
      v-if="desktopAvailable && ready && !report.evidenceAvailable && !busy"
      class="report-note"
    >
      尚无可分析的来源凭证。请先导入旧 HTML
      账单；手工交易暂不具备自动匹配所需的订单标识。
    </p>
    <div
      v-if="error"
      ref="errorBox"
      role="alert"
      tabindex="-1"
      class="form-error"
    >
      {{ error }}
    </div>
    <div class="refund-counts" role="status">
      <span
        >退款记录 <strong>{{ ready ? report.links.length : '—' }}</strong></span
      ><span
        >标识已关联 <strong>{{ ready ? linkedCount : '—' }}</strong></span
      ><span
        >口径已验证 <strong>{{ ready ? verifiedCount : '—' }}</strong></span
      ><span
        >口径待核实 / 冲突
        <strong>{{
          ready ? report.checks.length - verifiedCount : '—'
        }}</strong></span
      >
    </div>
  </section>
  <section class="panel refund-section" :aria-busy="busy">
    <div class="panel-heading">
      <h2>退款关联结果</h2>
      <label class="inline-filter"
        >关联状态<select v-model="filter" :disabled="busy" @change="page = 1">
          <option value="all">全部状态</option>
          <option
            v-for="(label, value) in stateLabels"
            :key="value"
            :value="value"
          >
            {{ label }}
          </option>
        </select></label
      >
    </div>
    <div class="table-scroll">
      <table class="entry-table">
        <thead>
          <tr>
            <th>退款日期 / 渠道</th>
            <th>退款与原订单</th>
            <th class="amount-cell">退款（元）</th>
            <th>状态 / 证据</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="link in shown" :key="link.refundId">
            <td>
              {{ link.businessDate
              }}<small class="refund-meta">{{
                link.sources.join(' / ')
              }}</small>
            </td>
            <td class="refund-identity">
              <router-link
                :to="transactionUrl(link.refundId, link.businessDate)"
                class="text-link"
                >{{ link.merchant || '退款交易' }}</router-link
              ><small
                v-if="link.expenseId && link.expenseDate"
                class="refund-meta"
                >原订单：<router-link
                  :to="transactionUrl(link.expenseId, link.expenseDate)"
                  >{{ link.expenseMerchant || link.expenseId }} ·
                  {{ link.expenseDate }}</router-link
                ></small
              ><small v-else class="refund-meta">{{
                link.candidateIds.length
                  ? `${link.candidateIds.length} 个原订单候选`
                  : '暂无明确原订单'
              }}</small>
            </td>
            <td class="amount-cell">{{ formatMoney(link.amountMinor) }}</td>
            <td class="refund-proof">
              <span class="subtle-tag">{{ stateLabels[link.state] }}</span>
              <details>
                <summary>查看关联依据</summary>
                <ul>
                  <li v-for="(item, index) in link.evidence" :key="index">
                    {{ item }}
                  </li>
                </ul>
                <small>{{ link.ruleVersion }}</small>
              </details>
            </td>
            <td>
              <n-button
                v-if="
                  ['linked', 'rejected'].includes(link.state) &&
                  link.expenseRevision !== null
                "
                size="small"
                :disabled="busy"
                @click="decide(link)"
                >{{
                  link.state === 'rejected' ? '撤销驳回' : '驳回关联'
                }}</n-button
              ><span v-else class="muted small-text">需核对原始凭证</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="!shown.length" class="table-empty">
      {{ busy ? '正在分析…' : '当前条件下没有退款关联结果。' }}
    </div>
    <div class="list-footer">
      <span class="muted"
        >共 {{ links.length }} 条 · 标识关联不等于人工确认</span
      ><n-pagination
        v-if="links.length > 20"
        v-model:page="page"
        :item-count="links.length"
        :page-size="20"
        :disabled="busy"
      />
    </div>
  </section>
  <section class="panel refund-section">
    <div class="panel-heading">
      <div>
        <h2>原订单金额口径</h2>
        <p class="muted small-text">
          区分原支出与已扣退款净额；订单标价和优惠券差额不作为退款证据。
        </p>
      </div>
      <label class="inline-filter"
        >校验状态<select
          v-model="mode"
          :disabled="busy"
          @change="checkPage = 1"
        >
          <option value="all">全部口径</option>
          <option
            v-for="(label, value) in modeLabels"
            :key="value"
            :value="value"
          >
            {{ label }}
          </option>
        </select></label
      >
    </div>
    <div class="table-scroll">
      <table class="entry-table">
        <thead>
          <tr>
            <th>原订单</th>
            <th class="amount-cell">导出金额（元）</th>
            <th class="amount-cell">累计退款（元）</th>
            <th class="amount-cell">验证原实付（元）</th>
            <th>口径 / 校验依据</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="check in shownChecks" :key="check.expenseId">
            <td class="refund-identity">
              <router-link
                :to="transactionUrl(check.expenseId, check.businessDate)"
                class="text-link"
                >{{ check.merchant || '原订单' }}</router-link
              ><small class="refund-meta">{{ check.businessDate }}</small>
            </td>
            <td class="amount-cell">{{ formatMoney(check.exportedMinor) }}</td>
            <td class="amount-cell">{{ formatMoney(check.refundMinor) }}</td>
            <td class="amount-cell">
              {{ check.grossMinor ? formatMoney(check.grossMinor) : '—' }}
            </td>
            <td class="refund-proof">
              <span class="subtle-tag">{{ modeLabels[check.mode] }}</span>
              <details>
                <summary>查看金额校验</summary>
                <ul>
                  <li v-for="(item, index) in check.evidence" :key="index">
                    {{ item }}
                  </li>
                </ul>
              </details>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="!shownChecks.length" class="table-empty">
      当前没有原订单金额校验结果。
    </div>
    <div class="list-footer">
      <span class="muted"
        >共 {{ checks.length }} 条 · 未通过的关联组暂停计入正式收支</span
      ><n-pagination
        v-if="checks.length > 20"
        v-model:page="checkPage"
        :item-count="checks.length"
        :page-size="20"
        :disabled="busy"
      />
    </div>
  </section>
</template>
