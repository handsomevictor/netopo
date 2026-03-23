# /project:fix

针对 `reports/blockers.md` 中的问题进行定点修复，不走完整迭代流程。

执行步骤：
1. 读取 `reports/blockers.md`
2. 对每个 BLOCKER，判断责任 agent（代码问题 → developer_agent，测试问题 → tester_agent，文档问题 → doc_agent）
3. 启动对应 agent 处理该 BLOCKER
4. 修复后运行 `cargo test`，通过后从 `reports/blockers.md` 删除该条目
5. 所有 BLOCKER 清空后，触发 git_agent 执行 commit

适用场景：迭代过程中卡在某个 BLOCKER 超过 2 轮时手动触发。
