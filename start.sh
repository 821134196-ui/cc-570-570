#!/usr/bin/env bash
# 门店价保系统一键启动：构建前端 + 启动后端（含 SQLite 自动建表与演示数据）
set -euo pipefail
cd "$(dirname "$0")"

export PATH="$HOME/.cargo/bin:$PATH"

echo "==> [1/2] 构建前端 (Svelte + Vite)"
if [ ! -d frontend/node_modules ]; then
  (cd frontend && npm install)
fi
(cd frontend && npm run build)

echo "==> [2/2] 启动后端 (Rust + Axum + SQLx/SQLite)"
cd backend
echo ""
echo "    打开 http://localhost:3000  即可使用"
echo "    顾客页：申请价保、查看逐项计算依据"
echo "    店员页：复核、退款、重放回执"
echo "    演示控制台：模拟降价 / 切换规则版本 / 重置数据"
echo ""
exec cargo run ${1:-}
