const utf8 = new TextDecoder()
const encoder = new TextEncoder()

export async function digest(bytes) {
  const hash = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(hash), byte => byte.toString(16).padStart(2, '0')).join('')
}

function csv(text) {
  const rows = []
  let row = []
  let field = ''
  let quoted = false
  for (let i = 0; i < text.length; i++) {
    const character = text[i]
    if (character === '"') {
      if (quoted && text[i + 1] === '"') { field += '"'; i++ }
      else if (quoted || !field) quoted = !quoted
      else field += character
    } else if (!quoted && character === ',') {
      row.push(field)
      field = ''
    } else if (!quoted && (character === '\n' || character === '\r')) {
      row.push(field)
      rows.push(row)
      row = []
      field = ''
      if (character === '\r' && text[i + 1] === '\n') i++
    } else {
      field += character
    }
  }
  if (quoted) throw new Error('CSV 包含未闭合的引号')
  if (field || row.length) rows.push([...row, field])
  return rows
}

function zipFiles(bytes) {
  const data = new DataView(bytes)
  const files = new Map()
  let end = data.byteLength - 22
  while (end >= Math.max(0, data.byteLength - 65557) && data.getUint32(end, true) !== 0x06054b50) end--
  if (end < Math.max(0, data.byteLength - 65557)) throw new Error('无法读取 XLSX 压缩目录')
  let offset = data.getUint32(end + 16, true)
  const entries = data.getUint16(end + 10, true)
  for (let i = 0; i < entries; i++) {
    if (data.getUint32(offset, true) !== 0x02014b50) throw new Error('XLSX 压缩目录损坏')
    const length = data.getUint16(offset + 28, true)
    const name = utf8.decode(new Uint8Array(bytes, offset + 46, length))
    files.set(name, {
      method: data.getUint16(offset + 10, true),
      size: data.getUint32(offset + 20, true),
      expanded: data.getUint32(offset + 24, true),
      local: data.getUint32(offset + 42, true),
    })
    offset += 46 + length + data.getUint16(offset + 30, true) + data.getUint16(offset + 32, true)
  }
  return async (name) => {
    const entry = files.get(name)
    if (!entry) return null
    const {method, size, expanded, local} = entry
    if (expanded > 96_000_000 || data.getUint32(local, true) !== 0x04034b50) throw new Error('XLSX 工作表过大或已损坏')
    const start = local + 30 + data.getUint16(local + 26, true) + data.getUint16(local + 28, true)
    const contents = new Uint8Array(bytes, start, size)
    let result = contents
    if (method === 8) {
      const stream = new Blob([contents]).stream().pipeThrough(new DecompressionStream('deflate-raw'))
      const reader = stream.getReader()
      result = new Uint8Array(expanded)
      let length = 0
      try {
        while (true) {
          const {value, done} = await reader.read()
          if (done) break
          if (length + value.length > expanded) throw new Error('XLSX 解压长度异常')
          result.set(value, length)
          length += value.length
        }
        if (length !== expanded) throw new Error('XLSX 解压长度异常')
      } finally {
        await reader.cancel()
      }
    } else if (method !== 0) {
      throw new Error('不支持该 XLSX 压缩方式')
    }
    const xml = new DOMParser().parseFromString(utf8.decode(result), 'application/xml')
    if (xml.getElementsByTagName('parsererror').length) throw new Error('XLSX 工作表 XML 无效')
    return xml
  }
}

const elements = (node, tag) => Array.from(node?.getElementsByTagNameNS('*', tag) || [])
const texts = (node) => elements(node, 't').map(x => x.textContent).join('')

async function xlsx(bytes) {
  const read = zipFiles(bytes)
  const workbook = await read('xl/workbook.xml')
  const rels = await read('xl/_rels/workbook.xml.rels')
  const sheets = elements(workbook, 'sheet')
  const index = Number(elements(workbook, 'workbookView')[0]?.getAttribute('activeTab') || 0)
  const sheet = sheets[index] || sheets[0]
  const relation = sheet?.getAttributeNS('http://schemas.openxmlformats.org/officeDocument/2006/relationships', 'id')
  const target = elements(rels, 'Relationship').find(x => x.getAttribute('Id') === relation)?.getAttribute('Target')
  if (!target) throw new Error('找不到微信账单的工作表')
  const path = target.startsWith('/') ? target.slice(1) : 'xl/' + target
  const strings = elements(await read('xl/sharedStrings.xml'), 'si').map(texts)
  const rows = elements(await read(path), 'row')
  const date1904 = ['1', 'true'].includes(elements(workbook, 'workbookPr')[0]?.getAttribute('date1904'))
  return rows.map(r => {
    const row = []
    for (const cell of elements(r, 'c')) {
      const reference = cell.getAttribute('r') || ''
      const letters = reference.match(/^[A-Z]+/)?.[0] || ''
      const col = letters ? [...letters].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0) - 1 : row.length
      if (col > 127) continue
      const type = cell.getAttribute('t')
      const value = elements(cell, 'v')[0]?.textContent ?? ''
      row[col] = type === 's' ? strings[Number(value)] ?? '' : type === 'inlineStr' ? texts(cell) : value
      if (col === 0 && (!type || type === 'n') && /^\d{5}(?:\.\d+)?$/.test(value)) {
        const origin = date1904 ? Date.UTC(1904, 0, 1) : Date.UTC(1899, 11, 30)
        row[col] = new Date(origin + Math.round(Number(value) * 86400) * 1000).toISOString().slice(0, 19).replace('T', ' ')
      }
    }
    return row
  })
}

function cents(value) {
  const parts = value.match(/[+-]?\d[\d,]*(?:\.\d+)?/)?.[0].replace(/[,+-]/g, '').split('.')
  if (!parts) throw new Error('缺少有效金额')
  return Number(parts[0]) * 100 + Number((parts[1] || '').padEnd(2, '0').slice(0, 2))
}

export async function parseLedger(file) {
  if (file.size > 48_000_000) throw new Error('账单文件超过 48 MB')
  const bytes = await file.arrayBuffer()
  let rows
  let source
  if (/\.xlsx$/i.test(file.name)) {
    rows = await xlsx(bytes)
    source = '微信'
  } else if (/\.csv$/i.test(file.name)) {
    let text
    try { text = new TextDecoder('utf-8', {fatal: true}).decode(bytes) }
    catch { text = new TextDecoder('gb18030', {fatal: true}).decode(bytes) }
    rows = csv(text)
    source = '支付宝'
  } else throw new Error('请选择支付宝 CSV 或微信 XLSX')
  const heading = rows.findIndex(row => row.some(cell => cell.trim() === '交易时间') && row.length > 6)
  if (heading < 0) throw new Error('未找到交易表头')
  const header = rows[heading].map(value => value.trim())
  if (!(source === '微信' ? header.includes('微信支付') || header.includes('交易单号') && header.includes('交易类型')
    : header.includes('交易订单号') && header.includes('商品说明'))) throw new Error('文件表头与所选平台格式不符')
  const report = {source, file: file.name, count: 0, duplicates: 0, skipped: 0, start: '', end: ''}
  const imported = []
  for (const row of rows.slice(heading + 1)) {
    if (!/^20\d\d-\d\d-\d\d/.test(row[0]?.trim() || '')) continue
    const item = Object.fromEntries(header.map((name, index) => [name, row[index]?.trim() || '']))
    const get = (...names) => names.map(name => item[name]).find(value => value && value !== '-' && value !== '/') || ''
    const time = get('交易时间').replace('T', ' ').slice(0, 19)
    const amountText = get('金额(元)', '金额')
    let amount
    try {
      amount = cents(amountText)
      if (!/^20\d\d-\d\d-\d\d \d\d:\d\d:\d\d$/.test(time) || !Number.isSafeInteger(amount) || amount > 9999999999) throw new Error()
    } catch {
      report.skipped++
      continue
    }
    const record = {
      source, time, amount, direction: get('收/支'), kind: get('交易类型', '交易分类'),
      state: get('当前状态', '交易状态') + (amountText.includes('已全额退款') ? ' 已全额退款' : ''),
      title: get('商品说明', '商品'), merchant: get('交易对方'), method: get('支付方式', '收/付款方式'),
      tx: get('交易单号', '交易订单号'), order: get('商户单号', '商家订单号'), account: '', note: get('备注'),
    }
    const identity = record.tx ? [source, record.tx, time, amount, record.direction, record.kind] : Object.values(record)
    // Match the original Python IDs so exported manual corrections remain attached.
    const encoded = '[' + identity.map(value => JSON.stringify(value)).join(', ') + ']'
    record.id = (await digest(encoder.encode(encoded))).slice(0, 24)
    const day = time.slice(0, 10)
    if (!report.start || day < report.start) report.start = day
    if (day > report.end) report.end = day
    report.count++
    imported.push(record)
  }
  return {digest: await digest(bytes), modified: file.lastModified, rows: imported, reports: [report]}
}
