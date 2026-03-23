# /project:status

检查当前项目状态，输出一份简报。不启动迭代循环。

执行以下操作：
1. 读取 `docs/progress.md`，输出当前模块状态和迭代轮次
2. 读取 `reports/blockers.md`，列出当前所有 BLOCKER
3. 运行 `cargo test --all 2>&1 | tail -5`，显示最新测试结果
4. 读取最新的 `reports/arbiter_*.md`，显示最后裁决

以清晰的中文摘要输出，不超过 30 行。
