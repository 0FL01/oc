//! MCP09 remote stage deadlines use the same normalized config as stdio.
use super::*;
use oc_adapters::config::{Source, assemble};
use std::collections::BTreeMap;
use std::time::Instant;

#[tokio::test]
async fn mcp09_remote_separate_startup_catalog_execution_deadlines() {
    for (stage, mode) in [
        ("startup", Mode::SlowInitialize),
        ("catalog", Mode::SlowCatalog),
        ("execution", Mode::Ok),
    ] {
        let (url, records) = Fake::start(mode, Duration::from_millis(600));
        let mut timeout = json!({"startup":2000,"catalog":2000,"execution":2000});
        timeout[stage] = json!(100);
        let src = Source { path:"fixture/opencode.json".into(), trusted:true,text:json!({"mcp":{"servers":{"codex_web":{
            "type":"remote","url":url,"headers":{"Authorization":"Bearer test-key"},"timeout":timeout
        }}}}).to_string() };
        let generation = assemble(&[src], &BTreeMap::new(), None).unwrap();
        let mut config = CodexWebConfig::from_entry(&generation.mcp["codex_web"]).unwrap();
        config.allow_private = true;
        let cancel = AtomicBool::new(false);
        let began = Instant::now();
        let connected = CodexWebClient::connect(&config).await;
        if stage == "startup" {
            assert!(matches!(connected, Err(McpError::Deadline)));
        } else {
            let client = connected.unwrap();
            let list = client.list_tools(&cancel).await;
            if stage == "catalog" {
                assert_eq!(list, Err(McpError::Deadline));
            } else {
                assert!(!list.unwrap().is_empty());
                assert_eq!(
                    client.search("slow", None, &cancel).await,
                    Err(McpError::Deadline)
                );
            }
            client.close().await.unwrap();
        }
        assert!(began.elapsed() < Duration::from_secs(1));
        let seen = records_of(&records);
        assert_eq!(
            seen.iter()
                .filter(|r| r.body.contains("\"method\":\"initialize\""))
                .count(),
            1
        );
        assert_eq!(
            seen.iter()
                .filter(|r| r.body.contains("\"method\":\"tools/call\""))
                .count(),
            usize::from(stage == "execution")
        );
    }
}
