const BASE = '/api'

async function req(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  const data = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(data.error || `请求失败 (${res.status})`)
  return data
}

export const api = {
  get: (p) => req('GET', p),
  post: (p, b) => req('POST', p, b ?? {}),
}

export const fmt = (cents) =>
  cents == null ? '-' : '¥' + (cents / 100).toFixed(2)

export const fmtTime = (iso) => {
  if (!iso) return '-'
  const d = new Date(iso)
  return d.toLocaleString('zh-CN', { hour12: false })
}

export const STATUS_TEXT = {
  pending: '待审核',
  approved: '退款中',
  rejected: '已拒绝',
  refunded: '已退款',
  paid: '已支付',
  returned: '已退货',
  processing: '处理中',
  success: '已到账',
  failed: '失败',
}
