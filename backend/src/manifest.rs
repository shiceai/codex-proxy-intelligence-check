use gateway_plugin_sdk::Manifest;

pub const PLUGIN_ID: &str = "shiceai.intelligence-check";

/// 读取打包器和运行注册共同使用的作者清单。
///
/// # 错误
///
/// 作者清单无效时返回错误。
pub fn manifest() -> Result<Manifest, gateway_plugin_sdk::ManifestError> {
    Manifest::from_author_slice(include_bytes!("../../plugin.json"))
}
