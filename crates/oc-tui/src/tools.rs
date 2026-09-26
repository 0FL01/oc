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
//!   `(N earlier lines)` for UI collapse, `Command exited with code N` /
//!   finished notices after output (`core/src/shell/result.ts:29-32`,
//!   `core/src/tool/plugin/shell.ts:77-85`);
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
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Block-card inner padding: `padding 1/2` (`index.tsx:2784-2866`).
pub const TOOL_PADDING: usize = 2;
/// Inline icon column (`message-parts.tsx:18,213-219`).
pub const TOOL_ICON_WIDTH: usize = 2;
/// Shared command/output collapse budget (`index.tsx:2980`).
pub const TOOL_OUTPUT_LINES: usize = 10;

/// Static spinner fallback while animations are off
/// (`component/spinner.tsx:29-33`, `spinner-frames.ts:1`).
pub const SPINNER: &str = "⋯";
/// Output collapse marker (`index.tsx:2976`).
pub const EARLIER_OUTPUT_OMITTED: &str = "[earlier output omitted]";
/// Finished shell notice (`core/src/shell/result.ts:65`).
pub const COMMAND_CANCELLED: &str = "Command cancelled.";
/// Finished shell notice (`core/src/shell/result.ts:30`).
pub const COMMAND_TIMED_OUT: &str = "Command timed out before completion.";
/// Finished shell notice (`core/src/shell/result.ts:31`). Native success omits it.
pub fn command_exited(code: i64) -> String {
    format!("Command exited with code {code}.")
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
    /// Process output's terminal newline, before native transport markers.
    pub output_ends_with_newline: bool,
}

/// `apply_patch` card: bounded per-file hunks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatchRender {
    /// Files in patch order (bounded by the adapter caps).
    pub files: Vec<DiffFileRender>,
    /// True when the recorded outcome is an error.
    pub failed: bool,
    /// The durable result confirms only these earlier operations applied.
    pub partial: bool,
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
    /// Retained string bytes, without allocating a rendered/debug copy.
    pub(crate) fn retained_bytes(&self) -> usize {
        let optional = |s: &Option<String>| s.as_ref().map_or(0, String::len);
        match self {
            Self::Shell(s) => {
                s.command.len()
                    + optional(&s.cwd)
                    + s.stdout
                        .iter()
                        .chain(&s.stderr)
                        .map(String::len)
                        .sum::<usize>()
            }
            Self::Patch(p) => p
                .files
                .iter()
                .map(|f| {
                    f.path.len()
                        + optional(&f.move_to)
                        + f.hunks
                            .iter()
                            .map(|h| {
                                optional(&h.anchor)
                                    + h.lines.iter().map(|l| l.text.len()).sum::<usize>()
                            })
                            .sum::<usize>()
                })
                .sum(),
            Self::Subagent(s) => {
                s.agent.len()
                    + s.description.len()
                    + optional(&s.model)
                    + optional(&s.child_session)
                    + optional(&s.child_state)
                    + optional(&s.error)
                    + s.result.iter().map(String::len).sum::<usize>()
            }
            Self::Inline(InlineRender::Read { path }) => path.len(),
            Self::Inline(
                InlineRender::Glob { pattern, .. } | InlineRender::Grep { pattern, .. },
            ) => pattern.len(),
            Self::Inline(InlineRender::WebFetch { url }) => url.len(),
            Self::Inline(InlineRender::Skill { id }) => id.len(),
            Self::Inline(InlineRender::Generic { args }) => {
                args.iter().map(|(k, v)| k.len() + v.len()).sum()
            }
        }
    }

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
    let mut captured = rest;
    loop {
        let next = ["\n[truncated]", "\n[timeout]", "\n[cancelled]"]
            .iter()
            .find_map(|marker| captured.strip_suffix(marker));
        let Some(next) = next else {
            break;
        };
        captured = next;
    }
    render.output_ends_with_newline = captured.ends_with('\n');
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
    let failed = is_error_state(state) || output.is_some_and(|out| out.starts_with("error: "));
    let partial = failed && output.is_some_and(|out| out.starts_with("error: partial op "));
    let mut files = oc_adapters::patch::diff_render(patch);
    if failed {
        // A request is not evidence that it applied. The runner records each
        // committed prefix operation as `done <FileResult>` after the error.
        // Unknown/denied outcomes contain no confirmed diff.
        let mut index = 0;
        files.retain(|file| {
            let current = index;
            index += 1;
            partial
                && output.is_some_and(|out| {
                    let failed_op = out
                        .split_once('(')
                        .and_then(|(prefix, _)| prefix.strip_prefix("error: partial op "))
                        .and_then(|number| number.trim().parse::<usize>().ok());
                    if failed_op.is_none_or(|failed_op| current >= failed_op) {
                        return false;
                    }
                    let verb = match file.change {
                        "Add" => "add",
                        "Delete" => "delete",
                        _ => "update",
                    };
                    let prefix = format!("done {verb} {}", file.path);
                    out.lines().skip(1).any(|line| {
                        line.strip_prefix(&prefix).is_some_and(|rest| {
                            rest.starts_with(" (hash_before=")
                                || file.move_to.as_ref().is_some_and(|target| {
                                    rest.starts_with(&format!(" -> {target} (hash_before="))
                                })
                        })
                    })
                })
        });
    }
    PatchRender {
        files,
        failed,
        partial,
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
    matches!(
        state,
        "failed" | "denied" | "cancelled" | "unknown" | "no_gain"
    )
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

    /// BlockTool explicitly uses background.base (`index.tsx:2804`).
    fn border_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(self.theme.background())
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
    shell_block_expanded(shell, card, theme, width, false)
}

/// Session-local expansion exposes only the recorded preview, never missing bytes.
pub(crate) fn shell_block_expanded(
    shell: &ShellRender,
    card: &ToolCard,
    theme: &Theme,
    width: u16,
    expanded: bool,
) -> Vec<Line> {
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
    let input = header.iter().map(Span::content).collect::<String>();
    let (input, command_lines, _) = collapse_shell_command(&input, width, running);
    let input_chars = input.chars().count();
    let input = if expanded {
        header.iter().map(Span::content).collect::<String>()
    } else {
        input
    };
    let inner = if width == 0 {
        usize::MAX
    } else {
        (width as usize).saturating_sub(3).max(1)
    };
    for raw in input.split('\n') {
        for line in styled::wrap_line_limited(
            &Line::styled(
                raw,
                ratatui::style::Style::default()
                    .fg(theme.text())
                    .bg(frame.bg),
            ),
            inner,
            usize::MAX,
        ) {
            out.push(frame.row(line.spans()));
        }
    }

    let muted = frame.body_style();
    let error = ratatui::style::Style::default()
        .fg(theme.error())
        .bg(frame.bg);
    // Running shell cards show the spinner header only: our runtime records
    // the complete argv before the call appears, so the upstream
    // `Writing command…` placeholder (`index.tsx:3001-3003`, for a command
    // still streaming) has nothing to describe here.
    if !running {
        let mut output = shell_output(shell, card);
        if !expanded {
            let max_lines = TOOL_OUTPUT_LINES.saturating_sub(command_lines + 1).max(1);
            let max_chars = TOOL_OUTPUT_LINES
                * (width as usize)
                    .saturating_sub(6 + usize::from(running) * 2)
                    .max(20);
            let chars = max_chars.saturating_sub(input_chars + 2).max(1);
            collapse_shell_tail(&mut output, max_lines, chars);
        }
        if !output.is_empty() {
            // ShellDisplay's inner box gap=1, only when an output child exists.
            out.push(frame.row(&[]));
        }
        // An error outcome colors the whole recorded output; a successful one
        // keeps stdout muted (`index.tsx:2784-2866` error line).
        let stdout_style = if is_error_state(&card.state) {
            error
        } else {
            muted
        };
        for (line, stderr) in output {
            let line = Line::styled(line, if stderr { error } else { stdout_style });
            for wrapped in styled::wrap_line_limited(&line, inner, usize::MAX) {
                out.push(frame.row(wrapped.spans()));
            }
        }
    }
    out.push(frame.row(&[]));
    out
}

/// Tool/plugin/shell.ts:77–85 returns captured output followed by a notice;
/// ToolPart (:2471–2475) joins those text parts with a newline before collapse.
/// Owner fee0123 overrides original success presentation: a zero exit stays in
/// metadata, without a generated notice or empty-output placeholder. Recorded
/// stdout (including any status prose) remains untouched.
fn shell_output(shell: &ShellRender, card: &ToolCard) -> Vec<(String, bool)> {
    let mut output = shell
        .stdout
        .iter()
        .map(|line| (line.clone(), false))
        .collect::<Vec<_>>();
    if !shell.stderr.is_empty() {
        output.push(("[stderr]".to_string(), false));
        output.extend(shell.stderr.iter().map(|line| (line.clone(), true)));
    }
    let status = if card.state == "unknown" {
        Some("Outcome unknown (operation was interrupted)".to_string())
    } else if card.state == "cancelled" {
        Some(COMMAND_CANCELLED.to_string())
    } else if shell.timed_out {
        Some(COMMAND_TIMED_OUT.to_string())
    } else if let Some(code) = shell.exit.filter(|code| *code != 0) {
        Some(command_exited(code))
    } else if shell.signal {
        Some("exit signal".to_string())
    } else {
        None
    };
    if output.is_empty() && (shell.exit.is_some_and(|code| code != 0) || shell.signal) {
        output.push(("(no output)".to_string(), false));
    }
    if shell.truncated || card.output_truncated {
        output.push(("[truncated]".to_string(), false));
    } else if status.is_some() && !output.is_empty() && shell.output_ends_with_newline {
        // Retain the captured terminal newline before ToolPart's join newline.
        output.push((String::new(), false));
    }
    if let Some(status) = status {
        output.push((status, is_error_state(&card.state) || shell.timed_out));
    }
    output
}

/// Port of util/collapse-tool-output.ts:51–80 (grapheme/display-cell command limit).
fn collapse_shell_command(input: &str, width: u16, running: bool) -> (String, usize, bool) {
    let line_width = (width as usize)
        .saturating_sub(6 + usize::from(running) * 2)
        .max(20);
    let mut visible = Vec::new();
    let mut lines = 1;
    let mut cells = 0;
    for segment in input.graphemes(true) {
        let next = UnicodeWidthStr::width(segment);
        if segment == "\n" || cells + next > line_width {
            if lines >= 2 {
                if cells >= line_width {
                    visible.pop();
                }
                return (format!("{}…", visible.concat()), lines, true);
            }
            lines += 1;
            cells = 0;
        }
        visible.push(segment);
        cells += next;
    }
    (input.to_string(), lines, false)
}

/// Port of collapseTail (:83–95), retaining stderr styles on the visible tail.
fn collapse_shell_tail(output: &mut Vec<(String, bool)>, max_lines: usize, max_chars: usize) {
    let chars = output
        .iter()
        .map(|(line, _)| line.chars().count())
        .sum::<usize>()
        + output.len().saturating_sub(1);
    if output.len() <= max_lines && chars <= max_chars {
        return;
    }
    let count = output
        .len()
        .saturating_sub(max_lines.saturating_sub(1))
        .max(1);
    let label = format!(
        "({count} earlier {})",
        if count == 1 { "line" } else { "lines" }
    );
    output.drain(..count.min(output.len()));
    let available = max_chars.saturating_sub(label.chars().count() + 1);
    let retained = output
        .iter()
        .map(|(line, _)| line.chars().count())
        .sum::<usize>()
        + output.len().saturating_sub(1);
    let mut discard = retained.saturating_sub(available);
    while discard > 0 && !output.is_empty() {
        let len = output[0].0.chars().count();
        if discard > len {
            output.remove(0);
            discard -= len + 1;
        } else {
            output[0].0 = output[0].0.chars().skip(discard).collect();
            discard = 0;
        }
    }
    if available == 0 {
        output.clear();
    }
    output.insert(0, (label, false));
}

pub(crate) fn shell_expandable(card: &ToolCard, width: u16) -> bool {
    let ToolRender::Shell(shell) = &card.render else {
        return false;
    };
    let input = format!(
        "{}{}{}",
        if is_running(&card.state) { "" } else { "$ " },
        shell
            .cwd
            .as_ref()
            .map_or(String::new(), |cwd| format!("cd {cwd} && ")),
        shell.command
    );
    let (input, lines, overflow) = collapse_shell_command(&input, width, is_running(&card.state));
    let output = if is_running(&card.state) {
        Vec::new()
    } else {
        shell_output(shell, card)
    };
    let rows = output.len();
    let chars =
        output.iter().map(|(s, _)| s.chars().count()).sum::<usize>() + rows.saturating_sub(1);
    overflow
        || rows > TOOL_OUTPUT_LINES.saturating_sub(lines + 1).max(1)
        || chars
            > (TOOL_OUTPUT_LINES * (width as usize).saturating_sub(6).max(20))
                .saturating_sub(input.chars().count() + 2)
                .max(1)
}

fn patch_block(patch: &PatchRender, card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    let frame = BlockFrame::new(theme, width);
    let muted = frame.body_style();
    let error = ratatui::style::Style::default()
        .fg(theme.error())
        .bg(frame.bg);
    let mut out = vec![frame.row(&[])];
    if patch.failed {
        let label = if card.state == "unknown" {
            "# Patch outcome unknown"
        } else {
            PATCH_FAILED
        };
        out.push(frame.row(&[Span::styled(label, error)]));
        if patch.partial && !patch.files.is_empty() {
            out.push(frame.row(&[Span::styled("Applied before failure (recorded):", muted)]));
        }
    }
    if !patch.failed && is_running(&card.state) {
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
                    DiffLineKind::Added => ("+", theme.diff_added(), theme.diff_added_background()),
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
    if patch.files.is_empty() && !patch.failed {
        // Malformed or empty payload: keep the recorded outcome text instead
        // of an invented diff.
        for line in card.output_preview.lines() {
            out.push(frame.row(&[Span::styled(line.to_string(), muted)]));
        }
    }
    if patch.failed {
        for line in card.output_preview.lines() {
            if !line.is_empty() {
                out.push(frame.row(&[Span::styled(line.to_string(), error)]));
            }
        }
    }
    if card.output_truncated {
        out.push(frame.row(&[Span::styled(
            "[output preview truncated; full result retained]",
            muted,
        )]));
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
    // Read's Loaded rows require upstream metadata.loaded (:3097–3121).
    // Our recorded operation has no such metadata; a path is not evidence of it.
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
    if card.state == "unknown" {
        out.push(Line::new(vec![
            pad.clone(),
            Span::styled(
                "[outcome unknown]",
                ratatui::style::Style::default().fg(theme.error()),
            ),
        ]));
    }
    if card.output_truncated {
        out.push(Line::new(vec![
            pad,
            Span::styled(
                "[output preview truncated; full result retained]",
                ratatui::style::Style::default().fg(theme.text_muted()),
            ),
        ]));
    }
    out
}

/// Expanded exploration groups show the original inline part immediately
/// below their header, without the standalone card's result summary or gap.
/// Group eligibility is enforced by the transcript before calling this.
pub(crate) fn exploration_member(card: &ToolCard, theme: &Theme) -> Line {
    let ToolRender::Inline(inline) = &card.render else {
        unreachable!("only inline exploration parts are grouped");
    };
    let running = is_running(&card.state);
    let style = ratatui::style::Style::default().fg(theme.text());
    Line::new(vec![
        Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING)),
        Span::styled(if running { SPINNER } else { "→" }, style),
        Span::plain(" "),
        Span::styled(
            if running {
                inline.pending_label().to_string()
            } else {
                inline_label(inline)
            },
            style,
        ),
    ])
}

/// Terminal label for an inline tool (`index.tsx:3084-3168,3548,2620-2669`).
fn inline_label(inline: &InlineRender) -> String {
    match inline {
        InlineRender::Read { path } => format!("Read {path}"),
        // Our glob/grep take no path argument, so the upstream ` in <path>`
        // clause is omitted instead of being invented.
        InlineRender::Glob { pattern, matches } => match matches {
            Some(count) => format!(
                "Glob \"{pattern}\" ({count} {})",
                if *count == 1 { "match" } else { "matches" }
            ),
            None => format!("Glob \"{pattern}\""),
        },
        InlineRender::Grep { pattern, matches } => match matches {
            Some(count) => format!(
                "Grep \"{pattern}\" ({count} {})",
                if *count == 1 { "match" } else { "matches" }
            ),
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
            message_id: None,
            seq: i64::MAX,
            role: "tool".to_string(),
            text: String::new(),
            agent: Some("build".to_string()),
            agent_color_index: None,
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
        assert_eq!(buffer[(0, 2)].fg, theme.background());
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
                "┃".to_string(),
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
        let (rows, _) = render(&plain, 40, 6);
        assert_eq!(rows, ["", "┃", "┃  $ pwd", "┃", "┃  /tmp", "┃"]);

        for (output, expected) in [
            ("exit 0", vec!["", "┃", "┃  $ true", "┃"]),
            (
                "exit 0\nCommand exited with code 0.\n",
                vec![
                    "",
                    "┃",
                    "┃  $ true",
                    "┃",
                    "┃  Command exited with code 0.",
                    "┃",
                ],
            ),
        ] {
            let card = make_card(
                "bash",
                "completed",
                serde_json::json!({"argv":["true"]}),
                Some(output),
            );
            let (rows, _) = render(&card, 60, expected.len() as u16);
            assert_eq!(rows, expected);
            let ToolRender::Shell(shell) = &card.render else {
                panic!("shell")
            };
            assert_eq!(shell.exit, Some(0));
        }
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

    #[test]
    fn v06b_partial_patch_shows_only_confirmed_hunks_and_unknown_is_not_success() {
        let patch = "*** Begin Patch\n*** Add File: done.txt\n+real\n*** Update File: absent.txt\n@@\n-old\n+new\n*** End Patch";
        let failed = make_card(
            "apply_patch",
            "failed",
            serde_json::json!({"patchText":patch}),
            Some(
                "error: partial op 1 (absent.txt): file not found\ndone add done.txt (hash_before=-, hash_after=abc)",
            ),
        );
        let (rows, _) = render(&failed, 78, 12);
        let text = rows.join("\n");
        assert!(text.contains("# Patch failed"), "{text}");
        assert!(
            text.contains("done.txt") && text.contains("+real"),
            "{text}"
        );
        assert!(
            !text.contains("← Patched absent.txt") && !text.contains("+new"),
            "{text}"
        );
        assert!(text.contains("partial op 1"), "{text}");
        let unknown = make_card(
            "apply_patch",
            "unknown",
            serde_json::json!({"patchText":patch}),
            None,
        );
        let (rows, _) = render(&unknown, 78, 10);
        let text = rows.join("\n");
        assert!(
            text.contains("outcome unknown") && !text.contains("# Created"),
            "{text}"
        );
        let shell = make_card(
            "bash",
            "unknown",
            serde_json::json!({"argv":["touch","x"]}),
            None,
        );
        let (rows, _) = render(&shell, 78, 6);
        assert!(rows.join("\n").contains("Outcome unknown"));
    }

    /// Inline tools (`message-parts.tsx:176-253`, `index.tsx:2688-2759`):
    /// pending spinner + label, terminal labels without invented Loaded metadata, and
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
        assert_eq!(rows[1], "   ⋯ Exploring — 1 read");
        assert_eq!(buffer[(3, 1)].symbol(), "⋯");
        assert_eq!(buffer[(3, 1)].fg, theme.text_muted());
        let individual = tool_block(&running, theme, 60)
            .iter()
            .map(|line| {
                line.spans()
                    .iter()
                    .map(|span| span.content())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(individual, ["   ⋯ Reading file…"]);

        let completed = make_card(
            "read",
            "completed",
            serde_json::json!({"path": "src/main.rs"}),
            Some("fn main() {}\n"),
        );
        let (rows, _) = render(&completed, 60, 3);
        assert_eq!(rows[1], "   → Explored — 1 read");
        // The individual renderer is retained for non-collapsed/tool-detail
        // presentation; grouping changes only the transcript projection.
        let individual = tool_block(&completed, theme, 60)
            .iter()
            .map(|line| {
                line.spans()
                    .iter()
                    .map(|span| span.content())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(individual, ["     Read src/main.rs"]);

        let glob = make_card(
            "glob",
            "completed",
            serde_json::json!({"pattern": "*.rs", "limit": 10}),
            Some(r#"{"items":["a.rs","b.rs"],"pagination":{"returned":2,"truncated":false}}"#),
        );
        let (rows, _) = render(&glob, 60, 2);
        assert_eq!(rows[1], "   → Explored — 1 search");

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
    /// with the finished-result notices (`core/src/shell/result.ts:29-32,65`).
    #[test]
    fn golden_error_and_cancelled_cards() {
        let theme = Theme::dark();
        let failed = make_card(
            "bash",
            "failed",
            serde_json::json!({"argv": ["false"]}),
            Some("error: invalid arguments for bash: missing argv"),
        );
        let (rows, buffer) = render(&failed, 60, 5);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  $ false".to_string(),
                "┃".to_string(),
                "┃  error: invalid arguments for bash: missing argv".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 4)].fg, theme.error());

        let cancelled = make_card(
            "bash",
            "cancelled",
            serde_json::json!({"argv": ["sleep", "100"]}),
            Some("error: cancelled"),
        );
        let (rows, buffer) = render(&cancelled, 60, 7);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  $ sleep 100".to_string(),
                "┃".to_string(),
                "┃  error: cancelled".to_string(),
                "┃  Command cancelled.".to_string(),
                "┃".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 4)].fg, theme.error());
        assert_eq!(buffer[(3, 5)].fg, theme.error());

        let timed_out = make_card(
            "bash",
            "failed",
            serde_json::json!({"argv": ["sleep", "100"]}),
            Some("exit signal\n\n[timeout]"),
        );
        let (rows, _) = render(&timed_out, 60, 7);
        assert_eq!(rows[5], "┃  Command timed out before completion.");

        let exited = make_card(
            "bash",
            "failed",
            serde_json::json!({"argv":["false"]}),
            Some("exit 7\nfailed output\n"),
        );
        let (rows, _) = render(&exited, 60, 8);
        assert_eq!(
            &rows[4..7],
            ["┃  failed output", "┃", "┃  Command exited with code 7."]
        );
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
        // Genuine printf output captured in bounded-shell-06/upstream/
        // bounded-shell-completed.txt:19–25. Native argv stays unquoted.
        // Owner fee0123 omits the original notice and its blank separator:
        // native retains lines 35–40 (34 omitted), original 37–40 (36 omitted).
        let mut output = String::from("exit 0\n");
        for index in 1..=40 {
            output.push_str(&format!("SHELL-LINE-{index:02}\n"));
        }
        let argv = std::iter::once("printf".to_string())
            .chain(std::iter::once("SHELL-LINE-%02d\\n".to_string()))
            .chain((1..=40).map(|i| i.to_string()))
            .collect::<Vec<_>>();
        let card = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv": argv}),
            Some(&output),
        );
        let (rows, _) = render(&card, 114, 14);
        assert_eq!(
            &rows[5..13],
            [
                "┃  (34 earlier lines)",
                "┃  SHELL-LINE-35",
                "┃  SHELL-LINE-36",
                "┃  SHELL-LINE-37",
                "┃  SHELL-LINE-38",
                "┃  SHELL-LINE-39",
                "┃  SHELL-LINE-40",
                "┃"
            ]
        );
        assert!(
            !rows.join("\n").contains("'SHELL-LINE"),
            "do not fake shell quoting for native argv"
        );
        let history = [tool_row(&card)];
        let cache = std::cell::RefCell::new(crate::messages::MarkdownCache::default());
        for expanded in [false, true, false] {
            let full = crate::messages::transcript_with_expansion(
                &history,
                Theme::dark(),
                114,
                120,
                |_| Theme::dark().text(),
                Some(&cache),
                &|_| expanded,
            );
            let (visible, total) = crate::messages::visible_transcript_expanded(
                &history,
                Theme::dark(),
                (114, 120),
                (100, 0, None),
                |_| Theme::dark().text(),
                &cache,
                &|_| expanded,
            );
            let full = full
                .iter()
                .flat_map(|line| {
                    styled::wrap_line(line, 114)
                        .into_iter()
                        .map(|wrapped| wrapped.with_style(line.style()))
                })
                .collect::<Vec<_>>();
            assert_eq!(total, full.len() + 1);
            assert_eq!(&visible[1..], full.as_slice());
            let text = full
                .iter()
                .map(Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(text.contains("SHELL-LINE-01"), expanded);
            assert_eq!(text.contains("(34 earlier lines)"), !expanded);
            assert!(!text.contains("Command exited with code 0."));
            let tail = full
                .iter()
                .rev()
                .take(4)
                .map(|line| line.plain_text().trim_end().to_string())
                .collect::<Vec<_>>();
            assert_eq!(
                tail,
                [
                    "┃",
                    "┃  SHELL-LINE-40",
                    "┃  SHELL-LINE-39",
                    "┃  SHELL-LINE-38"
                ]
            );
        }
    }

    #[test]
    fn vis16_shell_expansion_hit_and_indexed_render_agree() {
        use crate::messages::{
            MarkdownCache, exploration_header_at, transcript_with_expansion,
            visible_transcript_expanded,
        };
        use std::cell::RefCell;
        let theme = Theme::dark();
        let output = format!(
            "exit 0\n{}\n[truncated]\n",
            (0..18)
                .map(|i| format!("retained line {i}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut card = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv":["seq"]}),
            Some(&output),
        );
        card.output_truncated = true;
        let rows = [tool_row(&card)];
        let cache = RefCell::new(MarkdownCache::default());
        for width in [40, 80] {
            assert!(shell_expandable(&card, width));
            for expanded in [false, true, false] {
                let full = transcript_with_expansion(
                    &rows,
                    theme,
                    width,
                    width,
                    |_| theme.text(),
                    Some(&cache),
                    &|_| expanded,
                );
                let full: Vec<_> = full
                    .iter()
                    .flat_map(|line| {
                        styled::wrap_line(line, width as usize)
                            .into_iter()
                            .map(|wrapped| wrapped.with_style(line.style()))
                    })
                    .collect();
                let (visible, total) = visible_transcript_expanded(
                    &rows,
                    theme,
                    (width, width),
                    (100, 0, None),
                    |_| theme.text(),
                    &cache,
                    &|_| expanded,
                );
                assert_eq!(total, full.len() + 1);
                assert_eq!(&visible[1..], full.as_slice(), "styled full/indexed rows");
                assert_eq!(
                    visible
                        .iter()
                        .skip(1)
                        .map(Line::plain_text)
                        .collect::<Vec<_>>(),
                    full.iter().map(Line::plain_text).collect::<Vec<_>>()
                );
                let text = full
                    .iter()
                    .map(Line::plain_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert_eq!(text.contains("retained line 0"), expanded);
                assert!(text.contains("retained line 17") && text.contains("[truncated]"));
                assert!(!text.contains("Command exited with code 0."));
                assert_eq!(
                    exploration_header_at(
                        &rows,
                        theme,
                        (width, width),
                        (100, 0, None),
                        |_| theme.text(),
                        &cache,
                        (&|_| expanded, (5, 3))
                    ),
                    Some(card.op.clone())
                );
                assert_eq!(
                    exploration_header_at(
                        &rows,
                        theme,
                        (width, width),
                        (100, 0, None),
                        |_| theme.text(),
                        &cache,
                        (&|_| expanded, (5, 1))
                    ),
                    None
                );
                let hover = crate::messages::hover_tool_content(&visible[3], theme);
                assert_eq!(
                    crate::messages::tool_hover_range(&visible, theme, 3),
                    2..visible.len()
                );
                assert_eq!(hover.plain_text(), visible[3].plain_text());
                assert_eq!(
                    hover.style().bg,
                    Some(theme.decrease(theme.background_raised()))
                );
                assert_eq!(hover.spans()[0].style().fg, Some(theme.background()));
                for scroll in [0, 5] {
                    let (page, count) = visible_transcript_expanded(
                        &rows,
                        theme,
                        (width, width),
                        (4, scroll, None),
                        |_| theme.text(),
                        &cache,
                        &|_| expanded,
                    );
                    let end = total - scroll.min(total.saturating_sub(4));
                    assert_eq!(count, total);
                    assert_eq!(page.as_slice(), &visible[end.saturating_sub(4)..end]);
                }
            }
        }
        let short = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv":["true"]}),
            Some("exit 0"),
        );
        assert!(!shell_expandable(&short, 80));
        let long = make_card(
            "bash",
            "completed",
            serde_json::json!({"argv":["界".repeat(80)]}),
            Some("exit 0"),
        );
        assert!(shell_expandable(&long, 40));
        let ToolRender::Shell(shell) = &long.render else {
            panic!("shell")
        };
        let collapsed = shell_block_expanded(shell, &long, theme, 40, false);
        let expanded = shell_block_expanded(shell, &long, theme, 40, true);
        assert!(collapsed.iter().any(|line| line.plain_text().contains('…')));
        assert!(expanded.len() > collapsed.len());
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
