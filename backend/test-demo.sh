#!/usr/bin/env bash
# 端到端演示场景验证：多次降价、增量退款、券不重复抵扣、幂等防重、拒绝原因
set -u
BASE=http://localhost:3000/api
PASS=0; FAIL=0
ck() { # ck <描述> <实际值> <期望值>
  if [ "$2" == "$3" ]; then PASS=$((PASS+1)); echo "  ✅ $1：$2";
  else FAIL=$((FAIL+1)); echo "  ❌ $1：期望[$3] 实际[$2]"; fi
}
J() { python3 -c "import sys,json;d=json.load(sys.stdin);print(eval(sys.argv[1]))" "$1" 2>/dev/null; }

echo "=== 0. 重置演示数据 ==="
curl -s -X POST $BASE/demo/reset >/dev/null

echo "=== 1. 第一次降价：耳机标准版 500→450 (v1 直降) ==="
curl -s -X POST $BASE/demo/price-drop -H 'Content-Type: application/json' \
  -d '{"sku_id":1,"activity_price":45000,"rule_version_id":1}' >/dev/null

echo "--- 1a. O1001(实付450=500-券50) 应无差价（券不重复抵扣）"
B=$(curl -s "$BASE/price-protection/preview?order_item_id=1")
ck "总差价=0" "$(echo "$B" | J 'd["breakdown"]["total_diff"]')" "0"
ck "资格=拒绝" "$(echo "$B" | J 'd["breakdown"]["reject_reason"]')" "当前应付 45000 分不低于原实付 45000 分，无差价可退"

echo "--- 1b. O1004(实付500,无券) 差价 50 元"
B=$(curl -s "$BASE/price-protection/preview?order_item_id=4")
ck "总差价=5000分" "$(echo "$B" | J 'd["breakdown"]["total_diff"]')" "5000"
ck "本次应退=5000分" "$(echo "$B" | J 'd["breakdown"]["this_refund"]')" "5000"

echo "--- 1c. 提交申请 + 重复提交（同一幂等键）"
A1=$(curl -s -X POST $BASE/applications -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-key-1' -d '{"order_item_id":4}')
APP1=$(echo "$A1" | J 'd["application"]["id"]')
ck "申请已建(pending)" "$(echo "$A1" | J 'd["application"]["status"]')" "pending"
A2=$(curl -s -X POST $BASE/applications -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-key-1' -d '{"order_item_id":4}')
ck "重复提交被去重" "$(echo "$A2" | J 'd["deduplicated"]')" "True"
ck "返回同一申请" "$(echo "$A2" | J 'd["application"]["id"]')" "$APP1"
CNT=$(curl -s "$BASE/applications?customer=张三" | J 'len([a for a in d["applications"] if a["application"]["order_item_id"]==4])')
ck "该明细只有1条申请" "$CNT" "1"

echo "--- 1d. 店员复核通过 → 退款 50 元"
R=$(curl -s -X POST $BASE/staff/applications/$APP1/approve)
ck "退款金额=5000" "$(echo "$R" | J 'd["refund"]["amount"]')" "5000"
ck "退款已到账" "$(echo "$R" | J 'd["refund"]["status"]')" "succeeded"
RCPT1=$(echo "$R" | J 'd["receipt"]["receipt_no"]')
ck "申请状态=refunded" "$(echo "$R" | J 'd["application"]["status"]')" "refunded"

echo "--- 1e. 重复审核同一申请 → 拦截"
R2=$(curl -s -o /dev/null -w '%{http_code}' -X POST $BASE/staff/applications/$APP1/approve)
ck "重复审核返回409" "$R2" "409"

echo "--- 1f. 模拟支付回执重复送达 → 不多退"
D=$(curl -s -X POST $BASE/demo/receipts/$RCPT1/resend)
ck "回执去重标记" "$(echo "$D" | J 'd["result"]["deduplicated"]')" "True"
SUM=$(curl -s $BASE/staff/refunds | J 'sum(r["refund"]["amount"] for r in d["refunds"] if r["refund"]["status"]=="succeeded")')
ck "累计退款仍=5000" "$SUM" "5000"

echo "=== 2. 第二次降价：450→400 (v2 满300减30) → 当前应付 370 ==="
curl -s -X POST $BASE/demo/price-drop -H 'Content-Type: application/json' \
  -d '{"sku_id":1,"activity_price":40000,"rule_version_id":2}' >/dev/null
B=$(curl -s "$BASE/price-protection/preview?order_item_id=4")
ck "规则优惠=3000" "$(echo "$B" | J 'd["breakdown"]["rule_discount"]')" "3000"
ck "当前应付=37000" "$(echo "$B" | J 'd["breakdown"]["current_payable"]')" "37000"
ck "总差价=13000" "$(echo "$B" | J 'd["breakdown"]["total_diff"]')" "13000"
ck "已退=5000" "$(echo "$B" | J 'd["breakdown"]["already_refunded"]')" "5000"
ck "本次只退新增=8000" "$(echo "$B" | J 'd["breakdown"]["this_refund"]')" "8000"
ck "当前规则版本=v2" "$(echo "$B" | J 'd["breakdown"]["current_rule_version"]')" "v2"
A=$(curl -s -X POST $BASE/applications -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-key-2' -d '{"order_item_id":4}')
APP2=$(echo "$A" | J 'd["application"]["id"]')
R=$(curl -s -X POST $BASE/staff/applications/$APP2/approve)
ck "第二次退款=8000" "$(echo "$R" | J 'd["refund"]["amount"]')" "8000"

echo "=== 3. 第三次降价：400→380 (v3 全场9折) → 当前应付 342 ==="
curl -s -X POST $BASE/demo/price-drop -H 'Content-Type: application/json' \
  -d '{"sku_id":1,"activity_price":38000,"rule_version_id":3}' >/dev/null
B=$(curl -s "$BASE/price-protection/preview?order_item_id=4")
ck "当前应付=34200" "$(echo "$B" | J 'd["breakdown"]["current_payable"]')" "34200"
ck "本次只退新增=2800" "$(echo "$B" | J 'd["breakdown"]["this_refund"]')" "2800"
A=$(curl -s -X POST $BASE/applications -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-key-3' -d '{"order_item_id":4}')
APP3=$(echo "$A" | J 'd["application"]["id"]')
R=$(curl -s -X POST $BASE/staff/applications/$APP3/approve)
ck "第三次退款=2800" "$(echo "$R" | J 'd["refund"]["amount"]')" "2800"
SUM=$(curl -s $BASE/staff/refunds | J 'sum(r["refund"]["amount"] for r in d["refunds"] if r["refund"]["status"]=="succeeded")')
ck "三次累计=15800(≤实付50000)" "$SUM" "15800"

echo "=== 4. 拒绝场景 ==="
B=$(curl -s "$BASE/price-protection/preview?order_item_id=2")
ck "已退货拒绝" "$(echo "$B" | J 'd["breakdown"]["checks"][1]["passed"]')" "False"
B=$(curl -s "$BASE/price-protection/preview?order_item_id=3")
ck "超价保期拒绝" "$(echo "$B" | J 'd["breakdown"]["checks"][2]["passed"]')" "False"
B=$(curl -s "$BASE/price-protection/preview?order_item_id=1&claimed_sku_id=2")
ck "规格不符拒绝" "$(echo "$B" | J 'd["breakdown"]["checks"][0]["passed"]')" "False"
A=$(curl -s -X POST $BASE/applications -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-key-4' -d '{"order_item_id":1,"claimed_sku_id":2}')
ck "规格不符申请落库为rejected" "$(echo "$A" | J 'd["application"]["status"]')" "rejected"
ck "拒绝原因含规格说明" "$(echo "$A" | J '"规格不同" in d["application"]["reject_reason"] or "规格" in d["application"]["reject_reason"]')" "True"

echo "=== 5. 无新增差价：再次对 O1004 申请（差价已退完）==="
A=$(curl -s -X POST $BASE/applications -H 'Content-Type: application/json' -H 'Idempotency-Key: demo-key-5' -d '{"order_item_id":4}')
ck "状态=rejected" "$(echo "$A" | J 'd["application"]["status"]')" "rejected"
ck "原因=无新增差价" "$(echo "$A" | J '"无新增差价" in d["application"]["reject_reason"]')" "True"

echo "=== 6. 多件+券订单 O1005（键盘×2 实付580）在当前价格下 ==="
B=$(curl -s "$BASE/price-protection/preview?order_item_id=5")
ck "键盘未降价→差价为负(不可退)" "$(echo "$B" | J 'd["breakdown"]["total_diff"]')" "-2000"
ck "本次应退=0" "$(echo "$B" | J 'd["breakdown"]["this_refund"]')" "0"
ck "资格=不可申请" "$(echo "$B" | J 'd["breakdown"]["eligible"]')" "False"
curl -s -X POST $BASE/demo/price-drop -H 'Content-Type: application/json' \
  -d '{"sku_id":3,"activity_price":26000,"rule_version_id":1}' >/dev/null
B=$(curl -s "$BASE/price-protection/preview?order_item_id=5")
ck "降价后当前应付=52000" "$(echo "$B" | J 'd["breakdown"]["current_payable"]')" "52000"
ck "差价=6000(券2000不重复抵扣)" "$(echo "$B" | J 'd["breakdown"]["total_diff"]')" "6000"

echo "=== 7. 累计退款不得超原实付（钳制逻辑抽查）==="
B=$(curl -s "$BASE/price-protection/preview?order_item_id=4")
ck "可退上限=50000-15800=34200" "$(echo "$B" | J 'd["breakdown"]["refundable_cap"]')" "34200"

echo ""
echo "=================================="
echo "结果：通过 $PASS 项，失败 $FAIL 项"
[ $FAIL -eq 0 ] && echo "全部通过 🎉" || exit 1
