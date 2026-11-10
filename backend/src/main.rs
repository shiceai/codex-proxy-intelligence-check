use codex_proxy_intelligence_check::{PLUGIN_ID, call_budgets, manifest, plugin};
use gateway_plugin_sdk::client::{PluginSession, SessionConfig, SessionError};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        SessionConfig::default(),
    )
    .await?;
    let manifest = manifest()?;
    if session.handshake().plugin_id != PLUGIN_ID
        || session.handshake().contributes != manifest.contributes
        || !session
            .handshake()
            .configuration
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        return Err(SessionError::Handshake.into());
    }
    session.set_call_budgets(call_budgets())?;
    session.run(plugin()?).await?;
    Ok(())
}
