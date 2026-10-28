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
export interface EntryQuery {
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
export const saveEntry = (input: SaveEntry) =>
  command<Entry>('save_transaction', { input })
export const getOverview = (month: string) =>
  command<Overview>('get_overview', { month })
export const voidEntry = (input: {
  id: string
  expectedRevision: number
  requestId: string
  reason: string
}) => command<Entry>('void_transaction', { input })
export const natureLabel = (value: string) =>
  natures.find((item) => item.value === value)?.label ?? value
