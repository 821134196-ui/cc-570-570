#!/usr/bin/env bash
# 端到端演示:多次降价、重复回执、多规则版本、各类拒绝场景
# 用法: ./scripts/demo.sh  (需服务已在 8080 端口运行)
set -euo pipefail
BASE="${BASE:-http://localhost:8080/api}"
J() { python3 -c "import sys,json;d=json.load(sys.stdin);print(json.dumps(d,ensure_ascii=False,indent=1))"; }
step() { echo; echo "════════ $* ════════"; }

step "0. 重置演示数据"
curl -s -X POST $BASE/admin/reset | J

step "1. 价格未降时申请 → 拒绝(无差价)"
curl -s -X POST $BASE/pp/preview -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":1}' | J

step "2. 第一次降价:2999 → 2799"
curl -s -X POST $BASE/admin/prices -H 'Content-Type: application/json' \
  -d '{"sku_id":1,"price_cents":279900}' | J
echo "--- 试算(逐项计算依据) ---"
curl -s -X POST $BASE/pp/preview -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":1}' | J
echo "--- 提交申请 ---"
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":1}' | J

step "3. 重复提交同一申请 → 拒绝(重复申请)"
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":1}' | J

step "4. 店员复核通过 → 退款 ¥130.00"
curl -s -X POST $BASE/applications/1/approve -H 'Content-Type: application/json' \
  -d '{"reviewer":"店员小王"}' | J

step "5. 支付网关重复推送同一回执 → 忽略,不多退"
RCPT=$(curl -s $BASE/refunds | python3 -c "import sys,json;print(json.load(sys.stdin)['refunds'][0]['refund']['receipt_no'])")
echo "回执号: $RCPT"
curl -s -X POST $BASE/mock/receipts/$RCPT/resend | J
curl -s -X POST $BASE/mock/receipts/$RCPT/resend | J
echo "--- 该明细累计已退(应仍为 130.00) ---"
curl -s $BASE/orders/1 | python3 -c "import sys,json;d=json.load(sys.stdin);print('refunded_cents =',d['items'][0]['refunded_cents'])"

step "6. 第二次降价:2799 → 2599 → 只退新增差价 ¥180.00"
curl -s -X POST $BASE/admin/prices -H 'Content-Type: application/json' \
  -d '{"sku_id":1,"price_cents":259900}' | J
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":1}' | python3 -c "import sys,json;a=json.load(sys.stdin)['application'];print('本次应退:',a['this_refund_cents'],'累计可退:',a['cumulative_refundable_cents'],'此前已退:',a['already_refunded_cents'],'规则版本:v'+str(a['rule_version']))"
curl -s -X POST $BASE/applications/2/approve -H 'Content-Type: application/json' -d '{}' | J

step "7. 发布新规则版本 v2(八五折)→ 同价不同规则,结果不同"
curl -s -X POST $BASE/admin/rules -H 'Content-Type: application/json' \
  -d '{"name":"店庆八五折","rule_type":"percent_off","percent_off":85}' | J
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":1}' | python3 -c "import sys,json;a=json.load(sys.stdin)['application'];print('本次应退:',a['this_refund_cents'],'规则版本:v'+str(a['rule_version']))"
curl -s -X POST $BASE/applications/3/approve -H 'Content-Type: application/json' -d '{}' | J
echo "--- 三笔退款的规则版本快照 ---"
curl -s $BASE/refunds | python3 -c "
import sys,json
for r in json.load(sys.stdin)['refunds']:
    f=r['refund'];print(f['refund_no'],'金额:',f['amount_cents'],'规则版本:v'+str(f['rule_version']),'状态:',f['status'])"

step "8. 拒绝场景"
echo "--- 8a. 已退货订单(订单2) ---"
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":2,"sku_id":3}' | J
echo "--- 8b. 超过价保期限(订单3,李四) ---"
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":3,"sku_id":2}' | J
echo "--- 8c. 相同商品不同规格(订单1的手机申请白色/256G) ---"
curl -s -X POST $BASE/pp/apply -H 'Content-Type: application/json' \
  -d '{"order_item_id":1,"sku_id":2}' | J

step "9. 重复审核同一申请 → 拒绝(状态机保护)"
curl -s -X POST $BASE/applications/1/approve -H 'Content-Type: application/json' -d '{}' | J

step "10. 回执投递记录(重复送达可见)"
curl -s $BASE/receipts | python3 -c "
import sys,json
for r in json.load(sys.stdin)['receipts']:
    print(r['receipt_no'],r['refund_no'],'重复' if r['duplicated'] else '首次')"
echo; echo "演示完成 ✔"
