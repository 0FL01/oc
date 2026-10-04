use super::*;

#[test]
fn run_agent_preserves_positional_session_and_global_flags() {
    for command in [
        vec![
            "oc",
            "--auto",
            "run",
            "--agent",
            "plan",
            "--session",
            "saved",
            "--json",
            "prompt words",
        ],
        vec![
            "oc",
            "run",
            "prompt words",
            "--session",
            "saved",
            "--json",
            "--agent",
            "plan",
            "--auto",
        ],
    ] {
        let parsed = Args::try_parse_from(command).unwrap();
        assert!(parsed.auto);
        let Some(Command::Run {
            prompt,
            session,
            agent,
            json,
            image,
        }) = parsed.command
        else {
            panic!("run");
        };
        assert_eq!(prompt, "prompt words");
        assert_eq!(session.as_deref(), Some("saved"));
        assert_eq!(agent.as_deref(), Some("plan"));
        assert!(json);
        assert!(image.is_none());
    }
    let parsed = Args::try_parse_from([
        "oc",
        "run",
        "--session",
        "saved",
        "--",
        "--agent plan is user text",
    ])
    .unwrap();
    let Some(Command::Run { prompt, agent, .. }) = parsed.command else {
        panic!("run");
    };
    assert_eq!(prompt, "--agent plan is user text");
    assert!(agent.is_none());
    assert!(Args::try_parse_from(["oc", "run", "--agent"]).is_err());
}
