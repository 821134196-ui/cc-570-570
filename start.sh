#!/usr/bin/env bash
# 门店价保系统 — 一条命令启动前后端
set -euo pipefail
cd "$(dirname "$0")"

export PATH="$HOME/.cargo/bin:$PATH"

# 1. 构建前端(首次会自动安装依赖)
if [ ! -d frontend/node_modules ]; then
  echo "==> 安装前端依赖..."
  (cd frontend && npm install --no-fund --no-audit)
fi
echo "==> 构建前端..."
(cd frontend && npm run build)

# 2. 启动后端(自动建库、迁移、写入演示数据,并托管前端静态文件)
echo "==> 启动后端 http://localhost:8080 ..."
cd backend
export DATABASE_URL="${DATABASE_URL:-sqlite://priceprotect.db}"
export STATIC_DIR="${STATIC_DIR:-../frontend/dist}"
export BIND_ADDR="${BIND_ADDR:-0.0.0.0:8080}"
exec cargo run --release
