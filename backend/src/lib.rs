//! 模型智力检测插件：按账号运行推理题并记录结果；只通过公开 SDK 与宿主通信。

mod check;
mod host_calls;
pub mod jobs;
pub mod judge;
mod management;
mod manifest;
mod questions;
pub mod responses;
pub mod results;
mod worker;

use gateway_plugin_sdk::{
    CallBudgets, Stage,
    client::{AuthorError, ComposedPlugin, Empty, PluginBuilder, TypedReply, methods},
};
use std::time::{SystemTime, UNIX_EPOCH};

pub use manifest::{PLUGIN_ID, manifest};

/// 一轮后台处理的预算；剩余不足一题时停止开始新题，宿主 30 秒后再调下一轮。
const MAINTENANCE_BUDGET_MS: u64 = 30 * 60 * 1000;

/// 组装插件处理器。
///
/// # 错误
///
/// 清单与处理器声明不一致时返回错误。
pub fn plugin() -> Result<ComposedPlugin, AuthorError> {
    PluginBuilder::from_json(include_bytes!("../../plugin.json"))?
        .management(management::registration(), management::handle)?
        .on(methods::RECONCILE, |call| async move {
            worker::reconcile(&call.host).await?;
            Ok(TypedReply::new(Empty {}))
        })?
        .build()
}

/// 会话握手时声明的阶段预算，`main` 与测试宿主共用。
#[must_use]
pub fn call_budgets() -> CallBudgets {
    CallBudgets::from([(Stage::Maintenance, MAINTENANCE_BUDGET_MS)])
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}
