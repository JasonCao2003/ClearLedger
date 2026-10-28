import { command } from './ledger'
export interface RefundLink {
  refundId: string
  expenseId: string | null
  state: 'linked' | 'review' | 'unmatched' | 'conflict' | 'rejected' | 'stale'
  refundRevision: number
  expenseRevision: number | null
  amountMinor: string
  businessDate: string
  merchant: string
  expenseDate: string | null
  expenseMerchant: string | null
  sources: string[]
  candidateIds: string[]
  evidence: string[]
  ruleVersion: string
}
export interface AmountCheck {
  expenseId: string
  expenseRevision: number
  merchant: string
  businessDate: string
  exportedMinor: string
  mode: 'gross' | 'net' | 'unknown' | 'conflict' | 'stale'
  grossMinor: string | null
  refundMinor: string
  evidence: string[]
  ruleVersion: string
}
export interface RefundReport {
  links: RefundLink[]
  checks: AmountCheck[]
  evidenceAvailable: boolean
}
export const getRefundReport = () =>
  command<RefundReport>('get_refund_report', {})
export const scanRefunds = () => command<RefundReport>('scan_refunds', {})
export const rejectRefundLink = (link: RefundLink, reset = false) =>
  command<RefundReport>('reject_refund_link', {
    input: {
      refundId: link.refundId,
      expectedRefundRevision: link.refundRevision,
      expectedExpenseRevision: link.expenseRevision,
      reset,
    },
  })
export const stateLabels: Record<RefundLink['state'], string> = {
  linked: '标识已关联',
  review: '多订单待核实',
  unmatched: '未找到原订单',
  conflict: '证据冲突',
  rejected: '人工已驳回',
  stale: '记录变更待重查',
}
export const modeLabels: Record<AmountCheck['mode'], string> = {
  gross: '原支出金额',
  net: '已扣退款净额',
  unknown: '金额口径待核实',
  conflict: '金额校验冲突',
  stale: '记录变更待重查',
}
