#!/bin/bash
# post_write.sh — 每次 src/ 文件变更后自动触发质量门

CHANGED=$(git diff --name-only 2>/dev/null | grep '^src/')
CHANGED_STAGED=$(git diff --cached --name-only 2>/dev/null | grep '^src/')

if [ -z "$CHANGED" ] && [ -z "$CHANGED_STAGED" ]; then
  exit 0
fi

echo "[quality-gate] src/ 变更检测，运行 cargo check..."

cargo check 2>&1
if [ $? -ne 0 ]; then
  echo "BLOCKER[hook]: cargo check 失败" >> reports/blockers.md
  echo "[quality-gate] FAILED: cargo check"
  exit 1
fi

echo "[quality-gate] 通过"
exit 0
