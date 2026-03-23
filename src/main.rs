mod cli;
mod data_manager;
mod scanner;
mod connection_tracker;
mod graph_builder;
mod visualization;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 参数解析、模块调用由 developer_agent 实现
    // 此文件禁止包含业务逻辑
    Ok(())
}
