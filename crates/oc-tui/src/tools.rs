//! Upstream v2.0.12 tool-call rendering: inline tool rows and block tool
//! cards (shell, apply_patch diffs, subagent), with the upstream visible
//! strings and theme roles.
//!
//! All labels and colors cite the upstream sources at tag `v2.0.12`
//! (`packages/tui/src/routes/session/index.tsx`, `message-parts.tsx`,
//! `component/spinner.tsx`) as inventoried in
//! `evidence/tui/upstream-inventory.md` §4 and §6. Fields our recorded
//! operations do not carry are omitted, never invented; the deviations are
//! listed in the report and in the individual renderer docs.
//!
//! - inline tools (`message-parts.tsx:176-253`, `index.tsx:2688-2759`): one
//!   row at `paddingLeft=3` with a 2-cell icon column; pending shows the
//!   static spinner fallback `⋯` plus the pending label, failed uses
//!   `text.feedback.error.base`, denied adds strikethrough;
//! - block tools (`index.tsx:2784-2866`): left `┃` border on
//!   `background.raised.base`, padding 1/2, header, body, error line;
//! - shell (`index.tsx:2884-3037`): `$ <cmd>` (running: spinner, no `$`),
//!   `cd <workdir> && ` prefix, output collapsed with
//!   `[earlier output omitted]`, `Command exited with code N` /
//!   `Command cancelled` / `Command timed out` (`index.tsx:2256-2259`);
//! - apply_patch (`index.tsx:3388-3503`): per-file `# Created` /
//!   `← Patched` / `# Deleted`, `# Patch failed` on error, diff lines with
//!   `diff.text.*` / `diff.background.*` / `diff.lineNumber.text`;
//! - subagent (`index.tsx:3173-3205`): `<Agent> Subagent — <desc> · <model>`
//!   with the `↳`/`✓` icon and the pending `Delegating…`;
//! - generic/MCP (`index.tsx:2620-2669`): `✓/✗ <tool> <args>` with the
//!   output visible on failure.

use oc_adapters::patch::{DiffFileRender, DiffLineKind};

use crate::history::ToolCard;
use crate::styled::{self, Line, Span};
use crate::theme::Theme;

/// Block-card inner padding: `padding 1/2` (`index.tsx:2784-2866`).
pub const TOOL_PADDING: usize = 2;
/// Inline icon column (`message-parts.tsx:18,213-219`).
pub const TOOL_ICON_WIDTH: usize = 2;
/// Output rows kept before the `[earlier output omitted]` marker
/// (`index.tsx:2980-3037`).
pub const TOOL_OUTPUT_LINES: usize = 10;

/// Static spinner fallback while animations are off
/// (`component/spinner.tsx:29-33`, `spinner-frames.ts:1`).
pub const SPINNER: &str = "⋯";
/// Output collapse marker (`index.tsx:2976`).
pub const EARLIER_OUTPUT_OMITTED: &str = "[earlier output omitted]";
/// Shell status strings (`index.tsx:2256-2259`).
pub const COMMAND_CANCELLED: &str = "Command cancelled";
/// Shell status strings (`index.tsx:2256-2259`).
pub const COMMAND_TIMED_OUT: &str = "Command timed out";
/// Shell status strings (`index.tsx:2256-2259`).
pub fn command_exited(code: i64) -> String {
    format!("Command exited with code {code}")
}
/// apply_patch failure header (`index.tsx:3496`, §6).
pub const PATCH_FAILED: &str = "# Patch failed";

/// Parsed presentation data for one tool card; built once from the recorded
/// input/output so rendering never re-parses a large payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRender {
    /// `bash` command card.
    Shell(ShellRender),
    /// `apply_patch` diff card.
    Patch(PatchRender),
    /// `subagent` child-session card.
    Subagent(SubagentRender),
    /// Short-lived tools and generic/MCP calls.
    Inline(InlineRender),
}

/// `bash` card fields parsed from the recorded argv/cwd and output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellRender {
    /// Command as recorded (`argv` joined by spaces).
    pub command: String,
    /// `cd <cwd> && ` prefix when the call pinned a working directory.
    pub cwd: Option<String>,
    /// Exit code when the output carried one.
    pub exit: Option<i64>,
    /// True when the process ended on a signal (`exit signal`).
    pub signal: bool,
    /// Stdout rows.
    pub stdout: Vec<String>,
    /// Stderr rows (after the runtime's `[stderr]` separator).
    pub stderr: Vec<String>,
    /// Runtime `[timeout]` marker.
    pub timed_out: bool,
    /// Runtime `[truncated]` marker.
    pub truncated: bool,
}

/// `apply_patch` card: bounded per-file hunks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatchRender {
    /// Files in patch order (bounded by the adapter caps).
    pub files: Vec<DiffFileRender>,
    /// True when the recorded outcome is an error.
    pub failed: bool,
}

/// `subagent` card fields parsed from the recorded request and result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentRender {
    /// Child agent id.
    pub agent: String,
    /// Short child title.
    pub description: String,
    /// Explicit model override, when the call pinned one.
    pub model: Option<String>,
    /// Child session id from the `<subagent …>` result wrapper.
    pub child_session: Option<String>,
    /// Child state from the `<subagent …>` result wrapper.
    pub child_state: Option<String>,
    /// Child final text rows from the result wrapper.
    pub result: Vec<String>,
    /// Failure reason from the recorded error output.
    pub error: Option<String>,
}

/// Inline tool variants (`index.tsx:3080-3170,3544-3550,2620-2669`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineRender {
    /// `read` tool.
    Read {
        /// Recorded path.
        path: String,
    },
    /// `glob` tool.
    Glob {
        /// Recorded pattern.
        pattern: String,
        /// Returned match count from the recorded pagination.
        matches: Option<usize>,
    },
    /// `grep` tool.
    Grep {
        /// Recorded pattern.
        pattern: String,
        /// Returned match count from the recorded pagination.
        matches: Option<usize>,
    },
    /// `webfetch` tool.
    WebFetch {
        /// Recorded URL.
        url: String,
    },
    /// `skill` tool.
    Skill {
        /// Recorded skill id.
        id: String,
    },
    /// Any other tool (MCP included): `key: value` arguments.
    Generic {
        /// Recorded argument pairs, bounded.
        args: Vec<(String, String)>,
    },
}

/// Max argument pairs rendered for a generic tool.
pub const GENERIC_ARGS_MAX: usize = 4;
/// Max characters per rendered argument value.
pub const GENERIC_ARG_CHARS: usize = 120;

impl ToolRender {
    /// Parse the presentation data of one recorded tool operation.
    ///
    /// Unknown or unparsable inputs produce the generic renderer over the
    /// bounded argument preview; nothing is fabricated.
    pub fn parse(name: &str, input: Option<&str>, output: Option<&str>, state: &str) -> Self {
        let value = input.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
        match name {
            "bash" => ToolRender::Shell(shell_render(value.as_ref(), output)),
            "apply_patch" => ToolRender::Patch(patch_render(value.as_ref(), output, state)),
            "subagent" => ToolRender::Subagent(subagent_render(value.as_ref(), output)),
            "read" => ToolRender::Inline(InlineRender::Read {
                path: string_arg(value.as_ref(), "path"),
            }),
            "glob" => ToolRender::Inline(InlineRender::Glob {
                pattern: string_arg(value.as_ref(), "pattern"),
                matches: returned_count(output),
            }),
            "grep" => ToolRender::Inline(InlineRender::Grep {
                pattern: string_arg(value.as_ref(), "pattern"),
                matches: returned_count(output),
            }),
            "webfetch" => ToolRender::Inline(InlineRender::WebFetch {
                url: string_arg(value.as_ref(), "url"),
            }),
            "skill" => ToolRender::Inline(InlineRender::Skill {
                id: string_arg(value.as_ref(), "id"),
            }),
            _ => ToolRender::Inline(InlineRender::Generic {
                args: generic_args(value.as_ref()),
            }),
        }
    }
}

impl InlineRender {
    /// Upstream pending label (`index.tsx:3072,3083,3107,3130,3142,3191,3547`).
    pub fn pending_label(&self) -> &'static str {
        match self {
            InlineRender::Read { .. } => "Reading file…",
            InlineRender::Glob { .. } => "Finding files…",
            InlineRender::Grep { .. } => "Searching content…",
            InlineRender::WebFetch { .. } => "Fetching from the web…",
            InlineRender::Skill { .. } => "Loading skill…",
            InlineRender::Generic { .. } => "",
        }
    }
}

fn string_arg(value: Option<&serde_json::Value>, key: &str) -> String {
    value
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Returned count from the recorded glob/grep pagination (never invented).
fn returned_count(output: Option<&str>) -> Option<usize> {
    let value = serde_json::from_str::<serde_json::Value>(output?).ok()?;
    value
        .get("pagination")?
        .get("returned")?
        .as_u64()
        .map(|count| count as usize)
}

fn generic_args(value: Option<&serde_json::Value>) -> Vec<(String, String)> {
    let Some(object) = value.and_then(|value| value.as_object()) else {
        return Vec::new();
    };
    object
        .iter()
        .take(GENERIC_ARGS_MAX)
        .map(|(key, value)| {
            let text = match value {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            (
                key.clone(),
                crate::truncate_utf8(&text, GENERIC_ARG_CHARS).to_string(),
            )
        })
        .collect()
}

fn shell_render(value: Option<&serde_json::Value>, output: Option<&str>) -> ShellRender {
    let command = value
        .and_then(|value| value.get("argv"))
        .and_then(|value| value.as_array())
        .map(|argv| {
            argv.iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let cwd = value
        .and_then(|value| value.get("cwd"))
        .and_then(|value| value.as_str())
        .filter(|cwd| !cwd.is_empty() && *cwd != ".")
        .map(str::to_string);
    let mut render = ShellRender {
        command,
        cwd,
        ..ShellRender::default()
    };
    let Some(output) = output else {
        return render;
    };
    let mut rest = output;
    if let Some((first, tail)) = output.split_once('\n') {
        if let Some(code) = first.strip_prefix("exit ") {
            match code.trim().parse::<i64>() {
                Ok(code) => render.exit = Some(code),
                Err(_) => render.signal = true,
            }
            rest = tail;
        }
    } else if let Some(code) = output.strip_prefix("exit ") {
        match code.trim().parse::<i64>() {
            Ok(code) => render.exit = Some(code),
            Err(_) => render.signal = true,
        }
        rest = "";
    }
    let (stdout, stderr) = match rest.split_once("\n[stderr]\n") {
        Some((stdout, stderr)) => (stdout, Some(stderr)),
        None => (rest, None),
    };
    for line in stdout.lines() {
        if line == "[truncated]" {
            render.truncated = true;
        } else if line == "[timeout]" {
            render.timed_out = true;
        } else {
            render.stdout.push(line.to_string());
        }
    }
    if let Some(stderr) = stderr {
        for line in stderr.lines() {
            if line == "[truncated]" {
                render.truncated = true;
            } else if line == "[timeout]" {
                render.timed_out = true;
            } else {
                render.stderr.push(line.to_string());
            }
        }
    }
    render
}

fn patch_render(
    value: Option<&serde_json::Value>,
    output: Option<&str>,
    state: &str,
) -> PatchRender {
    let patch = value
        .and_then(|value| value.get("patchText"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    PatchRender {
        files: oc_adapters::patch::diff_render(patch),
        failed: is_error_state(state) || output.is_some_and(|out| out.starts_with("error: ")),
    }
}

fn subagent_render(value: Option<&serde_json::Value>, output: Option<&str>) -> SubagentRender {
    let mut render = SubagentRender {
        agent: string_arg(value, "agent"),
        description: string_arg(value, "description"),
        model: value
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        ..SubagentRender::default()
    };
    let Some(output) = output else {
        return render;
    };
    if let Some(body) = output
        .strip_prefix("<subagent ")
        .and_then(|rest| rest.split_once('>'))
        .and_then(|(attrs, body)| {
            body.strip_suffix("</subagent>")
                .map(|body| (attrs.to_string(), body.to_string()))
        })
    {
        let (attrs, body) = body;
        render.child_session = attr_value(&attrs, "sessionID");
        render.child_state = attr_value(&attrs, "state");
        render.result = body
            .trim_matches('\n')
            .lines()
            .map(str::to_string)
            .collect();
        return render;
    }
    if let Some(reason) = output
        .strip_prefix("error: subagent failed (sessionID: ")
        .and_then(|rest| rest.split_once(')'))
    {
        render.child_session = Some(reason.0.to_string());
        render.error = Some(reason.1.trim_start_matches(':').trim().to_string());
        return render;
    }
    if let Some(reason) = output.strip_prefix("error: ") {
        render.error = Some(reason.to_string());
    }
    render
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = attrs.find(&needle)? + needle.len();
    let rest = &attrs[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// True while the operation has no terminal state yet. `unknown` is the
/// crash-recovery state (an intent whose outcome was never recorded) and is
/// rendered as an error, never as a forever-running card.
pub fn is_running(state: &str) -> bool {
    matches!(state, "started" | "")
}

/// True for every non-success terminal state (error presentation).
pub fn is_error_state(state: &str) -> bool {
    matches!(state, "failed" | "denied" | "cancelled" | "unknown")
}

/// Render one tool card as transcript rows. `width == 0` is the unbounded
/// text projection (no background padding).
pub fn tool_block(card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    match &card.render {
        ToolRender::Shell(shell) => shell_block(shell, card, theme, width),
        ToolRender::Patch(patch) => patch_block(patch, card, theme, width),
        ToolRender::Subagent(subagent) => subagent_block(subagent, card, theme, width),
        ToolRender::Inline(inline) => inline_rows(inline, card, theme),
    }
}

/// Block frame: left `┃` border, raised background, padding 1/2
/// (`index.tsx:2784-2866`).
struct BlockFrame<'t> {
    theme: &'t Theme,
    bg: ratatui::style::Color,
    width: usize,
}

impl BlockFrame<'_> {
    fn new(theme: &Theme, width: u16) -> BlockFrame<'_> {
        BlockFrame {
            theme,
            // Block tools sit on `background.raised.base`
            // (`index.tsx:2784-2866`), the same surface as user rows.
            bg: theme.background_raised(),
            width: width as usize,
        }
    }

    /// Left `┃` border. The inventory does not pin the block-tool border
    /// color, so the neutral `border.base` role is used (undetermined
    /// upstream detail, recorded in the report).
    fn border_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(self.theme.border())
            .bg(self.bg)
    }

    /// One padded row with the left border.
    fn row(&self, spans: &[Span]) -> Line {
        let mut all = vec![Span::styled("┃", self.border_style())];
        all.push(Span::styled(" ".repeat(TOOL_PADDING), self.body_style()));
        all.extend(spans.iter().cloned());
        let used: usize = all.iter().map(styled::span_width).sum();
        if used < self.width {
            all.push(Span::styled(
                " ".repeat(self.width - used),
                ratatui::style::Style::default().bg(self.bg),
            ));
        }
        Line::new(all).with_style(ratatui::style::Style::default().bg(self.bg))
    }

    fn body_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(self.theme.text_muted())
            .bg(self.bg)
    }
}

fn shell_block(shell: &ShellRender, card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    let frame = BlockFrame::new(theme, width);
    let mut out = vec![frame.row(&[])];
    let running = is_running(&card.state);
    let mut header = vec![Span::styled(
        if running {
            format!("{SPINNER} ")
        } else {
            "$ ".to_string()
        },
        ratatui::style::Style::default()
            .fg(theme.text())
            .bg(frame.bg),
    )];
    if let Some(cwd) = &shell.cwd {
        header.push(Span::styled(format!("cd {cwd} && "), frame.body_style()));
    }
    header.push(Span::styled(
        shell.command.clone(),
        ratatui::style::Style::default()
            .fg(theme.text())
            .bg(frame.bg),
    ));
    out.push(frame.row(&header));

    let muted = frame.body_style();
    let error = ratatui::style::Style::default()
        .fg(theme.error())
        .bg(frame.bg);
    // Status line: exact upstream shell strings where our recorded output
    // maps to them (`index.tsx:2256-2259`).
    if !running {
        let status = if card.state == "cancelled" {
            Some(COMMAND_CANCELLED.to_string())
        } else if shell.timed_out {
            Some(COMMAND_TIMED_OUT.to_string())
        } else if let Some(code) = shell.exit {
            Some(command_exited(code))
        } else if shell.signal {
            Some("exit signal".to_string())
        } else {
            None
        };
        if let Some(status) = status {
            out.push(frame.row(&[Span::styled(status, muted)]));
        }
    }
    // Running shell cards show the spinner header only: our runtime records
    // the complete argv before the call appears, so the upstream
    // `Writing command…` placeholder (`index.tsx:3001-3003`, for a command
    // still streaming) has nothing to describe here.
    if !running {
        let omitted = shell.stdout.len().saturating_sub(TOOL_OUTPUT_LINES);
        if omitted > 0 {
            out.push(frame.row(&[Span::styled(EARLIER_OUTPUT_OMITTED, muted)]));
        }
        // An error outcome colors the whole recorded output; a successful one
        // keeps stdout muted (`index.tsx:2784-2866` error line).
        let stdout_style = if is_error_state(&card.state) {
            error
        } else {
            muted
        };
        for line in shell.stdout.iter().skip(omitted) {
            if card.state == "cancelled" && line == "error: cancelled" {
                // The status row already says `Command cancelled`.
                continue;
            }
            out.push(frame.row(&[Span::styled(line.clone(), stdout_style)]));
        }
        if !shell.stderr.is_empty() {
            out.push(frame.row(&[Span::styled("[stderr]", muted)]));
            for line in &shell.stderr {
                out.push(frame.row(&[Span::styled(line.clone(), error)]));
            }
        }
        if shell.truncated || card.output_truncated {
            out.push(frame.row(&[Span::styled("[truncated]", muted)]));
        }
    }
    out.push(frame.row(&[]));
    out
}

fn patch_block(patch: &PatchRender, card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    let frame = BlockFrame::new(theme, width);
    let muted = frame.body_style();
    let error = ratatui::style::Style::default()
        .fg(theme.error())
        .bg(frame.bg);
    let mut out = vec![frame.row(&[])];
    if patch.failed {
        out.push(frame.row(&[Span::styled(PATCH_FAILED, error)]));
    } else {
        if is_running(&card.state) {
            // Upstream running label (`index.tsx:3496`, §6).
            out.push(
                frame.row(&[Span::styled(
                    "Patching",
                    ratatui::style::Style::default()
                        .fg(theme.text())
                        .bg(frame.bg),
                )]),
            );
        }
        for file in &patch.files {
            let (label, color) = match file.change {
                "Add" => ("# Created", theme.diff_added()),
                "Delete" => ("# Deleted", theme.diff_removed()),
                _ => ("← Patched", theme.diff_context()),
            };
            let mut header = vec![
                Span::styled(
                    label,
                    ratatui::style::Style::default().fg(color).bg(frame.bg),
                ),
                Span::styled(
                    format!(" {}", file.path),
                    ratatui::style::Style::default()
                        .fg(theme.text())
                        .bg(frame.bg),
                ),
            ];
            if file.additions > 0 {
                header.push(Span::styled(
                    format!(" +{}", file.additions),
                    ratatui::style::Style::default()
                        .fg(theme.diff_added())
                        .bg(frame.bg),
                ));
            }
            if file.removals > 0 {
                header.push(Span::styled(
                    format!(" -{}", file.removals),
                    ratatui::style::Style::default()
                        .fg(theme.diff_removed())
                        .bg(frame.bg),
                ));
            }
            out.push(frame.row(&header));
            for hunk in &file.hunks {
                if let Some(anchor) = &hunk.anchor {
                    out.push(
                        frame.row(&[Span::styled(
                            format!("@@ {anchor}"),
                            ratatui::style::Style::default()
                                .fg(theme.diff_hunk_header())
                                .bg(frame.bg),
                        )]),
                    );
                } else if !file.hunks.is_empty() && file.change == "Update" {
                    out.push(
                        frame.row(&[Span::styled(
                            "@@",
                            ratatui::style::Style::default()
                                .fg(theme.diff_hunk_header())
                                .bg(frame.bg),
                        )]),
                    );
                }
                let gutter = hunk
                    .lines
                    .iter()
                    .filter_map(|line| line.line_number)
                    .max()
                    .map(|max| max.to_string().len());
                for line in &hunk.lines {
                    let (marker, fg, bg) = match line.kind {
                        DiffLineKind::Added => {
                            ("+", theme.diff_added(), theme.diff_added_background())
                        }
                        DiffLineKind::Removed => {
                            ("-", theme.diff_removed(), theme.diff_removed_background())
                        }
                        DiffLineKind::Context => {
                            (" ", theme.diff_context(), theme.diff_context_background())
                        }
                    };
                    let mut spans = Vec::new();
                    if let (Some(number), Some(gutter)) = (line.line_number, gutter) {
                        spans.push(Span::styled(
                            format!("{number:>gutter$} "),
                            ratatui::style::Style::default()
                                .fg(theme.diff_line_number())
                                .bg(bg),
                        ));
                    }
                    spans.push(Span::styled(
                        format!("{marker}{}", line.text),
                        ratatui::style::Style::default().fg(fg).bg(bg),
                    ));
                    out.push(frame.row(&spans));
                }
                if hunk.truncated {
                    out.push(frame.row(&[Span::styled("[truncated]", muted)]));
                }
            }
        }
    }
    if patch.files.is_empty() && !patch.failed {
        // Malformed or empty payload: keep the recorded outcome text instead
        // of an invented diff.
        for line in card.output_preview.lines() {
            out.push(frame.row(&[Span::styled(line.to_string(), muted)]));
        }
    }
    if is_error_state(&card.state) {
        for line in card.output_preview.lines() {
            if !line.is_empty() {
                out.push(frame.row(&[Span::styled(line.to_string(), error)]));
            }
        }
    }
    out.push(frame.row(&[]));
    out
}

fn subagent_block(
    subagent: &SubagentRender,
    card: &ToolCard,
    theme: &Theme,
    width: u16,
) -> Vec<Line> {
    let frame = BlockFrame::new(theme, width);
    let muted = frame.body_style();
    let error = ratatui::style::Style::default()
        .fg(theme.error())
        .bg(frame.bg);
    let title = ratatui::style::Style::default()
        .fg(theme.text())
        .bg(frame.bg);
    let mut out = vec![frame.row(&[])];
    let running = is_running(&card.state);
    let mut header = Vec::new();
    if running {
        // Subagent icons are `↳`/`│`/`✓` (`index.tsx:3173-3205`); the running
        // card pairs `↳` with the pending `Delegating…`.
        header.push(Span::styled("↳ ", title));
        header.push(Span::styled("Delegating…", title));
    } else {
        let (icon, style) = if is_error_state(&card.state) {
            ("✗", error)
        } else {
            ("✓", title)
        };
        header.push(Span::styled(format!("{icon} "), style));
        header.push(Span::styled(
            format!(
                "{} Subagent — {}",
                crate::messages::Locale::titlecase(&subagent.agent),
                subagent.description
            ),
            title,
        ));
        if let Some(model) = &subagent.model {
            header.push(Span::styled(format!(" · {model}"), muted));
        }
    }
    out.push(frame.row(&header));
    if let Some(session) = &subagent.child_session {
        let line = match &subagent.child_state {
            Some(state) => format!("{session} · {state}"),
            None => session.clone(),
        };
        out.push(frame.row(&[Span::styled(line, muted)]));
    }
    let omitted = subagent.result.len().saturating_sub(TOOL_OUTPUT_LINES);
    if omitted > 0 {
        out.push(frame.row(&[Span::styled(EARLIER_OUTPUT_OMITTED, muted)]));
    }
    for line in subagent.result.iter().skip(omitted) {
        out.push(frame.row(&[Span::styled(line.clone(), muted)]));
    }
    if let Some(reason) = &subagent.error {
        out.push(frame.row(&[Span::styled(format!("error: {reason}"), error)]));
    }
    out.push(frame.row(&[]));
    out
}

fn inline_rows(inline: &InlineRender, card: &ToolCard, theme: &Theme) -> Vec<Line> {
    let pad = Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING));
    let running = is_running(&card.state);
    let failed = is_error_state(&card.state);
    let style = if failed {
        ratatui::style::Style::default().fg(theme.error())
    } else {
        ratatui::style::Style::default().fg(theme.text())
    };
    let generic = matches!(inline, InlineRender::Generic { .. });
    // Generic tools use the cited `✓/✗` markers (`index.tsx:2620-2669`); the
    // named inline tools keep an empty 2-cell icon column once terminal.
    let icon = if running {
        SPINNER.to_string()
    } else if generic {
        if failed {
            "✗".to_string()
        } else {
            "✓".to_string()
        }
    } else {
        String::new()
    };
    // Only a denied call is struck through (`message-parts.tsx:176-253`);
    // failed/cancelled keep the plain error color.
    let style = if card.state == "denied" {
        style.add_modifier(ratatui::style::Modifier::CROSSED_OUT)
    } else {
        style
    };
    let label = if running {
        let mut label = String::new();
        if generic {
            label.push_str(&card.name);
        }
        let pending = inline.pending_label();
        if !pending.is_empty() {
            if !label.is_empty() {
                label.push(' ');
            }
            label.push_str(pending);
        }
        label
    } else {
        let mut label = String::new();
        if generic {
            label.push_str(&card.name);
            label.push(' ');
        }
        label.push_str(&inline_label(inline));
        label
    };
    let spans = vec![
        pad.clone(),
        Span::styled(format!("{icon:<TOOL_ICON_WIDTH$}"), style),
        Span::styled(label, style),
    ];
    let mut out = vec![Line::new(spans)];
    if !running
        && !failed
        && let InlineRender::Read { path } = inline
    {
        out.push(Line::new(vec![
            pad.clone(),
            Span::styled(
                format!("{:<TOOL_ICON_WIDTH$}", ""),
                ratatui::style::Style::default().fg(theme.text_muted()),
            ),
            Span::styled(
                format!("↳ Loaded {path}"),
                ratatui::style::Style::default().fg(theme.text_muted()),
            ),
        ]));
    }
    if failed {
        for line in card.output_preview.lines().take(TOOL_OUTPUT_LINES) {
            if line.is_empty() {
                continue;
            }
            out.push(Line::new(vec![
                pad.clone(),
                Span::styled(
                    format!("{:<TOOL_ICON_WIDTH$}", ""),
                    ratatui::style::Style::default().fg(theme.error()),
                ),
                Span::styled(
                    line.to_string(),
                    ratatui::style::Style::default().fg(theme.error()),
                ),
            ]));
        }
    }
    out
}

/// Terminal label for an inline tool (`index.tsx:3084-3168,3548,2620-2669`).
fn inline_label(inline: &InlineRender) -> String {
    match inline {
        InlineRender::Read { path } => format!("Read {path}"),
        // Our glob/grep take no path argument, so the upstream ` in <path>`
        // clause is omitted instead of being invented.
        InlineRender::Glob { pattern, matches } => match matches {
            Some(count) => format!("Glob \"{pattern}\" ({count} matches)"),
            None => format!("Glob \"{pattern}\""),
        },
        InlineRender::Grep { pattern, matches } => match matches {
            Some(count) => format!("Grep \"{pattern}\" ({count} matches)"),
            None => format!("Grep \"{pattern}\""),
        },
        InlineRender::WebFetch { url } => format!("WebFetch {url}"),
        InlineRender::Skill { id } => format!("Skill \"{id}\""),
        InlineRender::Generic { args } => {
            let mut out = String::new();
            for (key, value) in args {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(key);
                out.push_str(": ");
                out.push_str(value);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryRow, card_from_row};
    use crate::messages::{Chip, transcript};
    use crate::theme::{MarkdownToken, SyntaxToken, Theme};
    use oc_core::queries::ToolOpView;
    use ratatui::buffer::Buffer;
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    fn make_card(
        name: &str,
        state: &str,
        input: serde_json::Value,
        output: Option<&str>,
    ) -> ToolCard {
        card_from_row(&ToolOpView {
            rowid: 0,
            op: "op-1".to_string(),
            name: name.to_string(),
            state: state.to_string(),
            input: Some(input.to_string()),
            output: output.map(str::to_string),
            output_bytes: output.map(str::len).unwrap_or(0) as i64,
            output_truncated: false,
        })
    }

    fn tool_row(card: &ToolCard) -> HistoryRow {
        HistoryRow {
            seq: i64::MAX,
            role: "tool".to_string(),
            text: String::new(),
            agent: Some("build".to_string()),
            chips: Vec::<Chip>::new(),
            reasoning: None,
            meta: None,
            tool: Some(card.clone()),
        }
    }

    /// Render one tool row through the transcript dispatcher and return the
    /// trimmed row texts plus the raw buffer for color assertions.
    fn render(card: &ToolCard, width: u16, height: u16) -> (Vec<String>, Buffer) {
        let theme = Theme::dark();
        let lines = transcript(&[tool_row(card)], theme, width, width, |_| {
            theme.categorical_agents()[0]
        });
        let wrapped = styled::wrap_lines(&lines, width as usize);
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(styled::Lines::from(wrapped).into_text()),
                    frame.area(),
                );
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let rows_text = (0..height)
            .map(|y| {
                let mut row = String::new();
                for x in 0..width {
                    row.push_str(buffer[(x, y)].symbol());
                }
                row.trim_end().to_string()
            })
            .collect();
        (rows_text, buffer)
    }

    /// Shell block card (`index.tsx:2884-3037`): `$ <cmd>` with the
    /// `cd <workdir> && ` prefix, muted output, stderr in the error color and
    /// the upstream exit-status string; running shows the spinner instead of
    /// `$`.
    #[test]
    fn golden_shell_card_running_and_completed() {
        let theme = Theme::dark();
        let running = make_card(
            "bash",
            "started",
            serde_json::json!({"argv": ["ls", "-la"], "cwd": "src"}),
            None,
        );
        let (rows, buffer) = render(&running, 60, 4);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  ⋯ cd src && ls -la".to_string(),
                "┃".to_string(),
            ]
        );
        assert_eq!(buffer[(0, 2)].fg, theme.border());
        assert_eq!(buffer[(0, 2)].bg, theme.user_message_background());
        assert_eq!(buffer[(3, 2)].symbol(), "⋯");
        assert_eq!(buffer[(3, 2)].fg, theme.text());
        assert_eq!(buffer[(59, 2)].bg, theme.user_message_background());

        let completed = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv": ["ls", "-la"], "cwd": "src"}),
            Some("exit 0\nfile1\nfile2\n\n[stderr]\nwarn: x\n"),
        );
        let (rows, buffer) = render(&completed, 60, 9);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  $ cd src && ls -la".to_string(),
                "┃  Command exited with code 0".to_string(),
                "┃  file1".to_string(),
                "┃  file2".to_string(),
                "┃  [stderr]".to_string(),
                "┃  warn: x".to_string(),
                "┃".to_string(),
            ]
        );
        // stdout is muted, stderr carries `text.feedback.error.base`.
        assert_eq!(buffer[(3, 4)].fg, theme.text_muted());
        assert_eq!(buffer[(3, 4)].bg, theme.user_message_background());
        assert_eq!(buffer[(3, 7)].symbol(), "w");
        assert_eq!(buffer[(3, 7)].fg, theme.error());

        // A missing cwd keeps the bare `$ <cmd>` header.
        let plain = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv": ["pwd"]}),
            Some("exit 0\n/tmp\n"),
        );
        let (rows, _) = render(&plain, 40, 4);
        assert_eq!(rows[2], "┃  $ pwd");
    }

    /// apply_patch update card: `← Patched <path>` with hunk header and
    /// added/removed/context tokens (`index.tsx:3341-3384,3388-3503`).
    #[test]
    fn golden_edit_diff_card_colors() {
        let theme = Theme::dark();
        let patch = "*** Begin Patch\n*** Update File: src/main.rs\n@@ fn main\n ctx line\n-removed\n+added\n*** End Patch";
        let card = make_card(
            "apply_patch",
            "completed",
            serde_json::json!({"patchText": patch}),
            Some("Update src/main.rs (hash_before=a, hash_after=b)"),
        );
        let (rows, buffer) = render(&card, 60, 8);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  ← Patched src/main.rs +1 -1".to_string(),
                "┃  @@ fn main".to_string(),
                "┃   ctx line".to_string(),
                "┃  -removed".to_string(),
                "┃  +added".to_string(),
                "┃".to_string(),
            ]
        );
        // Hunk header (`diff.text.hunkHeader`).
        assert_eq!(buffer[(3, 3)].symbol(), "@");
        assert_eq!(buffer[(3, 3)].fg, theme.diff_hunk_header());
        // Context line: `diff.text.context` on `diff.background.context`.
        assert_eq!(buffer[(4, 4)].symbol(), "c");
        assert_eq!(buffer[(4, 4)].fg, theme.diff_context());
        assert_eq!(buffer[(4, 4)].bg, theme.diff_context_background());
        // Removed line: `diff.text.removed` on `diff.background.removed`.
        assert_eq!(buffer[(3, 5)].symbol(), "-");
        assert_eq!(buffer[(3, 5)].fg, theme.diff_removed());
        assert_eq!(buffer[(3, 5)].bg, theme.diff_removed_background());
        // Added line: `diff.text.added` on `diff.background.added`.
        assert_eq!(buffer[(3, 6)].symbol(), "+");
        assert_eq!(buffer[(3, 6)].fg, theme.diff_added());
        assert_eq!(buffer[(3, 6)].bg, theme.diff_added_background());
        // The `← Patched` label uses the context token.
        assert_eq!(buffer[(4, 2)].fg, theme.diff_context());

        // Running: the upstream `Patching` label plus the already-known hunks.
        let running = make_card(
            "apply_patch",
            "started",
            serde_json::json!({"patchText": patch}),
            None,
        );
        let (rows, _) = render(&running, 60, 8);
        assert_eq!(rows[2], "┃  Patching");
        assert_eq!(rows[3], "┃  ← Patched src/main.rs +1 -1");
    }

    /// apply_patch variants: `# Created` / `← Patched` / `# Deleted` per file
    /// with exact added-file line numbers (`index.tsx:3415-3503`).
    #[test]
    fn golden_apply_patch_created_patched_deleted() {
        let theme = Theme::dark();
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: a.txt\n",
            "+first\n",
            "+second\n",
            "*** Update File: b.txt\n",
            "@@\n",
            "-old\n",
            "+new\n",
            "*** Delete File: c.txt\n",
            "*** End Patch",
        );
        let card = make_card(
            "apply_patch",
            "completed",
            serde_json::json!({"patchText": patch}),
            Some(
                "Add a.txt (hash_before=-, hash_after=x)\nUpdate b.txt (hash_before=y, hash_after=z)\nDelete c.txt (hash_before=w, hash_after=-)",
            ),
        );
        let (rows, buffer) = render(&card, 60, 12);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  # Created a.txt +2".to_string(),
                "┃  1 +first".to_string(),
                "┃  2 +second".to_string(),
                "┃  ← Patched b.txt +1 -1".to_string(),
                "┃  @@".to_string(),
                "┃  -old".to_string(),
                "┃  +new".to_string(),
                "┃  # Deleted c.txt".to_string(),
                "┃".to_string(),
                String::new(),
            ]
        );
        // `# Created` uses the added token; line numbers use
        // `diff.lineNumber.text`.
        assert_eq!(buffer[(3, 2)].symbol(), "#");
        assert_eq!(buffer[(3, 2)].fg, theme.diff_added());
        assert_eq!(buffer[(3, 3)].symbol(), "1");
        assert_eq!(buffer[(3, 3)].fg, theme.diff_line_number());
        assert_eq!(buffer[(3, 3)].bg, theme.diff_added_background());
        // `# Deleted` uses the removed token.
        assert_eq!(buffer[(3, 9)].symbol(), "#");
        assert_eq!(buffer[(3, 9)].fg, theme.diff_removed());

        // A failed patch shows the upstream `# Patch failed` header plus the
        // recorded error text, never a fabricated diff.
        let failed = make_card(
            "apply_patch",
            "failed",
            serde_json::json!({"patchText": patch}),
            Some("error: partial op 1 (b.txt): context mismatch"),
        );
        let (rows, buffer) = render(&failed, 60, 5);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  # Patch failed".to_string(),
                "┃  error: partial op 1 (b.txt): context mismatch".to_string(),
                "┃".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 2)].fg, theme.error());
    }

    /// Inline tools (`message-parts.tsx:176-253`, `index.tsx:2688-2759`):
    /// pending spinner + label, terminal labels, the `↳ Loaded` read row, and
    /// the generic `✓/✗ <tool> <args>` with the error text visible.
    #[test]
    fn golden_inline_tool_running_and_completed() {
        let theme = Theme::dark();
        let running = make_card(
            "read",
            "started",
            serde_json::json!({"path": "src/main.rs"}),
            None,
        );
        let (rows, buffer) = render(&running, 60, 2);
        assert_eq!(rows[1], "   ⋯ Reading file…");
        assert_eq!(buffer[(3, 1)].symbol(), "⋯");
        assert_eq!(buffer[(3, 1)].fg, theme.text());

        let completed = make_card(
            "read",
            "completed",
            serde_json::json!({"path": "src/main.rs"}),
            Some("fn main() {}\n"),
        );
        let (rows, _) = render(&completed, 60, 3);
        assert_eq!(rows[1], "     Read src/main.rs");
        assert_eq!(rows[2], "     ↳ Loaded src/main.rs");

        let glob = make_card(
            "glob",
            "completed",
            serde_json::json!({"pattern": "*.rs", "limit": 10}),
            Some(r#"{"items":["a.rs","b.rs"],"pagination":{"returned":2,"truncated":false}}"#),
        );
        let (rows, _) = render(&glob, 60, 2);
        assert_eq!(rows[1], "     Glob \"*.rs\" (2 matches)");

        let fetch = make_card(
            "webfetch",
            "completed",
            serde_json::json!({"url": "https://example.test/x"}),
            Some("body"),
        );
        let (rows, _) = render(&fetch, 60, 2);
        assert_eq!(rows[1], "     WebFetch https://example.test/x");

        let skill = make_card(
            "skill",
            "completed",
            serde_json::json!({"id": "review"}),
            Some("body"),
        );
        let (rows, _) = render(&skill, 60, 2);
        assert_eq!(rows[1], "     Skill \"review\"");

        // Generic/MCP tool: `✗ <tool> <args>` plus the recorded error.
        let failed = make_card(
            "mcp_server_search",
            "failed",
            serde_json::json!({"query": "needle"}),
            Some("error: mcp transport failure"),
        );
        let (rows, buffer) = render(&failed, 60, 3);
        assert_eq!(rows[1], "   ✗ mcp_server_search query: needle");
        assert_eq!(rows[2], "     error: mcp transport failure");
        assert_eq!(buffer[(3, 1)].symbol(), "✗");
        assert_eq!(buffer[(3, 1)].fg, theme.error());
        assert_eq!(buffer[(5, 2)].fg, theme.error());

        // Denied adds strikethrough (`message-parts.tsx:176-253`).
        let denied = make_card(
            "mcp_server_search",
            "denied",
            serde_json::json!({"query": "needle"}),
            Some("error: denied mcp_server_search"),
        );
        let (_, buffer) = render(&denied, 60, 3);
        assert!(
            buffer[(3, 1)]
                .modifier
                .contains(ratatui::style::Modifier::CROSSED_OUT)
        );
        assert_eq!(buffer[(3, 1)].fg, theme.error());

        // A cancelled inline call is error-colored without strikethrough.
        let cancelled = make_card(
            "read",
            "cancelled",
            serde_json::json!({"path": "src/main.rs"}),
            Some("error: cancelled"),
        );
        let (rows, buffer) = render(&cancelled, 60, 3);
        assert_eq!(rows[1], "     Read src/main.rs");
        assert_eq!(rows[2], "     error: cancelled");
        assert_eq!(buffer[(3, 1)].fg, theme.error());
        assert!(
            !buffer[(3, 1)]
                .modifier
                .contains(ratatui::style::Modifier::CROSSED_OUT)
        );
    }

    /// Error and cancelled block cards keep the recorded error text visible
    /// with the upstream status strings (`index.tsx:2256-2259`).
    #[test]
    fn golden_error_and_cancelled_cards() {
        let theme = Theme::dark();
        let failed = make_card(
            "bash",
            "failed",
            serde_json::json!({"argv": ["false"]}),
            Some("error: invalid arguments for bash: missing argv"),
        );
        let (rows, buffer) = render(&failed, 60, 4);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  $ false".to_string(),
                "┃  error: invalid arguments for bash: missing argv".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 3)].fg, theme.error());

        let cancelled = make_card(
            "bash",
            "cancelled",
            serde_json::json!({"argv": ["sleep", "100"]}),
            Some("error: cancelled"),
        );
        let (rows, buffer) = render(&cancelled, 60, 4);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  $ sleep 100".to_string(),
                "┃  Command cancelled".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 3)].fg, theme.text_muted());

        let timed_out = make_card(
            "bash",
            "failed",
            serde_json::json!({"argv": ["sleep", "100"]}),
            Some("exit signal\n\n[timeout]"),
        );
        let (rows, _) = render(&timed_out, 60, 4);
        assert_eq!(rows[3], "┃  Command timed out");
    }

    /// Subagent card (`index.tsx:3173-3205`): `<Agent> Subagent — <desc> ·
    /// <model>` with the child session/state parsed from the recorded
    /// `<subagent sessionID … state …>` result; pending shows `Delegating…`.
    #[test]
    fn golden_subagent_card() {
        let theme = Theme::dark();
        let running = make_card(
            "subagent",
            "started",
            serde_json::json!({
                "agent": "explore",
                "description": "find configs",
                "prompt": "find the config files",
            }),
            None,
        );
        let (rows, _) = render(&running, 60, 4);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  ↳ Delegating…".to_string(),
                "┃".to_string(),
            ]
        );

        let completed = make_card(
            "subagent",
            "completed",
            serde_json::json!({
                "agent": "explore",
                "description": "find configs",
                "prompt": "find the config files",
                "model": "ludka2/x",
            }),
            Some(
                "<subagent sessionID=\"s-parent-sub-1\" state=\"completed\">\nfound 3 files\n</subagent>",
            ),
        );
        let (rows, buffer) = render(&completed, 60, 6);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  ✓ Explore Subagent — find configs · ludka2/x".to_string(),
                "┃  s-parent-sub-1 · completed".to_string(),
                "┃  found 3 files".to_string(),
                "┃".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 2)].symbol(), "✓");
        assert_eq!(buffer[(3, 2)].fg, theme.text());

        // A failed child shows the upstream-shaped reason in the error color.
        let failed = make_card(
            "subagent",
            "failed",
            serde_json::json!({"agent": "explore", "description": "find configs", "prompt": "x"}),
            Some("error: subagent failed (sessionID: s-parent-sub-2): child crashed"),
        );
        let (rows, buffer) = render(&failed, 60, 5);
        assert_eq!(rows[2], "┃  ✗ Explore Subagent — find configs");
        assert_eq!(rows[3], "┃  s-parent-sub-2");
        assert_eq!(rows[4], "┃  error: child crashed");
        assert_eq!(buffer[(3, 2)].fg, theme.error());
    }

    /// Parsing is honest: unknown tools fall back to the generic renderer and
    /// missing fields stay absent.
    #[test]
    fn render_parsing_never_invents_fields() {
        let card = make_card("mcp_tool", "completed", serde_json::json!({}), Some("ok"));
        match &card.render {
            ToolRender::Inline(InlineRender::Generic { args }) => assert!(args.is_empty()),
            other => panic!("expected generic render, got {other:?}"),
        }
        let card = make_card(
            "read",
            "completed",
            serde_json::json!("not an object"),
            None,
        );
        match &card.render {
            ToolRender::Inline(InlineRender::Read { path }) => assert!(path.is_empty()),
            other => panic!("expected read render, got {other:?}"),
        }
        let card = make_card(
            "apply_patch",
            "completed",
            serde_json::json!({"patchText": "junk"}),
            None,
        );
        match &card.render {
            ToolRender::Patch(patch) => assert!(patch.files.is_empty()),
            other => panic!("expected patch render, got {other:?}"),
        }
        // Pending labels are the exact upstream strings.
        assert_eq!(
            InlineRender::Read {
                path: String::new()
            }
            .pending_label(),
            "Reading file…"
        );
        assert_eq!(
            InlineRender::Glob {
                pattern: String::new(),
                matches: None
            }
            .pending_label(),
            "Finding files…"
        );
        assert_eq!(
            InlineRender::Grep {
                pattern: String::new(),
                matches: None
            }
            .pending_label(),
            "Searching content…"
        );
        assert_eq!(
            InlineRender::WebFetch { url: String::new() }.pending_label(),
            "Fetching from the web…"
        );
        assert_eq!(
            InlineRender::Skill { id: String::new() }.pending_label(),
            "Loading skill…"
        );
    }

    /// `unknown` is a crashed operation, not a running one: it renders with
    /// the error presentation and never with a spinner.
    #[test]
    fn unknown_state_is_an_error_not_a_running_card() {
        let theme = Theme::dark();
        let card = make_card(
            "read",
            "unknown",
            serde_json::json!({"path": "src/main.rs"}),
            None,
        );
        let (rows, buffer) = render(&card, 60, 2);
        assert_eq!(rows[1], "     Read src/main.rs");
        assert_eq!(buffer[(3, 1)].fg, theme.error());
        assert!(!is_running("unknown"));
        assert!(is_error_state("unknown"));
    }

    /// The shell output collapse keeps the newest rows and marks the omission
    /// (`index.tsx:2976,2980-3037`).
    #[test]
    fn shell_output_collapses_with_the_upstream_marker() {
        let mut output = String::from("exit 0\n");
        for index in 0..(TOOL_OUTPUT_LINES + 3) {
            output.push_str(&format!("line {index}\n"));
        }
        let card = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv": ["seq"]}),
            Some(&output),
        );
        let (rows, _) = render(&card, 60, 15);
        assert!(rows.contains(&"┃  [earlier output omitted]".to_string()));
        assert!(!rows.contains(&"┃  line 0".to_string()));
        assert!(rows.contains(&format!("┃  line {}", TOOL_OUTPUT_LINES + 2)));
    }

    /// Unused theme tokens are still resolved (guards against a renderer that
    /// silently drops a cited role).
    #[test]
    fn cited_theme_roles_are_resolved() {
        let theme = Theme::dark();
        for color in [
            theme.diff_added(),
            theme.diff_removed(),
            theme.diff_context(),
            theme.diff_hunk_header(),
            theme.diff_added_background(),
            theme.diff_removed_background(),
            theme.diff_context_background(),
            theme.diff_line_number(),
        ] {
            assert_ne!(color, ratatui::style::Color::Reset);
        }
        assert_ne!(
            theme.markdown(MarkdownToken::Code),
            ratatui::style::Color::Reset
        );
        assert_ne!(
            theme.syntax(SyntaxToken::Keyword),
            ratatui::style::Color::Reset
        );
    }
}
