// 统一 API 封装；金额单位均为“分”
async function req(path, opts = {}) {
  const res = await fetch(`/api${path}`, {
    headers: { 'Content-Type': 'application/json', ...(opts.headers || {}) },
    ...opts,
  })
  const data = await res.json().catch(() => ({}))
  return { status: res.status, data }
}

export const api = {
  orders: (customer = '张三') => req(`/orders?customer=${encodeURIComponent(customer)}`),
  preview: (orderItemId, claimedSkuId) =>
    req(`/price-protection/preview?order_item_id=${orderItemId}` + (claimedSkuId ? `&claimed_sku_id=${claimedSkuId}` : '')),
  apply: (orderItemId, idempotencyKey, claimedSkuId) =>
    req('/applications', {
      method: 'POST',
      headers: { 'Idempotency-Key': idempotencyKey },
      body: JSON.stringify({ order_item_id: orderItemId, claimed_sku_id: claimedSkuId ?? null }),
    }),
  myApplications: (customer = '张三') => req(`/applications?customer=${encodeURIComponent(customer)}`),
  staffQueue: (status = 'pending') => req(`/staff/applications?status=${status}`),
  approve: (id) => req(`/staff/applications/${id}/approve`, { method: 'POST', body: '{}' }),
  reject: (id, reason) => req(`/staff/applications/${id}/reject`, { method: 'POST', body: JSON.stringify({ reason }) }),
  refunds: () => req('/staff/refunds'),
  meta: () => req('/meta'),
  priceDrop: (skuId, price, ruleVersionId) =>
    req('/demo/price-drop', { method: 'POST', body: JSON.stringify({ sku_id: skuId, activity_price: price, rule_version_id: ruleVersionId }) }),
  resendReceipt: (receiptNo) => req(`/demo/receipts/${receiptNo}/resend`, { method: 'POST', body: '{}' }),
  reset: () => req('/demo/reset', { method: 'POST', body: '{}' }),
  gatewayLedger: () => req('/mock-pay/refunds'),
}

export const fen = (cents) => (cents == null ? '-' : `¥${(cents / 100).toFixed(2)}`)
export const fmtTime = (iso) => (iso ? iso.replace('T', ' ').slice(0, 19) : '-')
