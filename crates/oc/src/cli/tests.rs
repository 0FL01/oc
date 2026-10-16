use super::*;

#[test]
fn vis36_cli_auto_aliases_share_the_global_flag_and_remain_hidden_from_help() {
    use clap::CommandFactory;

    for flag in ["--auto", "--yolo", "--dangerously-skip-permissions"] {
        for argv in [
            vec!["oc", flag],
            vec!["oc", flag, "tui", "--session", "saved"],
            vec!["oc", "tui", "--session", "saved", flag],
            vec!["oc", flag, "run", "prompt words", "--session", "saved"],
            vec!["oc", "run", "prompt words", "--session", "saved", flag],
        ] {
            let parsed = Args::try_parse_from(argv).unwrap();
            assert!(parsed.auto);
            match parsed.command {
                None => {}
                Some(Command::Tui { session }) => assert_eq!(session.as_deref(), Some("saved")),
                Some(Command::Run {
                    prompt, session, ..
                }) => {
                    assert_eq!(prompt, "prompt words");
                    assert_eq!(session.as_deref(), Some("saved"));
                }
                _ => panic!("unexpected command"),
            }
        }
        let parsed = Args::try_parse_from(["oc", "run", "--", flag]).unwrap();
        assert!(!parsed.auto);
        let Some(Command::Run { prompt, .. }) = parsed.command else {
            panic!("run");
        };
        assert_eq!(prompt, flag, "literal prompt is not an auto mode flag");
    }
    for argv in [vec!["oc"], vec!["oc", "tui"], vec!["oc", "run", "prompt"]] {
        assert!(!Args::try_parse_from(argv).unwrap().auto);
    }
    let mut command = Args::command();
    command.build();
    let mut helps = vec![
        command.render_help().to_string(),
        command.render_long_help().to_string(),
    ];
    for name in ["tui", "run"] {
        let subcommand = command.find_subcommand_mut(name).unwrap();
        helps.push(subcommand.render_help().to_string());
        helps.push(subcommand.render_long_help().to_string());
    }
    for help in helps {
        assert!(help.contains("--auto"));
        assert!(!help.contains("--yolo"));
        assert!(!help.contains("--dangerously-skip-permissions"));
    }
}

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
