import {invoke} from '@tauri-apps/api/core'
import {digest, parseLedger} from './import-ledger.js'

const $ = (id) => document.getElementById(id)
const names = {stats: '消费统计', calendar: '消费日历', details: '月度明细', debt: '分期还款', imports: '导入核对'}
const categories = ['饮食', '出行', '基本支出', '生活', '娱乐', '学习', '待分类']
const platforms = ['支付宝', '微信', '美团', '京东', '工商银行']
const current = new Intl.DateTimeFormat('sv-SE', {timeZone: 'Asia/Shanghai'}).format(new Date()).slice(0, 7)
const emptySettings = () => ({categories: {}, links: {}, roles: {}, debts: {}, merchants: {}, merchantNotes: {}})
let settings = emptySettings()
let source = {rows: [], reports: []}
let ledger = []
let page = 'stats'
let filter = {category: '', source: '', state: '', day: '', query: ''}
let limit = 100
let ready = false
let importing = false
let pendingSave = Promise.resolve()
$('month').value = current

async function request(command, input) {
  const result = await invoke(command, input)
  if (!result.ok) throw new Error(result.error.message)
  return result.data
}

function save() {
  const snapshot = structuredClone(settings)
  pendingSave = pendingSave.then(() => request('save_settings', {settings: snapshot})).catch(() => {
    $('status').textContent = '数据库保存失败。请导出核对设置，避免丢失当前修改。'
  })
  rebuild()
  render()
}
const esc = (value) => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[c]))
const money = (cents) => (cents / 100).toLocaleString('zh-CN', {style: 'currency', currency: 'CNY'})
const total = (rows, field = 'net') => rows.reduce((sum, row) => sum + row[field], 0)
const month = () => $('month').value || current
const option = (value, selected) => '<option' + (value === selected ? ' selected' : '') + '>' + esc(value) + '</option>'
const normalizeMerchant = (name) => name.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
const merchantKey = (name) => '商户:' + normalizeMerchant(name)

function role(r) {
  if (settings.roles[r.id]) return settings.roles[r.id]
  if (/还款失败|交易关闭|已关闭|支付失败|处理中|放款成功/.test(r.state)) return '排除'
  if (/本人账户互转|提现|零钱充值|零钱通|理财|小金库/.test(r.kind) ||
      /余额宝.*转入|余额宝.*转出/.test(r.title) ||
      (r.source === '工商银行' && /提现/.test(r.merchant + r.kind))) return '资金划转'
  if (r.kind === '还款' || /还款成功/.test(r.state)) return '还款'
  if (r.source === '工商银行' && /还款/.test(r.kind + r.merchant)) return '待判定'
  if (/退款|退还/.test(r.kind) || /退款成功/.test(r.state) ||
      (r.source === '工商银行' && r.direction === '收入' && /退款/.test(r.merchant + r.kind))) return '退款'
  if (r.source !== '工商银行' && /红包|转账|群收款|二维码收款/.test(r.kind)) return '待判定'
  if (r.direction === '支出' || /已全额退款/.test(r.state)) return '消费'
  return r.direction === '收入' ? '收入' : '待判定'
}

function category(r) {
  const text = [r.title, r.merchant, r.kind].join(' ')
  if (/chatgpt|openai|教育培训|学费|文具|教材|数码电器/i.test(text)) return '学习'
  if (/电影|影院|视频会员|文化休闲|音乐会员|游戏|影视|爱奇艺|腾讯视频|哔哩哔哩/.test(text)) return '娱乐'
  if (/餐饮美食|食品酒饮|外卖|餐厅|奶茶|快餐|水果|零食|饿了么|咖啡/.test(text)) return '饮食'
  if (/交通出行|爱车养车|滴滴|高德打车|公交|地铁|铁路|高铁|加油/.test(text)) return '出行'
  if (/水费|电费|燃气|话费|手机缴费|宽带|房租|网费/.test(text)) return '基本支出'
  if (/日用百货|服饰装扮|医疗健康|家居家装|美容美发|医院|药房|日用品/.test(text)) return '生活'
  return '待分类'
}
function methodChannel(r) {
  const text = r.source === '工商银行' ? r.merchant : r.method
  if (/支付宝/.test(text)) return '支付宝'
  if (/财付通|微信/.test(text)) return '微信'
  if (/美团/.test(text)) return '美团'
  if (/京东/.test(text)) return '京东'
  return ''
}
function seconds(r) { return Date.parse(r.time.replace(' ', 'T') + '+08:00') / 1000 }

function rebuild() {
  const byId = new Map()
  const payments = new Map()
  const orders = new Map()
  const bucket = new Map()
  const directed = new Map()
  const marketplaceLinks = new Map()
  ledger = source.rows.map(row => ({...row, role: role(row), linked: [], reason: '', flags: []}))
  for (const r of ledger) {
    byId.set(r.id, r)
    if (r.role !== '消费') continue
    for (const id of [r.tx, r.order].filter(Boolean)) {
      const key = r.source + '|' + id
      if (!orders.has(key)) orders.set(key, new Set())
      orders.get(key).add(r)
    }
    const key = r.source + '|' + r.amount
    if (!bucket.has(key)) bucket.set(key, [])
    bucket.get(key).push(r)
    payments.set(r.id, r)
  }
  for (const r of ledger) {
    if (Object.hasOwn(settings.links, r.id)) {
      const target = payments.get(settings.links[r.id])
      if (target && target.id !== r.id) directed.set(r.id, [target, '人工核对'])
      else if (settings.links[r.id]) r.flags.push('人工关联目标缺失')
      continue
    }
    if (r.role === '退款') {
      const candidates = new Set([r.order, r.tx].filter(Boolean).flatMap(id => [...(orders.get(r.source + '|' + id) || [])]))
      const valid = [...candidates].filter(p => p.time <= r.time && p.amount >= r.amount)
      if (valid.length === 1) directed.set(r.id, [valid[0], '同平台订单号'])
      continue
    }
    if (r.role !== '消费') continue
    const channel = methodChannel(r)
    const funding = r.source === '工商银行'
    const marketplace = r.source === '美团' || r.source === '京东'
    if (!channel || channel === r.source || (!funding && !marketplace)) continue
    const candidates = (bucket.get(channel + '|' + r.amount) || []).filter(p => Math.abs(seconds(r) - seconds(p)) <= 120)
    const strong = candidates.filter(p => funding
      ? /工商银行|工行/.test(p.method) && r.account && p.method.includes(r.account)
      : Math.abs(seconds(r) - seconds(p)) <= 2)
    if (candidates.length === 1 && strong.length === 1) {
      if (funding) directed.set(r.id, [strong[0], '金额、时间、渠道和银行卡尾号一致'])
      else if (!Object.hasOwn(settings.links, strong[0].id)) {
        const id = strong[0].id
        if (!marketplaceLinks.has(id)) marketplaceLinks.set(id, [])
        marketplaceLinks.get(id).push(r)
      }
    } else if (candidates.length) r.flags.push('跨平台同额候选 ' + candidates.length + ' 笔')
    else if (funding) r.flags.push('平台扣款待核对')
  }
  for (const [id, matches] of marketplaceLinks) {
    if (matches.length === 1) directed.set(id, [matches[0], '实付金额、渠道和支付时间一致'])
    else for (const row of matches) row.flags.push('多笔平台订单竞争同一支付流水')
  }
  const incoming = new Map()
  for (const [id, [target, reason]] of directed) {
    if (reason === '人工核对' || byId.get(id).role === '退款') continue
    const k = target.id + '|' + byId.get(id).source
    if (!incoming.has(k)) incoming.set(k, [])
    incoming.get(k).push(id)
  }
  for (const ids of incoming.values()) {
    if (ids.length < 2) continue
    for (const id of ids) {
      directed.delete(id)
      byId.get(id).flags.push('多笔流水竞争同一订单')
    }
  }
  for (const r of ledger) {
    if (!directed.has(r.id)) continue
    const visited = new Set([r.id])
    let cursor = r
    while (directed.has(cursor.id) && !visited.has(directed.get(cursor.id)[0].id)) {
      cursor = directed.get(cursor.id)[0]
      visited.add(cursor.id)
    }
    if (directed.has(cursor.id)) {
      r.flags.push('关联存在循环')
      continue
    }
    r.parent = cursor.id
    r.reason = directed.get(r.id)[1]
    cursor.linked.push(r)
  }
  for (const r of ledger) {
    const paid = r.linked.filter(child => child.role === '消费')
    const refundRows = r.linked.filter(child => child.role === '退款')
    const refunds = new Map()
    for (const child of refundRows) {
      const day = child.time.slice(0, 10)
      const k = day + '|' + child.amount
      const group = refunds.get(k) || new Map()
      group.set(child.source, (group.get(child.source) || 0) + 1)
      refunds.set(k, group)
    }
    if (refundRows.length > 1 && new Set(refundRows.map(x => x.source)).size > 1) r.flags.push('退款跨来源，需核对到账笔数')
    const actual = [...refunds].reduce((sum, [k, group]) => sum + Number(k.split('|')[1]) * Math.max(...group.values()), 0)
    const full = [r, ...paid].some(x => /已全额退款/.test(x.state))
    r.refund = full ? r.amount : Math.min(r.amount, actual)
    if (actual > r.amount) r.flags.push('退款合计超过支付额')
    if (full && !refundRows.length) r.flags.push('缺少退款到账流水')
    if (r.role === '退款' && !r.parent) r.flags.push('未找到原订单')
    r.net = r.role === '消费' ? r.amount - r.refund : 0
    const guessed = [r, ...paid].map(category).find(x => x !== '待分类')
    r.merchantName = [r, ...paid].find(x => x.merchant && x.source !== '工商银行')?.merchant || ''
    const rule = r.merchantName && settings.merchants[merchantKey(r.merchantName)]
    const marked = rule && !rule.excluded.includes(r.id) ? rule.category : ''
    r.category = settings.categories[r.id] || marked || guessed || '待分类'
  }
}

const roots = () => ledger.filter(r => !r.parent)
const spend = () => roots().filter(r => r.role === '消费')
const inMonth = (r) => r.time.startsWith(month())
const needsReview = (r) => r.flags.length || r.role === '待判定' || (r.role === '消费' && r.category === '待分类')
const card = (label, value, hint) => '<div class="card"><small>' + label + '</small><strong>' + value + '</strong><small>' + hint + '</small></div>'
const panel = (title, body) => '<section class="panel"><h2>' + title + '</h2>' + body + '</section>'
function table(headers, rows, extra = '') {
  return '<div class="scroll"><table class="' + extra + '"><thead><tr>' + headers.map(x => '<th scope="col">' + x + '</th>').join('') +
    '</tr></thead><tbody>' + rows.join('') + '</tbody></table></div>'
}
const tr = (...cells) => '<tr>' + cells.map(x => '<td>' + x + '</td>').join('') + '</tr>'

function stats() {
  const monthly = spend().filter(inMonth)
  const year = month().slice(0, 4)
  const annual = spend().filter(r => r.time.startsWith(year))
  const repaid = roots().filter(r => inMonth(r) && r.role === '还款')
  const months = Array.from({length: 12}, (_, i) => year + '-' + String(i + 1).padStart(2, '0'))
  const series = months.map(m => total(annual.filter(r => r.time.startsWith(m))))
  const max = Math.max(...series, 1)
  const coverage = source.reports.filter(r => r.start.slice(0, 7) <= month() && r.end.slice(0, 7) >= month()).map(r => r.source)
  const missing = platforms.filter(x => !coverage.includes(x))
  return '<div class="cards">' + card('本月净消费', monthly.length ? money(total(monthly)) : '—', monthly.length + ' 笔消费订单') +
    card('年度净消费', annual.length ? money(total(annual)) : '—', year + ' 年已导入记录') +
    card('本月已还', repaid.length ? money(total(repaid, 'amount')) : '—', '按已识别的还款流水') +
    card('本月待核对', String(roots().filter(r => inMonth(r) && needsReview(r)).length), '统计可能随核对更新') + '</div>' +
    (missing.length ? '<p class="warning">所选月缺少这些来源的流水覆盖迹象：' + missing.join('、') + '。</p>' : '') +
    panel('分类汇总', table(['分类', '本月净消费', '年度净消费'], categories.map(c => tr(c, money(total(monthly.filter(r => r.category === c))), money(total(annual.filter(r => r.category === c))))))) +
    panel('月度总消费', table(['月份', '净消费', '年度月度对比'], months.map((m, i) =>
      tr('<button data-month="' + m + '">' + m + '</button>', annual.some(r => r.time.startsWith(m)) ? money(series[i]) : '—',
        '<meter aria-label="' + m + ' 净消费" min="0" max="' + max + '" value="' + series[i] + '"></meter>'))))
}

function calendar() {
  const [year, m] = month().split('-').map(Number)
  const days = Array.from({length: new Date(year, m, 0).getDate()}, (_, i) => i + 1)
  const monthly = spend().filter(inMonth)
  return panel('按分类查看每日净消费', '<p class="muted">横向滚动查看整月。选择有记录的日期可查看对应明细。</p>' +
    table(['分类', ...days.map(d => d + ' 日')], categories.map(c => '<tr><th scope="row">' + c + '</th>' + days.map(d => {
      const day = String(d).padStart(2, '0')
      const rows = monthly.filter(r => r.category === c && r.time.slice(8, 10) === day)
      return '<td>' + (rows.length ? '<button data-day="' + day + '" data-category="' + c + '">' + money(total(rows)) + '</button>' : '—') + '</td>'
    }).join('') + '</tr>'), 'calendar'))
}

const label = (r) => r.title || r.merchant || r.kind || r.source
function detailTable(rows) {
  return table(['时间', '交易', '分类 / 状态', '实付 / 流水金额', '退款', '净消费', '核对'], rows.map(r =>
    tr(esc(r.time), esc(label(r)) + '<br><small>' + esc([...new Set([r.source, ...r.linked.map(x => x.source)])].join(' · ')) + '</small>',
      r.role === '消费' ? esc(r.category) : r.role, money(r.amount), r.refund ? money(r.refund) : '—',
      r.role === '消费' ? money(r.net) : '—', '<button data-id="' + r.id + '">' + (needsReview(r) ? '待核对' : '查看') + '</button>')))
}
function details() {
  const rows = roots().filter(r => inMonth(r) &&
    (!filter.category || r.role === '消费' && r.category === filter.category) &&
    (!filter.source || [r, ...r.linked].some(x => x.source === filter.source)) &&
    (!filter.day || r.time.slice(8, 10) === filter.day) &&
    (!filter.query || [r, ...r.linked].some(x => [label(x), x.tx, x.order].join(' ').toLowerCase().includes(filter.query.toLowerCase()))) &&
    (filter.state === '全部流水' || (filter.state === '待核对' ? needsReview(r) : r.role === '消费')))
    .sort((a, b) => b.time.localeCompare(a.time))
  const select = (name, caption, choices, defaultLabel) => '<label>' + caption + '<select data-filter="' + name + '">' +
    '<option value="">' + defaultLabel + '</option>' + choices.map(x => option(x, filter[name])).join('') + '</select></label>'
  return panel('归并后的账单', '<form id="filters" class="filters">' +
    select('category', '分类', categories, '全部分类') + select('source', '来源', platforms, '全部来源') +
    select('state', '范围', ['待核对', '全部流水'], '消费订单') +
    '<label>搜索<input name="query" value="' + esc(filter.query) + '" placeholder="商品、商户、订单号"></label><button>搜索</button>' +
    (filter.day ? '<button type="button" id="clear-day">清除 ' + filter.day + ' 日筛选</button>' : '') + '</form>' +
    '<div class="actions"><button id="merchants">管理商户分类</button></div>' +
    '<p class="muted">共 ' + rows.length + ' 条。本人划转、收入和还款可在「全部流水」查看。</p>' +
    (rows.length ? detailTable(rows.slice(0, limit)) : '<p>当前筛选没有记录。</p>') +
    (rows.length > limit ? '<button id="more">再显示 100 条</button>' : ''))
}

function manageMerchants(query = '', sortBy = 'count', hideMarked = false) {
  const groups = new Map()
  for (const row of spend()) {
    if (!row.merchantName) continue
    const key = merchantKey(row.merchantName)
    if (!groups.has(key)) groups.set(key, {name: row.merchantName, rows: [], amount: 0})
    const group = groups.get(key)
    group.rows.push(row)
    group.amount += row.amount
  }
  const ranked = [...groups.entries()]
    .filter(([key, group]) => (!hideMarked || !settings.merchants[key]) &&
      normalizeMerchant(group.name).includes(normalizeMerchant(query)))
    .sort(([, a], [, b]) => (sortBy === 'amount' ? b.amount - a.amount : b.rows.length - a.rows.length) ||
      a.name.localeCompare(b.name, 'zh-CN'))
  $('dialog-content').innerHTML = '<h2 id="dialog-title">商户分类</h2>' +
    '<p class="muted">统计全部已导入月份中有商户名称的归并订单。订单总额为退款前实付金额之和。</p>' +
    '<form id="merchant-search" class="filters"><label>商户名称<input name="query" value="' +
    esc(query) + '"></label><label>排序方式<select name="sort"><option value="count">订单数从多到少</option>' +
    '<option value="amount"' + (sortBy === 'amount' ? ' selected' : '') +
    '>订单总额从高到低</option></select></label><button>搜索</button>' +
    '<button type="button" id="toggle-marked" aria-pressed="' + hideMarked + '">' +
    (hideMarked ? '显示已标记' : '隐藏已标记') + '</button></form>' +
    (ranked.length ? table(['商户', '订单数', '订单总额', '分类标记', '备注'], ranked.map(([key, group], index) => {
      const marked = settings.merchants[key]?.category
      return tr('<button data-merchant-detail="' + index + '">' + esc(group.name) + '</button>',
        group.rows.length, money(group.amount), '<select data-merchant="' + index +
        '" aria-label="' + esc(group.name + '的分类标记') + '"><option value="">未标记</option>' +
        categories.map(c => option(c, marked)).join('') + '</select>',
        '<button data-merchant-note="' + index + '" aria-label="' + esc(group.name + '的备注') + '">' +
        (settings.merchantNotes[key] ? '编辑备注' : '添加备注') + '</button>')
    })) : '<p>' + (hideMarked ? '没有匹配的未标记商户。' : '没有匹配的商户。') + '</p>')
  const search = $('merchant-search')
  const applySearch = (focus) => {
    manageMerchants(search.elements.query.value.trim(), search.elements.sort.value, hideMarked)
    $('merchant-search').elements[focus].focus()
  }
  search.onsubmit = e => { e.preventDefault(); applySearch('query') }
  search.elements.sort.onchange = () => applySearch('sort')
  $('toggle-marked').onclick = () => {
    manageMerchants(search.elements.query.value.trim(), search.elements.sort.value, !hideMarked)
    $('toggle-marked').focus()
  }
  for (const button of $('dialog-content').querySelectorAll('[data-merchant-detail]')) {
    button.onclick = () => {
      const [key] = ranked[Number(button.dataset.merchantDetail)]
      showMerchant(key, () => {
        manageMerchants(query, sortBy, hideMarked)
        $('dialog-content').querySelector('[data-merchant-detail="' + button.dataset.merchantDetail + '"]')?.focus()
      })
    }
  }
  for (const button of $('dialog-content').querySelectorAll('[data-merchant-note]')) {
    button.onclick = () => {
      const [key, group] = ranked[Number(button.dataset.merchantNote)]
      editMerchantNote(key, group.name, () => {
        manageMerchants(query, sortBy, hideMarked)
        $('dialog-content').querySelector('[data-merchant-note="' + button.dataset.merchantNote + '"]')?.focus()
      })
    }
  }
  for (const select of $('dialog-content').querySelectorAll('[data-merchant]')) {
    select.onchange = () => {
      const [key, group] = ranked[Number(select.dataset.merchant)]
      const category = select.value
      if (!category) {
        delete settings.merchants[key]
      } else {
        const sync = window.confirm('将「' + group.name + '」已有的 ' + group.rows.length +
          ' 笔订单同步改为「' + category + '」？确定会覆盖这些订单的手动分类，取消则保留已有订单分类，仅将标记用于新订单。')
        const previous = new Set(settings.merchants[key]?.excluded || [])
        for (const row of group.rows) {
          if (sync) {
            delete settings.categories[row.id]
            previous.delete(row.id)
          } else {
            previous.add(row.id)
            settings.categories[row.id] = row.category
          }
        }
        settings.merchants[key] = {category, excluded: [...previous]}
      }
      save()
      manageMerchants(query, sortBy, hideMarked)
      const next = $('dialog-content').querySelector('[data-merchant="' + select.dataset.merchant + '"]') || $('toggle-marked')
      next.focus()
    }
  }
  if (!$('dialog').open) $('dialog').showModal()
}

function showMerchant(key, back, limit = 100) {
  const rows = spend().filter(row => row.merchantName && merchantKey(row.merchantName) === key)
    .sort((a, b) => b.time.localeCompare(a.time))
  const name = rows[0]?.merchantName || '商户'
  const note = settings.merchantNotes[key]
  $('dialog-content').innerHTML = '<div class="actions"><button id="merchant-back">返回商户列表</button>' +
    '<button id="edit-merchant-note">' + (note ? '编辑备注' : '添加备注') + '</button></div>' +
    '<h2 id="dialog-title">' + esc(name) + '</h2><p class="muted">全部已导入月份 · ' + rows.length + ' 笔归并订单 · ' +
    esc(settings.merchants[key]?.category || '未标记') + '</p>' +
    (note ? '<p class="merchant-note"><strong>商户备注</strong><br>' + esc(note) + '</p>' : '') + '<div class="cards">' +
    card('订单总额', money(total(rows, 'amount')), '退款前实付') +
    card('累计退款', money(total(rows, 'refund')), '已绑定的退款') +
    card('净消费', money(total(rows)), '扣除退款') + '</div>' +
    (rows.length ? detailTable(rows.slice(0, limit)) : '<p>该商户当前没有消费订单。</p>') +
    (rows.length > limit ? '<button id="merchant-more">再显示 100 条</button>' : '')
  $('merchant-back').onclick = back
  $('merchant-back').focus()
  $('edit-merchant-note').onclick = () => editMerchantNote(key, name, () => {
    showMerchant(key, back, limit)
    $('edit-merchant-note').focus()
  })
  if ($('merchant-more')) $('merchant-more').onclick = () => showMerchant(key, back, limit + 100)
  for (const button of $('dialog-content').querySelectorAll('[data-id]')) {
    button.onclick = e => {
      e.stopPropagation()
      inspect(button.dataset.id, () => {
        showMerchant(key, back, limit)
        $('dialog-content').querySelector('[data-id="' + button.dataset.id + '"]')?.focus()
      })
    }
  }
}

function editMerchantNote(key, name, back) {
  $('dialog-content').innerHTML = '<h2 id="dialog-title">' + esc(name) + ' · 商户备注</h2>' +
    '<form id="merchant-note-form"><label for="merchant-note-text">备注（最多 500 字，留空后保存可清除）</label>' +
    '<textarea id="merchant-note-text" name="note" rows="5" maxlength="500">' +
    esc(settings.merchantNotes[key] || '') + '</textarea><div class="actions">' +
    '<button type="submit" class="primary">保存备注</button><button type="button" id="cancel-note">取消</button></div></form>'
  $('merchant-note-text').focus()
  $('cancel-note').onclick = back
  $('merchant-note-form').onsubmit = e => {
    e.preventDefault()
    const note = e.target.elements.note.value.trim()
    if (note) settings.merchantNotes[key] = note
    else delete settings.merchantNotes[key]
    save()
    back()
  }
}


function debt() {
  const rows = roots().filter(r => r.role === '还款')
  const monthly = rows.filter(inMonth).sort((a, b) => b.time.localeCompare(a.time))
  const snapshot = settings.debts[month()]
  return '<div class="cards">' +
    card('总待还', snapshot ? money(snapshot.remaining) : '待补录', '所选月的手工余额快照') +
    card('本月待还', snapshot ? money(snapshot.due) : '待补录', '所选月应还未还金额') +
    card('本月已还', monthly.length ? money(total(monthly, 'amount')) : '—', '已识别成功流水') +
    card('累计已还', rows.length ? money(total(rows, 'amount')) : '—', '所有已导入月份') + '</div>' +
    panel('补录 ' + month() + ' 还款余额', '<p class="muted">按平台账单汇总后录入金额。补录值是快照，更新账单后需同步更新。</p>' +
      '<form id="debt-form" class="filters"><label>总待还（元）<input name="remaining" type="number" min="0" step=".01" required value="' +
      (snapshot ? snapshot.remaining / 100 : '') + '"></label><label>本月待还（元）<input name="due" type="number" min="0" step=".01" required value="' +
      (snapshot ? snapshot.due / 100 : '') + '"></label><button class="primary">保存余额</button></form><p id="debt-error" class="warning" role="alert"></p>') +
    panel('本月还款流水', monthly.length ? detailTable(monthly) : '<p>该月未识别到还款流水。</p>')
}

function imports() {
  const disabled = importing || !ready ? ' disabled' : ''
  return panel('本地账单文件', '<p class="muted">选择支付宝 CSV 或微信 XLSX，可一次选择多个文件。日期列显示读取到的首末交易。</p>' +
    '<div class="actions"><label>导入账单 <input type="file" id="import-files" accept=".csv,.xlsx" multiple' + disabled + '></label>' +
    '<button id="export"' + disabled + '>导出核对设置</button><button id="export-ledger"' + disabled + '>导出完整账本</button>' +
    '<label>恢复核对设置 <input type="file" id="restore" accept=".json,application/json"' + disabled + '></label>' +
    '<label>导入历史数据或账本备份 <input type="file" id="restore-ledger" accept=".json,application/json"' + disabled + '></label></div>') +
    panel('文件读取结果', table(['来源', '文件', '交易范围', '已读取', '重复流水', '跳过', '结果'], source.reports.map(r =>
      tr(esc(r.source), esc(r.file), r.start ? r.start + ' 至 ' + r.end : '—', r.count, r.duplicates, r.skipped,
        r.error ? '读取失败（' + esc(r.error) + '）' : r.skipped ? '部分记录需检查' : '已读取'))))
}

function inspect(id, back) {
  const row = ledger.find(r => r.id === id)
  if (!row) return
  const options = ['消费', '退款', '还款', '资金划转', '收入', '待判定', '排除']
  const raw = (r) => '<details><summary>' + esc(r.source + ' · ' + r.time + ' · ' + money(r.amount) + (r.reason ? ' · ' + r.reason : '')) +
    '</summary><p>' + esc([r.kind, r.state, r.merchant, r.title, r.method, r.note].filter(Boolean).join(' · ')) +
    '</p><p>交易单号 ' + esc(r.tx || '—') + '<br>商家单号 ' + esc(r.order || '—') +
    '<br>核对编号 <code>' + r.id + '</code></p>' +
    (r.parent ? '<button data-split="' + r.id + '">解除关联</button>' : '') + '</details>'
  const candidates = spend().filter(p => p.id !== id && (row.role === '退款'
    ? p.time <= row.time && p.amount >= row.amount
    : p.amount === row.amount && Math.abs(seconds(p) - seconds(row)) <= 300))
    .sort((a, b) => Math.abs(seconds(a) - seconds(row)) - Math.abs(seconds(b) - seconds(row))).slice(0, 12)
  $('dialog-content').innerHTML = (back ? '<button id="order-back">返回商户详情</button>' : '') +
    '<h2 id="dialog-title">' + esc(label(row)) + '</h2>' +
    '<p>' + esc(row.role) + ' · ' + money(row.amount) + (row.refund ? ' · 累计退款 ' + money(row.refund) : '') + '</p>' +
    (row.flags.length ? '<p class="warning">' + esc(row.flags.join('；')) + '</p>' : '') +
    '<div class="filters"><label>交易用途<select id="role"><option value="">自动识别（当前 ' + esc(row.role) + '）</option>' +
    options.map(x => option(x, settings.roles[id])).join('') + '</select></label>' +
    (row.role === '消费' ? '<label>消费分类<select id="category"><option value="">自动分类（当前 ' + esc(row.category) + '）</option>' +
    categories.map(x => option(x, settings.categories[id])).join('') + '</select></label>' : '') + '</div>' +
    '<h3>来源流水</h3>' + [row, ...row.linked].map(raw).join('') +
    '<form class="filters" id="link-form"><label>关联到消费订单<input name="target" required list="candidates" placeholder="选择候选或粘贴核对编号"></label>' +
    '<datalist id="candidates">' + candidates.map(p => '<option value="' + p.id + '">' + esc(p.source + ' ' + p.time + ' ' + money(p.amount) + ' ' + label(p)) + '</option>').join('') +
    '</datalist><button>确认关联</button></form><p id="link-error" class="warning" role="alert"></p>'
  const modal = $('dialog')
  if (!modal.open) modal.showModal()
  if (back) {
    $('order-back').onclick = back
    $('order-back').focus()
  }
  for (const button of $('dialog-content').querySelectorAll('[data-split]')) {
    button.onclick = e => {
      e.stopPropagation()
      settings.links[button.dataset.split] = null
      save()
      inspect(id, back)
    }
  }
  $('role').addEventListener('change', e => {
    if (e.target.value) settings.roles[id] = e.target.value
    else delete settings.roles[id]
    save()
    inspect(id, back)
  })
  $('category')?.addEventListener('change', e => {
    if (e.target.value) settings.categories[id] = e.target.value
    else delete settings.categories[id]
    save()
    inspect(id, back)
  })
  $('link-form').addEventListener('submit', e => {
    e.preventDefault()
    const target = spend().find(p => p.id === new FormData(e.target).get('target').trim())
    if (!['消费', '退款'].includes(row.role) || !target || target.id === id || target.linked.some(p => p.id === id) ||
        (row.role === '退款' && (target.amount < row.amount || target.time > row.time))) {
      $('link-error').textContent = '请选择有效的消费订单。退款应晚于原支付，且金额不超过原支付。'
      return
    }
    settings.links[id] = target.id
    save()
    inspect(target.id, back)
  })
}

function render() {
  $('title').textContent = names[page]
  document.querySelectorAll('nav button').forEach(button => {
    if (button.dataset.page === page) button.setAttribute('aria-current', 'page')
    else button.removeAttribute('aria-current')
  })
  $('view').innerHTML = {stats, calendar, details, debt, imports}[page]()
}
function setMonth(value) {
  $('month').value = value
  filter.day = ''
  limit = 100
  render()
}
function shiftMonth(step) {
  const [year, m] = month().split('-').map(Number)
  const date = new Date(Date.UTC(year, m - 1 + step, 1))
  setMonth(date.toISOString().slice(0, 7))
}
$('prev').onclick = () => shiftMonth(-1)
$('next').onclick = () => shiftMonth(1)
$('month').onchange = () => setMonth(month())

async function reload() {
  try {
    const data = await request('load_ledger')
    if (!ready) settings = {...emptySettings(), ...data.settings}
    source = data
    ready = true
    rebuild()
    const errors = source.reports.filter(r => r.error || r.skipped).length
    $('status').textContent = '已读取 ' + source.reports.length + ' 份文件 · ' + ledger.length + ' 条去重流水' +
      (errors ? ' · ' + errors + ' 份文件有导入异常，请查看导入核对' : '')
  } catch {
    $('status').textContent = '无法读取本机账本数据库，请关闭程序后重新打开。'
  }
  render()
}

function download(name, data) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], {type: 'application/json'}))
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

async function importFiles(files) {
  importing = true
  render()
  const summary = []
  for (const file of files) {
    $('status').textContent = '正在导入 ' + file.name + '…'
    try {
      const batch = await parseLedger(file)
      const added = await request('import_ledger', {batch})
      summary.push(file.name + (added ? ' 已读取 ' + batch.rows.length + ' 条' : ' 已导入过'))
    } catch (error) {
      summary.push(file.name + ' 读取失败（' + (error.message || '未知错误') + '）')
    }
  }
  importing = false
  await reload()
  $('status').textContent += '。' + summary.join('；')
}

document.addEventListener('click', e => {
  const button = e.target.closest('button')
  if (!button) return
  const {page: nextPage, id, day, category, month: nextMonth} = button.dataset
  if (nextPage) { page = nextPage; render() }
  if (id) inspect(id)
  if (day) {
    page = 'details'
    filter = {category, source: '', state: '', day, query: ''}
    limit = 100
    render()
  }
  if (nextMonth) setMonth(nextMonth)
  if (button.id === 'clear-day') { filter.day = ''; render() }
  if (button.id === 'more') { limit += 100; render() }
  if (button.id === 'merchants') manageMerchants()
  if (button.id === 'export') download('账本核对设置-' + current + '.json', settings)
  if (button.id === 'export-ledger') download('完整账本-' + current + '.json', {rows: source.rows, reports: source.reports, settings})
})
document.addEventListener('submit', e => {
  if (e.target.id === 'filters') {
    e.preventDefault()
    filter.query = new FormData(e.target).get('query').trim()
    limit = 100
    render()
  }
  if (e.target.id === 'debt-form') {
    e.preventDefault()
    const data = new FormData(e.target)
    const remaining = Math.round(Number(data.get('remaining')) * 100)
    const due = Math.round(Number(data.get('due')) * 100)
    if (!Number.isSafeInteger(remaining) || !Number.isSafeInteger(due) || due > remaining || due < 0) {
      $('debt-error').textContent = '本月待还应在 0 元至总待还之间。'
      return
    }
    settings.debts[month()] = {remaining, due}
    save()
  }
})
document.addEventListener('change', async e => {
  const name = e.target.dataset.filter
  if (name) {
    filter[name] = e.target.value
    filter.query = new FormData(e.target.form).get('query').trim()
    limit = 100
    render()
    document.querySelector('[data-filter="' + name + '"]')?.focus()
  }
  if (e.target.id === 'import-files' && !importing && e.target.files.length) {
    await importFiles([...e.target.files])
  }
  if (['restore', 'restore-ledger'].includes(e.target.id) && e.target.files[0] && !importing) {
    const full = e.target.id === 'restore-ledger'
    try {
      const file = e.target.files[0]
      if (file.size > 96_000_000) throw new Error('文件超过大小限制')
      const bytes = await file.arrayBuffer()
      const content = JSON.parse(new TextDecoder().decode(bytes))
      const data = full ? content.settings : content
      const id = /^[a-f0-9]{24}$/
      const role = ['消费', '退款', '还款', '资金划转', '收入', '待判定', '排除']
      const validators = {
        categories: (k, v) => id.test(k) && categories.includes(v),
        roles: (k, v) => id.test(k) && role.includes(v),
        links: (k, v) => id.test(k) && (v === null || typeof v === 'string' && id.test(v)),
        merchants: (k, v) => k.startsWith('商户:') && v && categories.includes(v.category) &&
          Array.isArray(v.excluded) && v.excluded.every(x => typeof x === 'string' && id.test(x)),
        merchantNotes: (k, v) => k.startsWith('商户:') && typeof v === 'string' && v.length <= 500,
        debts: (k, v) => /^\d{4}-(0[1-9]|1[0-2])$/.test(k) && v &&
          Number.isSafeInteger(v.remaining) && Number.isSafeInteger(v.due) && v.due >= 0 && v.due <= v.remaining,
      }
      if (!full || data !== undefined) {
        if (data && !Object.hasOwn(data, 'merchants')) data.merchants = {}
        if (data && !Object.hasOwn(data, 'merchantNotes')) data.merchantNotes = {}
        for (const [key, valid] of Object.entries(validators)) {
          if (!data?.[key] || typeof data[key] !== 'object' || Array.isArray(data[key]) ||
              !Object.entries(data[key]).every(([k, v]) => valid(k, v))) throw new Error('设置格式不符')
        }
      }
      if (full) {
        if (!Array.isArray(content.rows) || !Array.isArray(content.reports)) throw new Error('账本格式不符')
        if (!window.confirm('导入账本流水和核对设置？相同设置将使用备份值。')) return
        if (!content.rows.length && !content.reports.length && !data) return
        const merged = {...settings}
        if (data) {
          for (const key of Object.keys(validators)) merged[key] = {...settings[key], ...data[key]}
        }
        importing = true
        render()
        try {
          await pendingSave
          await request('import_ledger', {batch: {
            digest: await digest(bytes), modified: 0, rows: content.rows, reports: content.reports,
            settings: data ? merged : null,
          }})
          if (data) settings = merged
        } finally {
          importing = false
          render()
        }
        await reload()
      } else if (window.confirm('用备份合并本机设置？相同项目将使用备份值。')) {
        for (const key of Object.keys(validators)) settings[key] = {...settings[key], ...data[key]}
        save()
      }
    } catch (error) { $('status').textContent = '无法导入备份：' + (error.message || '格式不符') }
    e.target.value = ''
  }
})
render()
reload()
