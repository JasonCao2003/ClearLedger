<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { onBeforeRouteLeave, useRoute } from 'vue-router'
import { NButton, NModal } from 'naive-ui'
import EntryEditor from '../components/EntryEditor.vue'
import { desktopAvailable } from '../api/client'
import {
  confirmEntries,
  listEntries,
  listTransactionYears,
  natureLabel,
  natures,
  voidEntry,
  type Entry,
} from '../api/ledger'
import { formatMoney, todayShanghai } from '../composables/money'
const route = useRoute()
const today = todayShanghai()
const currentYear = today.slice(0, 4)
const currentMonth = today.slice(0, 7)
const routeMonth =
  typeof route.query.month === 'string' &&
  /^\d{4}-(0[1-9]|1[0-2])$/.test(route.query.month)
    ? route.query.month
    : currentMonth
const query = reactive({
  year:
    typeof route.query.year === 'string' && /^\d{4}$/.test(route.query.year)
      ? route.query.year
      : routeMonth.slice(0, 4),
  month: routeMonth.slice(5, 7),
  nature: '',
  keyword: typeof route.query.keyword === 'string' ? route.query.keyword : '',
})
const months = Array.from({ length: 12 }, (_, index) =>
  String(index + 1).padStart(2, '0'),
)
const years = ref<string[]>([])
// Keep the selected and current years selectable even when no entry falls in them.
const yearOptions = computed(() =>
  [...new Set([query.year, currentYear, ...years.value])]
    .filter(Boolean)
    .sort()
    .reverse(),
)
let yearsVersion = 0
async function loadYears() {
  const version = ++yearsVersion
  const result = await listTransactionYears()
  if (version !== yearsVersion) return
  if (result.ok) years.value = result.data
}
function onYearChange() {
  if (!query.year) query.month = ''
}
const entries = ref<Entry[]>([])
const busy = ref(false)
const error = ref('')
const notice = ref('')
const cursor = ref<{ cursorDate: string | null; cursorId: string | null }>({
  cursorDate: null,
  cursorId: null,
})
const editorOpen = ref(false)
const editor = ref<InstanceType<typeof EntryEditor> | null>(null)
const editing = ref<Entry | null>(null)
const voiding = ref<Entry | null>(null)
const voidReason = ref('')
const voidBusy = ref(false)
const voidError = ref('')
const selected = ref<Set<string>>(new Set())
const confirming = ref(false)
const confirmError = ref('')
const confirmable = () =>
  entries.value.filter(
    (entry) => entry.reviewStatus === 'pending' && entry.nature !== 'unknown',
  )
const selectedEntries = computed(() =>
  entries.value.filter((entry) => selected.value.has(entry.id)),
)
const selectedPendingCount = computed(
  () =>
    selectedEntries.value.filter(
      (entry) => entry.nature !== 'unknown' && entry.reviewStatus === 'pending',
    ).length,
)
const allConfirmableSelected = computed(() => {
  const targets = confirmable()
  return (
    targets.length > 0 && targets.every((entry) => selected.value.has(entry.id))
  )
})
const someConfirmableSelected = computed(
  () => selectedEntries.value.length > 0 && !allConfirmableSelected.value,
)
function pruneSelection() {
  const ids = new Set(entries.value.map((entry) => entry.id))
  selected.value = new Set([...selected.value].filter((id) => ids.has(id)))
}
function toggleRow(entry: Entry) {
  const next = new Set(selected.value)
  if (next.has(entry.id)) next.delete(entry.id)
  else next.add(entry.id)
  selected.value = next
}
function toggleAll() {
  const next = new Set(selected.value)
  const targets = confirmable()
  if (allConfirmableSelected.value) {
    for (const entry of targets) next.delete(entry.id)
  } else {
    for (const entry of targets) next.add(entry.id)
  }
  selected.value = next
}
async function confirmOne(entry: Entry) {
  await runConfirm([entry])
}
async function confirmSelected() {
  await runConfirm(selectedEntries.value)
}
async function runConfirm(targets: Entry[]) {
  if (confirming.value || busy.value) return
  const pendingTargets = targets.filter(
    (entry) => entry.reviewStatus === 'pending' && entry.nature !== 'unknown',
  )
  if (!pendingTargets.length) {
    confirmError.value =
      '所选交易均已确认，或性质仍为“待核实性质”，请先编辑填写性质。'
    return
  }
  const skipped = targets.length - pendingTargets.length
  if (
    !window.confirm(
      `确认核实 ${pendingTargets.length} 笔交易？` +
        (skipped ? `其余 ${skipped} 笔将跳过。` : '') +
        '核实后将计入正式收支。',
    )
  )
    return
  confirmError.value = ''
  confirming.value = true
  try {
    const result = await confirmEntries({
      requestId: crypto.randomUUID(),
      entries: pendingTargets.map((entry) => ({
        id: entry.id,
        expectedRevision: entry.revision,
      })),
    })
    if (result.ok) {
      selected.value = new Set()
      notice.value = `已核实 ${result.data.length} 笔交易。`
      await load()
    } else
      confirmError.value = `${result.error.message}（${result.error.code}）`
  } finally {
    confirming.value = false
  }
}
onBeforeRouteLeave(() => {
  if (voidBusy.value || confirming.value) return false
  if (editorOpen.value) return editor.value?.canLeave() ?? false
  return (
    !voiding.value ||
    !voidReason.value ||
    window.confirm('作废操作尚未提交，确定离开吗？')
  )
})
function resetFilters() {
  const changed =
    query.year !== currentYear ||
    query.month !== currentMonth.slice(5, 7) ||
    !!query.nature ||
    !!query.keyword
  Object.assign(query, {
    year: currentYear,
    month: currentMonth.slice(5, 7),
    nature: '',
    keyword: '',
  })
  if (!changed) void load()
}
let voidSignature = ''
let voidRequestId = ''
let requestVersion = 0
async function load(more = false) {
  if (more && (busy.value || !cursor.value.cursorId)) return
  const version = ++requestVersion
  const filters = {
    year: query.year,
    month: query.month ? `${query.year}-${query.month}` : '',
    nature: query.nature,
    keyword: query.keyword.trim(),
  }
  if (!more) {
    entries.value = []
    selected.value = new Set()
    cursor.value = { cursorDate: null, cursorId: null }
  }
  error.value = ''
  busy.value = true
  try {
    const result = await listEntries({
      ...filters,
      ...(more ? cursor.value : {}),
    })
    // Conditions can change while an older IPC request is still running.
    if (version !== requestVersion) return
    if (result.ok) {
      entries.value = more
        ? [...entries.value, ...result.data.items]
        : result.data.items
      cursor.value = {
        cursorDate: result.data.nextCursorDate,
        cursorId: result.data.nextCursorId,
      }
      pruneSelection()
    } else error.value = `${result.error.message}（${result.error.code}）`
  } finally {
    if (version === requestVersion) busy.value = false
  }
}
watch(
  () => [query.year, query.month, query.nature, query.keyword] as const,
  () => {
    void load()
  },
)
onBeforeUnmount(() => {
  ++requestVersion
})
function edit(entry: Entry | null) {
  editing.value = entry
  editorOpen.value = true
}
async function saved() {
  editorOpen.value = false
  notice.value = '交易已保存。'
  await load()
}
function beginVoid(entry: Entry) {
  voiding.value = entry
  voidReason.value = ''
  voidError.value = ''
  voidSignature = ''
  voidRequestId = ''
}
async function confirmVoid() {
  if (!voiding.value || voidBusy.value || !voidReason.value.trim()) return
  const payload = {
    id: voiding.value.id,
    expectedRevision: voiding.value.revision,
    reason: voidReason.value.trim(),
  }
  const signature = JSON.stringify(payload)
  if (signature !== voidSignature) {
    voidSignature = signature
    voidRequestId = crypto.randomUUID()
  }
  voidBusy.value = true
  try {
    const result = await voidEntry({ ...payload, requestId: voidRequestId })
    if (result.ok) {
      voiding.value = null
      notice.value = '交易已作废，历史记录已保留。'
      await load()
    } else voidError.value = `${result.error.message}（${result.error.code}）`
  } finally {
    voidBusy.value = false
  }
}
onMounted(() => {
  void load()
  if (desktopAvailable) void loadYears()
})
</script>
<template>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>手工交易</h2>
        <p class="muted small-text">
          原始文件导入将在后续接入，当前支持手工记账。
        </p>
      </div>
      <n-button
        type="primary"
        :disabled="!desktopAvailable || busy"
        @click="edit(null)"
        >记一笔</n-button
      >
    </div>
    <div class="filter-bar" role="search" aria-label="交易明细筛选">
      <label
        >年份<select v-model="query.year" @change="onYearChange">
          <option v-for="year in yearOptions" :key="year" :value="year">
            {{ year }} 年
          </option>
        </select></label
      ><label
        >月份<select v-model="query.month" :disabled="!query.year">
          <option value="">全年</option>
          <option v-for="month in months" :key="month" :value="month">
            {{ Number(month) }} 月
          </option>
        </select></label
      ><label
        >性质<select v-model="query.nature">
          <option value="">全部性质</option>
          <option
            v-for="nature in natures"
            :key="nature.value"
            :value="nature.value"
          >
            {{ nature.label }}
          </option>
        </select></label
      ><label class="search-field"
        >搜索<input
          v-model="query.keyword"
          maxlength="200"
          placeholder="商户、分类、备注"
          aria-describedby="transaction-filter-status" /></label
      ><n-button @click="resetFilters">重置</n-button>
      <span
        id="transaction-filter-status"
        class="muted small-text"
        role="status"
        aria-live="polite"
        >{{ busy ? '正在筛选…' : '筛选条件变更后自动更新' }}</span
      >
    </div>
    <p v-if="notice" class="success-note" role="status">{{ notice }}</p>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
    <div v-if="confirmError" class="form-error" role="alert">
      {{ confirmError }}
    </div>
    <div
      v-if="selectedEntries.length"
      class="bulk-bar"
      role="status"
      aria-live="polite"
    >
      <span
        >已选 {{ selectedEntries.length }} 笔，其中待核实
        {{ selectedPendingCount }} 笔</span
      ><n-button
        size="small"
        type="primary"
        :loading="confirming"
        :disabled="busy"
        @click="confirmSelected"
        >批量核实</n-button
      ><n-button
        size="small"
        :disabled="busy || confirming"
        @click="selected = new Set()"
        >清除选择</n-button
      >
    </div>
    <div class="table-scroll" :aria-busy="busy">
      <table class="entry-table">
        <thead>
          <tr>
            <th class="select-cell">
              <input
                type="checkbox"
                aria-label="选择本页全部可核实交易"
                :checked="allConfirmableSelected"
                :indeterminate="someConfirmableSelected"
                :disabled="busy || !confirmable().length"
                @change="toggleAll"
              />
            </th>
            <th>日期</th>
            <th>商户 / 对方</th>
            <th>性质</th>
            <th>分类</th>
            <th class="amount-cell">金额（元）</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="entry in entries"
            :key="entry.id"
            :class="{ 'row-selected': selected.has(entry.id) }"
          >
            <td class="select-cell">
              <input
                type="checkbox"
                :aria-label="`选择 ${entry.merchant || entry.businessDate}`"
                :checked="selected.has(entry.id)"
                :disabled="busy || entry.reviewStatus !== 'pending'"
                :title="
                  entry.reviewStatus !== 'pending' ? '该交易已确认' : undefined
                "
                @change="toggleRow(entry)"
              />
            </td>
            <td>{{ entry.businessDate }}</td>
            <td class="merchant-cell">
              <strong>{{ entry.merchant || '未填写' }}</strong
              ><small :title="entry.note">{{ entry.note }}</small>
            </td>
            <td>{{ natureLabel(entry.nature) }}</td>
            <td>{{ entry.category || '未分类' }}</td>
            <td class="amount-cell">{{ formatMoney(entry.amountMinor) }}</td>
            <td>
              <span class="subtle-tag">{{
                entry.reviewStatus === 'confirmed' ? '已确认' : '待核实'
              }}</span>
            </td>
            <td>
              <div class="row-actions">
                <n-button
                  size="small"
                  :disabled="busy || voidBusy"
                  @click="edit(entry)"
                  >编辑</n-button
                ><n-button
                  size="small"
                  :disabled="
                    busy ||
                    confirming ||
                    entry.reviewStatus !== 'pending' ||
                    entry.nature === 'unknown'
                  "
                  :title="
                    entry.reviewStatus !== 'pending'
                      ? '该交易已确认'
                      : entry.nature === 'unknown'
                        ? '请先编辑填写确切性质'
                        : undefined
                  "
                  @click="confirmOne(entry)"
                  >核实</n-button
                ><n-button
                  size="small"
                  :disabled="busy || voidBusy"
                  @click="beginVoid(entry)"
                  >作废</n-button
                >
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="!entries.length" class="table-empty">
      {{
        busy
          ? '正在读取交易…'
          : error
            ? '交易读取失败，请重试。'
            : '暂无符合条件的交易，可调整年份、月份、性质或搜索词。'
      }}
    </div>
    <div class="list-footer">
      <span class="muted"
        >已显示 {{ entries.length }} 条 · 金额按性质区分，不代表账户余额</span
      ><n-button v-if="cursor.cursorId" :loading="busy" @click="load(true)"
        >加载更多</n-button
      >
    </div>
  </section>
  <EntryEditor
    ref="editor"
    :show="editorOpen"
    :entry="editing"
    @close="editorOpen = false"
    @saved="saved"
  />
  <n-modal
    :show="!!voiding"
    preset="card"
    title="作废交易"
    class="entry-modal"
    :closable="false"
    :mask-closable="false"
    :close-on-esc="false"
    ><form class="entry-form" @submit.prevent="confirmVoid">
      <p>作废后不再计入统计，交易及修改历史仍然保留。</p>
      <label
        >作废原因<textarea
          v-model="voidReason"
          required
          maxlength="500"
          :disabled="voidBusy"
        />
      </label>
      <div v-if="voidError" class="form-error" role="alert">
        {{ voidError }}
      </div>
      <div class="form-actions">
        <n-button :disabled="voidBusy" @click="voiding = null">取消</n-button
        ><n-button attr-type="submit" type="error" :loading="voidBusy"
          >确认作废</n-button
        >
      </div>
    </form></n-modal
  >
</template>
