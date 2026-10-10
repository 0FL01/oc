use super::*;

#[test]
fn every_vendored_language_loads_its_exact_highlight_and_injection_queries() {
    let mut failures = Vec::new();
    for grammar in &GRAMMARS {
        let language = (grammar.language)();
        let mut parser = tree_sitter::Parser::new();
        if let Err(error) = parser.set_language(&language) {
            failures.push(format!("{}: native language ABI: {error}", grammar.name));
            continue;
        }
        for (kind, query) in [
            ("highlights", grammar.highlights),
            ("injections", grammar.injections),
        ] {
            if let Err(error) = tree_sitter::Query::new(&language, query) {
                failures.push(format!("{}: {kind}: {error}", grammar.name));
            }
        }
        assert!(parser.parse("", None).is_some(), "{}", grammar.name);
        if matches!(grammar.name, "swift" | "nix" | "vue") {
            println!(
                "{}",
                serde_json::json!({
                    "name": grammar.name,
                    "native_abi": language.abi_version(),
                    "node_type_count": language.node_kind_count(),
                    "sample_tree": parser.parse("let value = 42;\n", None).unwrap().root_node().to_sexp(),
                })
            );
        }
    }
    assert_eq!(GRAMMARS.len(), 39, "complete custom and built-in inventory");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn whole_source_queries_keep_multiline_unicode_and_bounded_shared_cache() {
    let theme = Theme::dark();
    let base = Style::default().fg(theme.text());
    let source = "/* first\n   中文 😀 second */\nfn main() { let value = r#\"世界\"#; }";
    let rows = highlight(source, Some("rust"), theme, base).unwrap();
    assert_eq!(
        rows.iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        source
    );
    assert!(rows[1].spans().iter().all(|span| {
        // The exact donor query overlaps @comment and @spell. Its later spell
        // rule overrides only foreground, retaining the comment's italic flag.
        span.style().fg == Some(theme.text())
            && span
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::ITALIC)
    }));
    assert!(rows[2].spans().iter().any(|span| span.content() == "fn"
        && span.style().fg == Some(theme.syntax(crate::theme::SyntaxToken::Function))));
    assert_eq!(highlight(source, Some("rs"), theme, base).unwrap(), rows);
    for index in 0..24 {
        highlight(
            &format!("let value{index} = \"{}\";", "x".repeat(2048)),
            Some("rust"),
            theme,
            base,
        )
        .unwrap();
    }
    SYNTAX.with(|owner| {
        let owner = owner.borrow();
        assert!(owner.bytes <= CACHE_BYTES && owner.cache.len() <= CACHE_ENTRIES);
        assert_eq!(
            owner.bytes,
            owner.cache.iter().map(Cached::bytes).sum::<usize>()
        );
    });
    assert_eq!(
        highlight(&"x".repeat(SOURCE_BYTES + 1), Some("rust"), theme, base),
        Err(Error::Limit)
    );
    assert_eq!(
        highlight(&"x".repeat(SOURCE_BYTES + 1), Some("unknown"), theme, base),
        Err(Error::Limit)
    );
    let plain = "fn fake(世界) { 123 /* no invented language */ }";
    assert_eq!(
        highlight(plain, Some("genuinely-unregistered"), theme, base).unwrap(),
        vec![Line::styled(plain, base)]
    );
    assert_eq!(filetype("src/example.js"), Some("typescript"));
    assert_eq!(filetype("src/example.tsx"), Some("typescript"));
    assert_eq!(filetype("src/example.diff"), Some("diff"));
    assert_eq!(filetype(".js"), None);
    assert_eq!(grammar("patch").unwrap().name, "diff");
    assert_eq!(grammar("makefile").unwrap().name, "make");
}

#[test]
fn code_text_and_actual_injection_registration_are_preserved() {
    let theme = Theme::dark();
    let base = Style::default().fg(theme.text());
    let json = "{\"key\":\"value\"}";
    let rows = highlight(json, Some("JSON"), theme, base).unwrap();
    assert_eq!(
        rows[0].plain_text(),
        json,
        "code and diff never conceal literal quotes"
    );
    let source = "let value = 1;";
    assert_eq!(
        highlight(source, Some("RUST"), theme, base),
        highlight(source, Some("rust"), theme, base)
    );
    assert_eq!(grammar("example.RS options").unwrap().name, "rust");
    assert_eq!(grammar(".YML").unwrap().name, "yaml");
    assert_eq!(grammar("Gemfile").unwrap().name, "ruby");
    assert!(
        grammar("shell").is_none(),
        "do not invent an unregistered alias"
    );
    let markdown = registered("markdown").unwrap();
    for (label, expected) in [("rs", false), ("rust", true)] {
        let source = format!("```{label}\nfn main() {{}}\n```\n");
        let captures = SYNTAX
            .with(|owner| owner.borrow_mut().captures(markdown, &source))
            .unwrap();
        assert_eq!(
            captures
                .iter()
                .any(|c| c.is_injection && c.scope == "keyword.function"),
            expected,
            "injections use the actual mapping and registered aliases, not the outer info resolver"
        );
    }
    let source = "```markdown\n    literal\n```\n";
    let rows = highlight(source, Some("markdown"), theme, base).unwrap();
    assert!(
        rows[1]
            .spans()
            .iter()
            .any(|span| span.content().contains("literal")
                && span.style().fg == Some(theme.markdown(crate::theme::MarkdownToken::Code)))
    );
}

#[test]
fn semantic_scope_rules_keep_attributes_backgrounds_and_exact_base_fallback() {
    use ratatui::style::Modifier;
    for theme in [Theme::dark(), Theme::light()] {
        assert!(
            styles::capture("keyword.type", theme)
                .unwrap()
                .add_modifier
                .contains(Modifier::BOLD | Modifier::ITALIC)
        );
        assert_eq!(
            styles::capture("keyword.import", theme)
                .unwrap()
                .add_modifier,
            Modifier::empty()
        );
        assert_eq!(
            styles::capture("keyword.unregistered.detail", theme),
            styles::capture("keyword", theme)
        );
        assert_eq!(
            styles::capture("markup.unregistered.detail", theme),
            None,
            "fallback is the first component, not an invented intermediate scope"
        );
        assert_eq!(
            styles::capture("diff.plus", theme).unwrap().bg,
            Some(theme.diff_added_background())
        );
        assert_eq!(
            styles::capture("markup.raw.inline", theme).unwrap().bg,
            Some(theme.background())
        );
        assert!(
            styles::capture("markup.heading.1", theme)
                .unwrap()
                .add_modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
        assert_eq!(
            styles::capture("function.call", theme).unwrap().fg,
            Some(theme.syntax(crate::theme::SyntaxToken::Variable))
        );
        assert_eq!(
            styles::capture("function.builtin", theme).unwrap().fg,
            Some(theme.error())
        );
    }
}

#[test]
fn full_inventory_matches_actual_opentui_worker_tokens_and_dark_light_styles() {
    use ratatui::style::{Color, Modifier};
    use serde_json::{Value, json};

    let fixtures: Value =
        serde_json::from_str(include_str!("../../assets/syntax/fixtures.json")).unwrap();
    let reference: Value =
        serde_json::from_str(include_str!("../../assets/syntax/fixtures.reference.json")).unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(reference["version"], 1);
    assert_eq!(fixtures.as_object().unwrap().len(), GRAMMARS.len());
    assert_eq!(cases.len(), GRAMMARS.len() * 2);
    let color = |value: Option<Color>| match value {
        Some(Color::Rgb(r, g, b)) => json!([r, g, b, 255]),
        None => Value::Null,
        other => panic!("unexpected non-RGB syntax color: {other:?}"),
    };
    let attributes = |style: Style| {
        let mut flags = 0;
        for (modifier, bit) in [
            (Modifier::BOLD, 1),
            (Modifier::DIM, 2),
            (Modifier::ITALIC, 4),
            (Modifier::UNDERLINED, 8),
        ] {
            if style.add_modifier.contains(modifier) {
                flags |= bit;
            }
        }
        flags
    };
    let mut failures = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for case in cases {
        let name = case["language"].as_str().unwrap();
        let index = case["index"].as_u64().unwrap() as usize;
        let source = case["source"].as_str().unwrap();
        assert!(seen.insert((name, index)), "duplicate reference case");
        assert_eq!(fixtures[name][index], source);
        let grammar = registered(name).expect("required registered grammar");
        let captures = match SYNTAX.with(|owner| owner.borrow_mut().captures(grammar, source)) {
            Ok(captures) => captures,
            Err(error) => {
                failures.push(format!("{name}/{index}: captures: {error:?}"));
                continue;
            }
        };
        let actual_tokens: Vec<_> = captures
            .iter()
            .map(|c| {
                json!([
                    c.start,
                    c.end,
                    c.scope,
                    c.is_injection,
                    c.contains_injection
                ])
            })
            .collect();
        let expected_tokens: Vec<_> = case["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                json!([
                    c["start"],
                    c["end"],
                    c["scope"],
                    c["meta"]["isInjection"].as_bool().unwrap_or(false),
                    c["meta"]["containsInjection"].as_bool().unwrap_or(false)
                ])
            })
            .collect();
        if actual_tokens != expected_tokens {
            failures.push(format!(
                "{name}/{index}: tokens: native={} reference={} first mismatch={:?}",
                actual_tokens.len(),
                expected_tokens.len(),
                actual_tokens
                    .iter()
                    .zip(&expected_tokens)
                    .find(|(a, b)| a != b)
            ));
        }
        for (mode, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let rows =
                highlight(source, Some(name), theme, Style::default().fg(theme.text())).unwrap();
            assert_eq!(
                rows.iter()
                    .map(Line::plain_text)
                    .collect::<Vec<_>>()
                    .join("\n"),
                source
            );
            let actual: Vec<_> = rows
                .iter()
                .flat_map(Line::spans)
                .flat_map(|span| {
                    span.content().chars().map(|ch| {
                        json!([
                            ch.to_string(),
                            color(span.style().fg),
                            color(span.style().bg),
                            attributes(span.style())
                        ])
                    })
                })
                .collect();
            let expected: Vec<_> = case["styled"][mode]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|chunk| {
                    chunk["text"]
                        .as_str()
                        .unwrap()
                        .chars()
                        .filter(|&ch| ch != '\n')
                        .map(|ch| {
                            json!([
                                ch.to_string(),
                                chunk["fg"],
                                chunk["bg"],
                                chunk["attributes"]
                            ])
                        })
                })
                .collect();
            if actual != expected {
                failures.push(format!(
                    "{name}/{index}/{mode}: styles: native={} reference={} first mismatch={:?}",
                    actual.len(),
                    expected.len(),
                    actual.iter().zip(&expected).find(|(a, b)| a != b)
                ));
            }
        }
    }
    for grammar in &GRAMMARS {
        for index in 0..2 {
            assert!(
                seen.contains(&(grammar.name, index)),
                "missing required fixture"
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
