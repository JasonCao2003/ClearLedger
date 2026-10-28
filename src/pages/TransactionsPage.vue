<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { onBeforeRouteLeave, useRoute } from 'vue-router'
import { NButton, NModal } from 'naive-ui'
import EntryEditor from '../components/EntryEditor.vue'
import { desktopAvailable } from '../api/client'
import {
  listEntries,
  natureLabel,
  natures,
  voidEntry,
  type Entry,
} from '../api/ledger'
import { formatMoney } from '../composables/money'
const route = useRoute()
const query = reactive({
  month: typeof route.query.month === 'string' ? route.query.month : '',
  nature: '',
  keyword: '',
})
const applied = ref({ ...query })
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
onBeforeRouteLeave(() => {
  if (voidBusy.value) return false
  if (editorOpen.value) return editor.value?.canLeave() ?? false
  return (
    !voiding.value ||
    !voidReason.value ||
    window.confirm('作废操作尚未提交，确定离开吗？')
  )
})
function resetFilters() {
  Object.assign(query, { month: '', nature: '', keyword: '' })
  void load()
}
let voidSignature = ''
let voidRequestId = ''
async function load(more = false) {
  if (busy.value) return
  if (!more) {
    applied.value = { ...query }
    entries.value = []
    cursor.value = { cursorDate: null, cursorId: null }
  }
  error.value = ''
  busy.value = true
  try {
    const result = await listEntries({
      ...applied.value,
      ...(more ? cursor.value : {}),
    })
    if (result.ok) {
      entries.value = more
        ? [...entries.value, ...result.data.items]
        : result.data.items
      cursor.value = {
        cursorDate: result.data.nextCursorDate,
        cursorId: result.data.nextCursorId,
      }
    } else error.value = `${result.error.message}（${result.error.code}）`
  } finally {
    busy.value = false
  }
}
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
    <form class="filter-bar" @submit.prevent="load()">
      <label
        >月份<input
          v-model="query.month"
          type="month"
          :disabled="busy"
          min="0001-01"
          max="9999-12" /></label
      ><label
        >性质<select v-model="query.nature" :disabled="busy">
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
          :disabled="busy" /></label
      ><n-button attr-type="submit" :loading="busy">查询</n-button
      ><n-button :disabled="busy" @click="resetFilters">重置</n-button>
    </form>
    <p v-if="notice" class="success-note" role="status">{{ notice }}</p>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
    <div class="table-scroll" :aria-busy="busy">
      <table class="entry-table">
        <thead>
          <tr>
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
          <tr v-for="entry in entries" :key="entry.id">
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
            : '暂无符合条件的交易。'
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
