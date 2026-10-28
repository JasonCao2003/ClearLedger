import { invoke } from '@tauri-apps/api/core'
import { desktopAvailable } from './client'
import type { ApiResult } from './types'

export const natures = [
  { value: 'expense', label: '消费支出' },
  { value: 'income', label: '收入' },
  { value: 'refund', label: '退款' },
  { value: 'transfer', label: '本人转账' },
  { value: 'repayment', label: '本金还款' },
  { value: 'loan', label: '借贷往来' },
  { value: 'unknown', label: '待核实性质' },
] as const
export const spendingCategories = [
  '餐饮',
  '酒水',
  '交通',
  '日用',
  '衣物',
  '学习',
  '红包',
  '娱乐',
  '水电房租',
  '服务',
  '其他',
] as const
export type Nature = (typeof natures)[number]['value']
export interface Entry {
  id: string
  businessDate: string
  nature: Nature
  amountMinor: string
  merchant: string
  category: string
  note: string
  reviewStatus: 'pending' | 'confirmed'
  lifecycle: 'active' | 'voided'
  revision: number
}
export interface SaveEntry {
  id: string | null
  expectedRevision: number | null
  requestId: string
  businessDate: string
  nature: Nature
  amountMinor: string
  merchant: string
  category: string
  note: string
  reviewStatus: 'pending' | 'confirmed'
}
export interface ConfirmEntries {
  requestId: string
  entries: { id: string; expectedRevision: number }[]
}
export interface EntryQuery {
  year: string
  month: string
  nature: string
  keyword: string
  cursorDate?: string | null
  cursorId?: string | null
}
export interface EntryPage {
  items: Entry[]
  nextCursorDate: string | null
  nextCursorId: string | null
}
export interface Overview {
  incomeMinor: string
  expenseMinor: string
  refundMinor: string
  netExpenseMinor: string
  balanceMinor: string
  confirmedCount: number
  pendingCount: number
  monthly: MonthlyOverview[]
  categorySpending: CategorySpending[]
}
export interface MonthlyOverview {
  month: string
  incomeMinor: string
  expenseMinor: string
  refundMinor: string
  netExpenseMinor: string
  balanceMinor: string
  confirmedCount: number
  pendingCount: number
}
export interface CategorySpending {
  category: string
  amountMinor: string
}
export interface MonthDetail {
  month: string
  days: DailySpending[]
  totals: MonthDetailTotals
}
export interface DailySpending {
  date: string
  incomeMinor: string
  expenseMinor: string
  refundMinor: string
  netExpenseMinor: string
  pendingCount: number
  categories: Record<string, string>
  memo: string
}
export interface MonthDetailTotals {
  incomeMinor: string
  expenseMinor: string
  refundMinor: string
  netExpenseMinor: string
  pendingCount: number
  categories: Record<string, string>
}
export async function command<T>(
  name: string,
  args: Record<string, unknown>,
): Promise<ApiResult<T>> {
  if (!desktopAvailable)
    return {
      ok: false,
      error: {
        code: 'DESKTOP_REQUIRED',
        message: '请在桌面应用中使用记账功能。',
        traceId: 'browser-preview',
      },
    }
  try {
    return await invoke<ApiResult<T>>(name, args)
  } catch {
    return {
      ok: false,
      error: {
        code: 'IPC_UNAVAILABLE',
        message:
          '本地服务响应失败，可使用相同内容重试；如内容已保存，请刷新确认。',
        traceId: crypto.randomUUID(),
      },
    }
  }
}
export const listEntries = (query: EntryQuery) =>
  command<EntryPage>('list_transactions', { query })
export const confirmEntries = (input: ConfirmEntries) =>
  command<Entry[]>('confirm_transactions', { input })
export const listTransactionYears = () =>
  command<string[]>('list_transaction_years', {})
export const saveEntry = (input: SaveEntry) =>
  command<Entry>('save_transaction', { input })
export const getOverview = (month: string) =>
  command<Overview>('get_overview', { month })
export const getMonthDetail = (month: string) =>
  command<MonthDetail>('get_month_detail', { month })
export const voidEntry = (input: {
  id: string
  expectedRevision: number
  requestId: string
  reason: string
}) => command<Entry>('void_transaction', { input })
export const natureLabel = (value: string) =>
  natures.find((item) => item.value === value)?.label ?? value
