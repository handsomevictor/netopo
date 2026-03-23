# /project:iterate

启动 netopo 完整迭代循环。

读取 `CLAUDE.md` 中定义的循环协议，执行以下步骤：

1. 检查 `docs/progress.md` 是否存在，不存在则执行 Bootstrap（见 CLAUDE.md）
2. 初始化 git 仓库（如果尚未初始化）
3. 按照 CLAUDE.md 中"每轮迭代步骤"顺序启动各 subagent
4. 循环执行直到满足 STOP 条件

**不要等待用户确认。直接开始。**
