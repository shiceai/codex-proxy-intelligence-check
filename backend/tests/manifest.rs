use codex_proxy_intelligence_check::{PLUGIN_ID, manifest};

#[test]
fn manifest_declares_management_and_maintenance() {
    let manifest = manifest().unwrap();
    assert_eq!(manifest.plugin_id().unwrap(), PLUGIN_ID);
    assert_eq!(PLUGIN_ID, "shiceai.intelligence-check");
    let contributes = serde_json::to_value(&manifest.contributes).unwrap();
    assert_eq!(contributes.as_object().unwrap().len(), 2, "{contributes}");
    assert_eq!(contributes["management"]["version"], 2);
    assert_eq!(contributes["maintenance"]["version"], 1);
}

#[test]
fn manifest_version_matches_crate_version() {
    assert_eq!(
        manifest().unwrap().version.to_string(),
        env!("CARGO_PKG_VERSION")
    );
}
