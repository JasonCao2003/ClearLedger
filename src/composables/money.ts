export function formatMoney(value: string): string {
  const amount = BigInt(value)
  const absolute = amount < 0n ? -amount : amount
  const integer = (absolute / 100n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ',')
  return `${amount < 0n ? '−' : ''}${integer}.${(absolute % 100n).toString().padStart(2, '0')}`
}
export function parseYuan(value: string): string {
  const input = value.trim()
  if (!/^\d{1,17}(\.\d{1,2})?$/.test(input))
    throw new Error('请输入有效金额，最多两位小数。')
  const [integer, fraction = ''] = input.split('.')
  const amount = BigInt(integer!) * 100n + BigInt(fraction.padEnd(2, '0'))
  if (amount <= 0n || amount > 9223372036854775807n)
    throw new Error('金额必须大于零，且不超过系统范围。')
  return amount.toString()
}
export function editableYuan(value: string): string {
  const amount = BigInt(value)
  return `${amount / 100n}.${(amount % 100n).toString().padStart(2, '0')}`
}
export function todayShanghai(): string {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Asia/Shanghai',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).formatToParts(new Date())
  const field = (name: string) =>
    parts.find((part) => part.type === name)!.value
  return `${field('year')}-${field('month')}-${field('day')}`
}
