use super::{model::*, parser::*, transport::Transport, write};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

fn remote() -> Vec<ServerEntry> {
    parse_config(r#"{"mcpServers":{"remote":{"type":"http","url":"https://example.invalid/mcp","headers":{"Authorization":"Bearer audit-only"}}}}"#,
        ClientType::Cursor,"mock.json".into()).unwrap()
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "mcp-hub-roundtrip-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let canonical = self.0.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(canonical.parent(), Some(temp.as_path()));
        let _ = std::fs::remove_dir_all(canonical);
    }
}
#[test]
fn remote_headers_do_not_become_environment_variables() {
    let entries = remote();
    assert!(matches!(entries[0].status, ServerStatus::Active));
    assert!(entries[0].command.is_empty() && entries[0].env.is_empty());
    for client in [
        ClientType::Cursor,
        ClientType::ClaudeCode,
        ClientType::VSCode,
        ClientType::Windsurf,
    ] {
        let config = ExportData::for_sync(&entries).config_for(&client).unwrap();
        let parsed = parse_config(&config.to_string(), client, "mock.json".into()).unwrap();
        assert_eq!(parsed[0].transport, entries[0].transport);
    }
    assert!(ExportData::for_sync(&entries)
        .config_for(&ClientType::ClaudeDesktop)
        .is_err());
}
#[test]
fn export_and_scan_hide_authorization_cookies_urls_and_arguments() {
    let mut entries = remote();
    if let Transport::StreamableHttp { url, headers } = &mut entries[0].transport {
        *url = "https://user:audit-only@example.invalid/mcp?token=audit-only#private".into();
        headers.insert("cOoKiE".into(), "audit-only".into());
    }
    entries[0].args = vec!["--api-key".into(), "audit-only".into()];
    let export = serde_json::to_string(&ExportData::from_servers(&entries)).unwrap();
    let scan = serde_json::to_string(&entries[0].redacted()).unwrap();
    assert!(!export.contains("audit-only") && !scan.contains("audit-only"));
    let mut exported = ExportData::from_servers(&entries);
    assert!(exported.resolve_references().is_err());
}
#[test]
fn legacy_export_format_still_imports_and_future_formats_are_rejected() {
    let old: ExportData = serde_json::from_value(
        json!({"version":1,"exported_at":"0","servers":{"a":{"command":"node","args":[]}}}),
    )
    .unwrap();
    assert_eq!(
        old.config_for(&ClientType::VSCode).unwrap()["servers"]["a"]["type"],
        "stdio"
    );
    let mut future = old;
    future.version = 99;
    assert!(future.config_for(&ClientType::Cursor).is_err());
}
#[test]
fn bad_remote_and_unknown_settings_are_explicit_errors() {
    for cfg in [
        json!({"type":"http","url":"file:///tmp/test"}),
        json!({"type":"http"}),
        json!({"type":"mystery","url":"https://example.invalid"}),
        json!({"command":"node","cwd":"/must-not-be-lost"}),
    ] {
        let entries = parse_config(
            &json!({"mcpServers":{"test":cfg}}).to_string(),
            ClientType::Cursor,
            "mock.json".into(),
        )
        .unwrap();
        assert!(matches!(entries[0].status, ServerStatus::ParseError(_)));
    }
}
#[test]
fn dsh_remote_roundtrip_preserves_other_patch_operations() {
    let dir = Directory::new();
    let path = dir.0.join("cordis.patch.yml");
    let original = "- patch:\n    - id: unrelated\n      config:\n        key: value\n";
    std::fs::write(&path, original).unwrap();
    let export = ExportData::for_sync(&remote());
    assert_eq!(
        write::merge(&path, &ClientType::DeepSeekHarness, &export, true).unwrap(),
        (1, 0)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_eq!(
        write::merge(&path, &ClientType::DeepSeekHarness, &export, false).unwrap(),
        (1, 0)
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(original.trim()));
    let entries = parse_dsh_cordis_yaml(&text, ClientType::DeepSeekHarness, path.clone()).unwrap();
    assert_eq!(entries[0].transport, remote()[0].transport);
    assert_eq!(
        write::merge(&path, &ClientType::DeepSeekHarness, &export, false).unwrap(),
        (0, 1)
    );
}
#[test]
fn invalid_destination_is_never_replaced_and_vscode_uses_its_own_bucket() {
    let dir = Directory::new();
    let path = dir.0.join("mcp.json");
    let export = ExportData::for_sync(&remote());
    for text in ["{broken", r#"{"servers":null}"#, "[]"] {
        std::fs::write(&path, text).unwrap();
        assert!(write::merge(&path, &ClientType::VSCode, &export, false).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
    std::fs::write(
        &path,
        r#"{"inputs":[],"servers":{"existing":{"command":"node"}}}"#,
    )
    .unwrap();
    write::merge(&path, &ClientType::VSCode, &export, false).unwrap();
    let result: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(result["inputs"].is_array() && result["servers"]["existing"].is_object());
    assert!(result.get("mcpServers").is_none());
}

#[test]
fn dsh_references_and_literal_exclamation_marks_survive_roundtrip() {
    let dir = Directory::new();
    let path = dir.0.join("cordis.patch.yml");
    let mut entries = remote();
    if let Transport::StreamableHttp { headers, .. } = &mut entries[0].transport {
        headers.insert("Authorization".into(), "${AUDIT_HEADER_TOKEN}".into());
        headers.insert("X-Literal".into(), "abc!!js-not-a-tag".into());
    }
    write::merge(
        &path,
        &ClientType::DeepSeekHarness,
        &ExportData::for_sync(&entries),
        false,
    )
    .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("process.env.AUDIT_HEADER_TOKEN"));
    let parsed =
        parse_dsh_cordis_yaml(&text, ClientType::DeepSeekHarness, "mock.yml".into()).unwrap();
    assert_eq!(parsed[0].transport, entries[0].transport);
}

#[test]
fn clients_receive_their_native_reference_syntax() {
    let mut entries = remote();
    if let Transport::StreamableHttp { headers, .. } = &mut entries[0].transport {
        headers.insert("Authorization".into(), "Bearer ${TOKEN}".into());
    }
    let export = ExportData::for_sync(&entries);
    assert_eq!(
        export.config_for(&ClientType::VSCode).unwrap()["servers"]["remote"]["headers"]
            ["Authorization"],
        "Bearer ${env:TOKEN}"
    );
    assert_eq!(
        export.config_for(&ClientType::ClaudeCode).unwrap()["mcpServers"]["remote"]["headers"]
            ["Authorization"],
        "Bearer ${TOKEN}"
    );
    let second = ServerEntry {
        name: "different endpoint".into(),
        ..entries[0].clone()
    };
    let exported = ExportData::from_servers(&[entries[0].clone(), second]);
    assert_ne!(
        exported.servers["remote"].transport.endpoint().unwrap().1["Authorization"],
        exported.servers["different endpoint"]
            .transport
            .endpoint()
            .unwrap()
            .1["Authorization"]
    );
}

#[test]
fn dsh_unknown_settings_are_not_silently_migrated() {
    let entries=parse_dsh_cordis_yaml("- name: '@deepseek-ai/dsh-mcp-client'\n  config:\n    serverName: test\n    command: node\n    cwd: /must-not-be-lost\n",ClientType::DeepSeekHarness,"mock.yml".into()).unwrap();
    assert!(matches!(entries[0].status, ServerStatus::ParseError(_)));
}
