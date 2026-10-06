//! Built-in OpenAI CLI consumer of the same SQLite credential/attempt owner.
use std::{
    io::{IsTerminal, Write},
    path::Path,
    process::ExitCode,
    time::Duration,
};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use oc_adapters::{
    auth::{AuthScope, OPENAI_BASE_URL, OpenAiAttempts},
    storage::{AccountSummary, CredentialKind, CredentialMaterial, Db},
};
use oc_core::queries::{AuthAttemptState, AuthMethod, OAuthMethod};

use crate::cli::{AuthCommand, AuthFormat};

type Result<T> = std::result::Result<T, &'static str>;
const CANCELLED: &str = "authorization cancelled";

pub async fn run(action: AuthCommand, data: &Path) -> ExitCode {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    match execute(action, data, interactive).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(if error == CANCELLED { 130 } else { 1 })
        }
    }
}

fn target(target: Option<&str>, interactive: bool) -> Result<()> {
    match target {
        Some("openai") => Ok(()),
        None if interactive => Ok(()),
        None => Err("explicit target required without a terminal; supported target: openai"),
        Some(_) => Err("auth commands support the built-in openai target only"),
    }
}

fn scope() -> Result<AuthScope> {
    AuthScope::admit("openai", OPENAI_BASE_URL).map_err(|_| "OpenAI authentication unavailable")
}

fn label(answers: &[String]) -> Result<String> {
    let mut label = None;
    for answer in answers {
        let Some(("label", value)) = answer.split_once('=') else {
            return Err(
                "only the non-secret label answer is supported; keys require the TTY prompt",
            );
        };
        if label.is_some()
            || value.trim().is_empty()
            || value.len() > 128
            || value.chars().any(char::is_control)
        {
            return Err("invalid account label answer");
        }
        label = Some(value.to_owned());
    }
    Ok(label.unwrap_or_else(|| "OpenAI account".into()))
}

fn method(raw: Option<&str>, interactive: bool, methods: &[AuthMethod]) -> Result<AuthMethod> {
    if let Some(raw) = raw {
        return methods
            .iter()
            .copied()
            .find(|m| m.id() == raw || m.label() == raw)
            .ok_or("unknown method; choose chatgpt-browser, chatgpt-headless, or key");
    }
    if !interactive {
        return Err(
            "explicit method required without a terminal: chatgpt-browser, chatgpt-headless, key",
        );
    }
    let choices = methods
        .iter()
        .map(|m| m.label().to_owned())
        .collect::<Vec<_>>();
    Ok(methods[select("OpenAI authentication method", &choices)?])
}

async fn execute(action: AuthCommand, data: &Path, interactive: bool) -> Result<()> {
    let scope = scope()?;
    match action {
        AuthCommand::Login(args) => {
            target(args.target.as_deref(), interactive)?;
            let label = label(&args.answers)?;
            let method = method(args.method.as_deref(), interactive, &scope.methods())?;
            if method == AuthMethod::Key && !interactive {
                return Err(
                    "API key entry requires stdin and stdout TTYs; use admitted OPENAI_API_KEY/config for headless generation",
                );
            }
            let db = Db::open(data).map_err(|_| "credential storage unavailable")?;
            match method {
                AuthMethod::Key => {
                    let key = password()?;
                    let row = db
                        .add_credential(scope.namespace(), &label, CredentialMaterial::Key { key })
                        .map_err(|_| "credential storage unavailable or invalid key")?;
                    output(&format!(
                        "Stored OpenAI API key ({})\nServer authorization has not been validated.\n",
                        row.id
                    ))
                }
                AuthMethod::OAuth(method) => {
                    let owner =
                        OpenAiAttempts::new(&db).map_err(|_| "authorization unavailable")?;
                    let result = login(&owner, &db, &scope, method, label, interactive).await;
                    let cleanup = owner
                        .shutdown()
                        .await
                        .map_err(|_| "authorization cleanup failed");
                    cleanup?;
                    result
                }
            }
        }
        AuthCommand::List { format } => {
            let db = Db::open(data).map_err(|_| "credential storage unavailable")?;
            let rows = accounts(&db, &scope)?;
            list_to(&mut std::io::stdout().lock(), &rows, format)
        }
        AuthCommand::Logout {
            target: selected,
            credential,
        } => {
            target(selected.as_deref(), interactive)?;
            let db = Db::open(data).map_err(|_| "credential storage unavailable")?;
            let rows = accounts(&db, &scope)?;
            let row = account(&rows, credential.as_deref(), interactive)?;
            db.remove_credential(scope.namespace(), &row.id)
                .map_err(|_| "account removal failed")?;
            output("OpenAI account removed\n")
        }
        AuthCommand::Switch {
            target: selected,
            credential,
        } => {
            target(selected.as_deref(), interactive)?;
            let db = Db::open(data).map_err(|_| "credential storage unavailable")?;
            let rows = accounts(&db, &scope)?;
            let row = account(&rows, credential.as_deref(), interactive)?;
            db.activate_credential(scope.namespace(), &row.id)
                .map_err(|_| "account activation failed")?;
            output("OpenAI account activated\n")
        }
    }
}

fn accounts(db: &Db, scope: &AuthScope) -> Result<Vec<AccountSummary>> {
    let mut rows = db
        .credential_accounts(scope.namespace())
        .map_err(|_| "credential storage unavailable")?;
    rows.sort_by(|a, b| a.label.cmp(&b.label).then(a.id.cmp(&b.id)));
    Ok(rows)
}

fn account<'a>(
    rows: &'a [AccountSummary],
    selected: Option<&str>,
    interactive: bool,
) -> Result<&'a AccountSummary> {
    if let Some(selected) = selected {
        if let Some(row) = rows.iter().find(|row| row.id == selected) {
            return Ok(row);
        }
        let matches = rows
            .iter()
            .filter(|row| row.label == selected)
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [row] => Ok(row),
            [] => Err("OpenAI account not found"),
            _ => Err("account label is ambiguous; use its local account ID"),
        };
    }
    match rows {
        [] => Err("no stored OpenAI accounts"),
        [row] => Ok(row),
        _ if !interactive => Err("explicit local account ID required without a terminal"),
        _ => {
            let labels = rows
                .iter()
                .map(|r| {
                    format!(
                        "{} ({}){}",
                        r.label,
                        r.id,
                        if r.active { " [active]" } else { "" }
                    )
                })
                .collect::<Vec<_>>();
            Ok(&rows[select("OpenAI account", &labels)?])
        }
    }
}

fn list_to(out: &mut impl Write, rows: &[AccountSummary], format: AuthFormat) -> Result<()> {
    if format == AuthFormat::Json {
        let rows = rows.iter().map(|row| serde_json::json!({
            "provider":"openai", "id":row.id, "label":row.label,
            "kind":match row.kind { CredentialKind::Key=>"key", CredentialKind::OAuth=>"oauth" },
            "methodID":match row.kind { CredentialKind::Key=>Some("key"), CredentialKind::OAuth=>row.method_id.as_deref().filter(|m| matches!(*m,"chatgpt-browser"|"chatgpt-headless")) },
            "active":row.active, "created_at":row.created_at
        })).collect::<Vec<_>>();
        serde_json::to_writer(&mut *out, &rows).map_err(|_| "auth output unavailable")?;
        writeln!(out).map_err(|_| "auth output unavailable")
    } else {
        if rows.is_empty() {
            writeln!(out, "No stored OpenAI accounts").map_err(|_| "auth output unavailable")?;
        }
        for row in rows {
            writeln!(
                out,
                "{} {} ({}) [{}]",
                if row.active { '*' } else { ' ' },
                row.label,
                row.id,
                match row.kind {
                    CredentialKind::Key => "API key",
                    CredentialKind::OAuth => match row.method_id.as_deref() {
                        Some("chatgpt-browser") => "ChatGPT Pro/Plus (browser)",
                        Some("chatgpt-headless") => "ChatGPT Pro/Plus (headless)",
                        _ => "OAuth (unsupported method)",
                    },
                }
            )
            .map_err(|_| "auth output unavailable")?;
        }
        Ok(())
    }
}

fn output(text: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|_| "auth output unavailable")
}

async fn login(
    owner: &OpenAiAttempts,
    db: &Db,
    scope: &AuthScope,
    method: OAuthMethod,
    label: String,
    interactive: bool,
) -> Result<()> {
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    // Poll/register SIGINT before publishing an attempt or its active URL.
    let attempt = tokio::select! {
        biased;
        result = &mut interrupt => { result.map_err(|_| "interrupt handler unavailable")?; return Err(CANCELLED); }
        result = owner.begin(method, label) => result.map_err(|_| "authorization unavailable")?,
    };
    let mut presented = false;
    loop {
        let view = owner
            .status(&attempt.id)
            .map_err(|_| "authorization unavailable")?;
        match view.state {
            AuthAttemptState::Complete => {
                let id = view
                    .account_id
                    .ok_or("authorization acknowledgement unavailable")?;
                if !accounts(db, scope)?.iter().any(|row| row.id == id) {
                    return Err("authorization acknowledgement unavailable");
                }
                return output(&format!("Connected to OpenAI ({id})\n"));
            }
            AuthAttemptState::Failed(failure) => return Err(failure.message()),
            AuthAttemptState::Expired => return Err("authorization expired"),
            AuthAttemptState::Pending => {}
        }
        if !presented && let Some(url) = &view.url {
            output(&format!(
                "{}\n{}\nWaiting for authorization...\n",
                view.instructions
                    .as_deref()
                    .unwrap_or("Open this URL to authorize OpenAI"),
                url
            ))?;
            presented = true;
            if method == OAuthMethod::Browser && interactive {
                let opened = tokio::select! {
                    biased;
                    result = &mut interrupt => {
                        result.map_err(|_| "interrupt handler unavailable")?;
                        owner.cancel(&attempt.id).await.map_err(|_| "authorization cleanup failed")?;
                        return Err(CANCELLED);
                    }
                    result = open_authorization(url) => result,
                };
                if let Err(error) = opened {
                    if error == "browser cleanup failed" {
                        return Err(error);
                    }
                    output("Browser opening unavailable; open the authorization URL manually.\n")?;
                }
            }
        }
        tokio::select! {
            biased;
            result = &mut interrupt => {
                result.map_err(|_| "interrupt handler unavailable")?;
                owner.cancel(&attempt.id).await.map_err(|_| "authorization cleanup failed")?;
                return Err(CANCELLED);
            }
            () = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
}

/// Explicit auth-surface action; desktop launchers never inherit API credentials.
pub(crate) async fn open_authorization(url: &str) -> Result<()> {
    if !url.starts_with("https://auth.openai.com/")
        || url.len() > 8192
        || url.chars().any(char::is_control)
    {
        return Err("authorization URL unavailable");
    }
    let parent = std::env::vars().collect::<std::collections::BTreeMap<_, _>>();
    let mut env = oc_adapters::shell::child_env(&parent);
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_RUNTIME_DIR",
    ] {
        if let Some(value) = parent
            .get(name)
            .filter(|v| v.len() <= 4096 && !v.chars().any(char::is_control))
        {
            env.insert(name.into(), value.clone());
        }
    }
    let mut command = tokio::process::Command::new("xdg-open");
    command
        .arg(url)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "browser opening unavailable")?;
    let status = match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
        Ok(result) => result.map_err(|_| "browser opening unavailable")?,
        Err(_) => {
            tokio::time::timeout(Duration::from_secs(1), child.kill())
                .await
                .map_err(|_| "browser cleanup failed")?
                .map_err(|_| "browser cleanup failed")?;
            return Err("browser opening unavailable");
        }
    };
    if status.success() {
        Ok(())
    } else {
        Err("browser opening unavailable")
    }
}

struct RawInput {
    active: bool,
    paste: bool,
}
impl RawInput {
    fn new(paste: bool) -> Result<Self> {
        crossterm::terminal::enable_raw_mode().map_err(|_| "terminal input unavailable")?;
        let guard = Self {
            active: true,
            paste,
        };
        if paste {
            crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste)
                .map_err(|_| "terminal input unavailable")?;
        }
        Ok(guard)
    }
    fn close(&mut self) -> Result<()> {
        let paste = if self.paste {
            crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste)
        } else {
            Ok(())
        };
        let mode = crossterm::terminal::disable_raw_mode();
        mode.map_err(|_| "terminal restoration failed")?;
        self.active = false;
        paste.map_err(|_| "terminal restoration failed")?;
        self.paste = false;
        Ok(())
    }
}
impl Drop for RawInput {
    fn drop(&mut self) {
        if self.paste {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);
        }
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

fn select(title: &str, choices: &[String]) -> Result<usize> {
    if choices.is_empty() {
        return Err("no authentication choices");
    }
    let mut raw = RawInput::new(false)?;
    let result = (|| {
        output(&format!("{title}\r\n"))?;
        for (index, label) in choices.iter().enumerate() {
            output(&format!("{}: {label}\r\n", index + 1))?;
        }
        let mut index = 0;
        loop {
            output(&format!("\r\x1b[2K> {}", choices[index]))?;
            if let Event::Key(key) =
                crossterm::event::read().map_err(|_| "terminal input unavailable")?
                && key.kind != KeyEventKind::Release
            {
                match key.code {
                    KeyCode::Esc => return Err(CANCELLED),
                    KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Err(CANCELLED);
                    }
                    KeyCode::Enter => {
                        output("\r\n")?;
                        return Ok(index);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        index = (index + choices.len() - 1) % choices.len()
                    }
                    KeyCode::Down | KeyCode::Char('j') => index = (index + 1) % choices.len(),
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        if let Some(selected) = c
                            .to_digit(10)
                            .and_then(|n| n.checked_sub(1))
                            .filter(|n| (*n as usize) < choices.len())
                        {
                            index = selected as usize;
                        }
                    }
                    _ => {}
                }
            }
        }
    })();
    raw.close()?;
    result
}

fn password() -> Result<String> {
    let mut raw = RawInput::new(true)?;
    let result = (|| {
        output("API key: ")?;
        let mut value = String::new();
        loop {
            match crossterm::event::read().map_err(|_| "terminal input unavailable")? {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Esc => return Err(CANCELLED),
                    KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Err(CANCELLED);
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        while value.pop().is_some() {
                            output("\x08 \x08")?;
                        }
                    }
                    KeyCode::Enter if !value.trim().is_empty() => {
                        output("\r\n")?;
                        return Ok(value);
                    }
                    KeyCode::Backspace => {
                        if value.pop().is_some() {
                            output("\x08 \x08")?;
                        }
                    }
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                            && !c.is_control() =>
                    {
                        if value.len() + c.len_utf8() > 16 * 1024 {
                            return Err("API key exceeds the input limit");
                        }
                        value.push(c);
                        output("*")?;
                    }
                    _ => {}
                },
                Event::Paste(text) => {
                    if text.chars().any(char::is_control) || value.len() + text.len() > 16 * 1024 {
                        return Err("API key paste exceeds the input limit or contains controls");
                    }
                    output(&"*".repeat(text.chars().count()))?;
                    value.push_str(&text);
                }
                _ => {}
            }
        }
    })();
    raw.close()?;
    result
}

#[cfg(test)]
mod tests;
