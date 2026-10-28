<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { NModal, NButton } from 'naive-ui'
import {
  natures,
  saveEntry,
  type Entry,
  type Nature,
  type SaveEntry,
} from '../api/ledger'
import { editableYuan, parseYuan, todayShanghai } from '../composables/money'
const props = defineProps<{ show: boolean; entry: Entry | null }>()
const emit = defineEmits<{ close: []; saved: [] }>()
const form = reactive({
  businessDate: '',
  nature: 'expense' as Nature,
  amount: '',
  merchant: '',
  category: '',
  note: '',
  reviewStatus: 'confirmed' as 'pending' | 'confirmed',
})
const busy = ref(false)
const error = ref('')
const savedSignature = ref('')
const requestId = ref('')
const amountError = ref('')
const initial = ref('')
const dirty = computed(() => JSON.stringify(form) !== initial.value)
watch(
  () => props.show,
  (show) => {
    if (!show) return
    const entry = props.entry
    Object.assign(
      form,
      entry
        ? {
            businessDate: entry.businessDate,
            nature: entry.nature,
            amount: editableYuan(entry.amountMinor),
            merchant: entry.merchant,
            category: entry.category,
            note: entry.note,
            reviewStatus: entry.reviewStatus,
          }
        : {
            businessDate: todayShanghai(),
            nature: 'expense',
            amount: '',
            merchant: '',
            category: '',
            note: '',
            reviewStatus: 'confirmed',
          },
    )
    initial.value = JSON.stringify(form)
    error.value = ''
    amountError.value = ''
    savedSignature.value = ''
    requestId.value = ''
  },
)
watch(
  () => form.nature,
  (nature) => {
    if (nature === 'unknown') form.reviewStatus = 'pending'
  },
)
function canLeave() {
  return (
    !busy.value &&
    (!dirty.value || window.confirm('尚有未保存修改，确定放弃吗？'))
  )
}
defineExpose({ canLeave })
function close() {
  if (canLeave()) emit('close')
}
async function submit() {
  if (busy.value) return
  amountError.value = ''
  error.value = ''
  let amountMinor: string
  try {
    amountMinor = parseYuan(form.amount)
  } catch (e) {
    amountError.value = (e as Error).message
    return
  }
  const payload = {
    id: props.entry?.id ?? null,
    expectedRevision: props.entry?.revision ?? null,
    businessDate: form.businessDate,
    nature: form.nature,
    amountMinor,
    merchant: form.merchant,
    category: form.category,
    note: form.note,
    reviewStatus: form.reviewStatus,
  }
  const signature = JSON.stringify(payload)
  if (signature !== savedSignature.value) {
    requestId.value = crypto.randomUUID()
    savedSignature.value = signature
  }
  busy.value = true
  try {
    const result = await saveEntry({
      ...payload,
      requestId: requestId.value,
    } satisfies SaveEntry)
    if (result.ok) emit('saved')
    else error.value = `${result.error.message}（${result.error.code}）`
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <n-modal
    :show="show"
    preset="card"
    :title="entry ? '编辑交易' : '记一笔'"
    class="entry-modal"
    :closable="false"
    :mask-closable="false"
    :close-on-esc="false"
    role="dialog"
    aria-modal="true"
  >
    <form class="entry-form" @submit.prevent="submit">
      <div v-if="error" class="form-error" role="alert">{{ error }}</div>
      <div class="form-columns">
        <label
          >金额（元）<input
            v-model="form.amount"
            inputmode="decimal"
            placeholder="0.00"
            required
            :disabled="busy"
            :aria-invalid="!!amountError"
            aria-describedby="amount-error"
          /><span v-if="amountError" id="amount-error" class="field-error">{{
            amountError
          }}</span></label
        >
        <label
          >归属日期<input
            v-model="form.businessDate"
            type="date"
            min="0001-01-01"
            max="9999-12-31"
            required
            :disabled="busy"
        /></label>
        <label
          >交易性质<select v-model="form.nature" :disabled="busy">
            <option
              v-for="nature in natures"
              :key="nature.value"
              :value="nature.value"
            >
              {{ nature.label }}
            </option>
          </select></label
        >
        <label
          >核实状态<select
            v-model="form.reviewStatus"
            :disabled="busy || form.nature === 'unknown'"
          >
            <option value="confirmed">已确认</option>
            <option value="pending">待核实</option>
          </select></label
        >
        <label
          >商户 / 对方<input
            v-model="form.merchant"
            maxlength="200"
            :disabled="busy"
            placeholder="如：便利店、工资"
        /></label>
        <label
          >分类<input
            v-model="form.category"
            maxlength="80"
            :disabled="busy"
            placeholder="如：餐饮、日用购物"
        /></label>
      </div>
      <label
        >备注<textarea
          v-model="form.note"
          maxlength="2000"
          :disabled="busy"
          rows="3"
        />
      </label>
      <p class="muted">
        退款冲减退款当期支出；本人转账、本金还款和借贷往来不计入收支。待核实交易不计入正式汇总。退款关联原订单将在后续接入。
      </p>
      <div class="form-actions">
        <n-button :disabled="busy" @click="close">取消</n-button
        ><n-button type="primary" attr-type="submit" :loading="busy"
          >保存交易</n-button
        >
      </div>
    </form>
  </n-modal>
</template>
