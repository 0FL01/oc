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

pub(crate) mod execute;

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
    /// Canonical `shell` or compatible legacy `bash` command card.
    Shell(ShellRender),
    /// `apply_patch` diff card.
    Patch(PatchRender),
    /// `subagent` child-session card.
    Subagent(SubagentRender),
    /// Typed, operation-associated DCP snapshot; no input/output text parsing.
    Dcp(Box<crate::dcp_view::DcpRender>),
    /// Short-lived tools and generic/MCP calls.
    Inline(InlineRender),
}

/// Shell card fields parsed from recorded command/workdir or legacy argv/cwd.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellRender {
    /// Command source as recorded (legacy display joins argv only for rendering).
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
    /// Cancellation from the native owner's structured outcome, not body prose.
    pub cancelled: bool,
    /// Process output's terminal newline, before native transport markers.
    pub output_ends_with_newline: bool,
    /// Standalone user command, not the model-facing Shell result renderer.
    pub direct_user: bool,
    /// Current owned process still runs, independently of a returned tool part.
    pub live_running: bool,
    pub process_state: Option<String>,
    /// Actual captured conversion; persists with terminal presentation facts.
    pub background: bool,
    pub command_limited: bool,
    pub diagnostic: Option<String>,
}

/// Legacy `apply_patch` request preview. Confirmed effects live on `ToolCard`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatchRender {
    /// Requested files in patch order (bounded by the adapter caps).
    pub files: Vec<DiffFileRender>,
    /// True when the recorded outcome is an error.
    pub failed: bool,
}

/// `subagent` card fields parsed from the recorded request and result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentRender {
    /// View-only pending glyph; never parsed from arguments or stored history.
    pub spinner: Option<&'static str>,
    /// Explicit recorded continuation input, not a returned session id.
    pub continuation: bool,
    /// Current existing supervisor inventory; logical completion is independent.
    pub live_running: bool,
    /// Human name for the explicit recorded model; unavailable names fall back.
    pub model_label: Option<String>,
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
        /// Valid recorded requested start and optional inclusive end.
        range: Option<(u64, Option<u64>)>,
    },
    /// `glob` tool.
    Glob {
        /// Recorded pattern.
        pattern: String,
        /// Explicit search root, if supplied in the recorded call.
        path: String,
        /// Returned match count from the recorded pagination.
        matches: Option<usize>,
    },
    /// `grep` tool.
    Grep {
        /// Recorded pattern.
        pattern: String,
        /// Explicit search root, if supplied in the recorded call.
        path: String,
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
    /// Count from a validated, complete question input for this operation.
    /// Argument-stream fragments never supply a count or an answer result.
    Question { count: Option<usize> },
    /// Structured execute facts are owned by the card, never reparsed from RAW.
    Execute,
    /// Any other tool (MCP included): `key: value` arguments.
    Generic {
        /// Available recorded parameters, bounded independently of the summary.
        args: Vec<(String, String)>,
        /// Pinned primitive summary within the aggregate argument preview budget.
        summary: String,
        arguments_limited: bool,
    },
}

/// Short approval resource label budget; Generic summaries use PREVIEW_BYTES.
pub const GENERIC_ARG_CHARS: usize = 120;

impl ToolRender {
    /// Retained string bytes, without allocating a rendered/debug copy.
    pub(crate) fn retained_bytes(&self) -> usize {
        let optional = |s: &Option<String>| s.as_ref().map_or(0, String::len);
        match self {
            Self::Dcp(view) => view.retained_bytes(),
            Self::Shell(s) => {
                s.command.len()
                    + optional(&s.cwd)
                    + optional(&s.diagnostic)
                    + optional(&s.process_state)
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
                    + optional(&s.model_label)
                    + optional(&s.child_session)
                    + optional(&s.child_state)
                    + optional(&s.error)
                    + s.result.iter().map(String::len).sum::<usize>()
            }
            Self::Inline(InlineRender::Read { path, .. }) => path.len(),
            Self::Inline(
                InlineRender::Glob { pattern, path, .. } | InlineRender::Grep { pattern, path, .. },
            ) => pattern.len() + path.len(),
            Self::Inline(InlineRender::WebFetch { url }) => url.len(),
            Self::Inline(InlineRender::Skill { id }) => id.len(),
            Self::Inline(InlineRender::Question { .. } | InlineRender::Execute) => 0,
            Self::Inline(InlineRender::Generic { args, summary, .. }) => {
                summary.len() + args.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
            }
        }
    }

    /// Parse the presentation data of one recorded tool operation.
    ///
    /// Unknown or unparsable inputs produce the generic renderer over the
    /// bounded argument preview; nothing is fabricated.
    pub fn parse(name: &str, input: Option<&str>, output: Option<&str>, state: &str) -> Self {
        if name == "compress" {
            return Self::Dcp(Box::default());
        }
        if name == "question" {
            let count = if state == "argument_stream" {
                None
            } else {
                input
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .and_then(|value| oc_core::question::QuestionInput::parse(&value).ok())
                    .map(|input| input.questions.len())
            };
            return Self::Inline(InlineRender::Question { count });
        }
        let value = input.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
        match name {
            "shell" | "bash" => ToolRender::Shell(shell_render(name, value.as_ref(), output)),
            "apply_patch" => ToolRender::Patch(patch_render(value.as_ref(), output, state)),
            "edit" | "write" => ToolRender::Patch(Default::default()),
            "subagent" => ToolRender::Subagent(subagent_render(value.as_ref(), output, state)),
            "read" => ToolRender::Inline(InlineRender::Read {
                path: string_arg(value.as_ref(), "path"),
                range: read_range(value.as_ref()),
            }),
            "glob" => ToolRender::Inline(InlineRender::Glob {
                pattern: string_arg(value.as_ref(), "pattern"),
                path: string_arg(value.as_ref(), "path"),
                matches: returned_count(output),
            }),
            "grep" => ToolRender::Inline(InlineRender::Grep {
                pattern: string_arg(value.as_ref(), "pattern"),
                path: string_arg(value.as_ref(), "path"),
                matches: returned_count(output),
            }),
            "webfetch" => ToolRender::Inline(InlineRender::WebFetch {
                url: string_arg(value.as_ref(), "url"),
            }),
            "skill" => ToolRender::Inline(InlineRender::Skill {
                id: string_arg(value.as_ref(), "id"),
            }),
            _ => ToolRender::Inline(generic_args(value.as_ref())),
        }
    }
}

impl InlineRender {
    fn icon(&self) -> &'static str {
        match self {
            Self::Glob { .. } | Self::Grep { .. } => "✱",
            Self::WebFetch { .. } => "%",
            Self::Read { .. } | Self::Skill { .. } | Self::Question { .. } => "→",
            Self::Execute | Self::Generic { .. } => SPINNER,
        }
    }

    /// Upstream pending label (`index.tsx:3072,3083,3107,3130,3142,3191,3547`).
    pub fn pending_label(&self) -> &'static str {
        match self {
            InlineRender::Read { .. } => "Reading file…",
            InlineRender::Glob { .. } => "Finding files…",
            InlineRender::Grep { .. } => "Searching content…",
            InlineRender::WebFetch { .. } => "Fetching from the web…",
            InlineRender::Skill { .. } => "Loading skill…",
            InlineRender::Question { .. } => "Asking questions…",
            InlineRender::Execute => "execute",
            InlineRender::Generic { .. } => "",
        }
    }
}

fn read_range(value: Option<&serde_json::Value>) -> Option<(u64, Option<u64>)> {
    let value = value?.as_object()?;
    if !value.contains_key("offset") && !value.contains_key("limit") {
        return None;
    }
    let positive = |key| match value.get(key) {
        None => Some(None),
        Some(value) => value.as_u64().filter(|value| *value > 0).map(Some),
    };
    // Invalid supplied fields must not acquire defaults and invent a range.
    let start = positive("offset")?.unwrap_or(1);
    let end = match positive("limit")? {
        Some(limit) => Some(start.checked_add(limit - 1)?),
        None => None,
    };
    Some((start, end))
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

fn generic_args(value: Option<&serde_json::Value>) -> InlineRender {
    let object = value.and_then(serde_json::Value::as_object);
    let mut args = Vec::new();
    let mut primitive = Vec::new();
    let mut remaining = oc_core::tool_output::PREVIEW_BYTES;
    let mut arguments_limited = false;
    for (key, value) in object.into_iter().flatten() {
        if key.len() > remaining {
            arguments_limited = true;
            break;
        }
        remaining -= key.len();
        let text = value.as_str().map_or_else(
            || serde_json::to_string_pretty(value).expect("JSON value serialization"),
            str::to_owned,
        );
        let end = text.floor_char_boundary(text.len().min(remaining));
        arguments_limited |= end < text.len();
        if value.is_string() || value.is_number() || value.is_boolean() {
            primitive.push(format!("{key}={}", &text[..end]));
        }
        args.push((key.clone(), text[..end].to_owned()));
        remaining -= end;
    }
    // OC2 includes primitive values without shortening each key/value. Use the
    // same admitted, aggregate-bounded parameters as expansion, not model prose
    // or a second unbounded input copy. Separators also consume the summary budget.
    let summary = if primitive.is_empty() {
        String::new()
    } else {
        // Normalize after enclosing the summary: trailing value whitespace is
        // an interior separator before ']', and must not disappear.
        let text = format!("[{}]", primitive.join(", "))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let inner = &text[1..text.len() - 1];
        let kept = crate::truncate_utf8(inner, oc_core::tool_output::PREVIEW_BYTES - 2);
        arguments_limited |= kept.len() < inner.len();
        format!("[{kept}]")
    };
    InlineRender::Generic {
        args,
        summary,
        arguments_limited,
    }
}

fn shell_render(
    name: &str,
    value: Option<&serde_json::Value>,
    output: Option<&str>,
) -> ShellRender {
    let command = if name == "shell" {
        string_arg(value, "command")
    } else {
        value
            .and_then(|value| value.get("argv"))
            .and_then(|value| value.as_array())
            .map(|argv| {
                argv.iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    };
    let cwd = value
        .and_then(|value| value.get(if name == "shell" { "workdir" } else { "cwd" }))
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
    // Legacy marker-like text has no trustworthy loss/status provenance. Keep
    // it as recorded data; modern native outcomes supply separate stream facts.
    render.output_ends_with_newline = rest.ends_with('\n');
    let (stdout, stderr) = match rest.split_once("\n[stderr]\n") {
        Some((stdout, stderr)) => (stdout, Some(stderr)),
        None => (rest, None),
    };
    render.stdout = stdout.lines().map(str::to_owned).collect();
    if let Some(stderr) = stderr {
        render.stderr = stderr.lines().map(str::to_owned).collect();
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
    let files = if failed {
        Vec::new()
    } else {
        oc_adapters::patch::diff_render(patch)
    };
    PatchRender { files, failed }
}

fn subagent_render(
    value: Option<&serde_json::Value>,
    output: Option<&str>,
    state: &str,
) -> SubagentRender {
    let mut render = SubagentRender {
        continuation: value
            .and_then(|value| value.get("sessionID"))
            .and_then(|value| value.as_str())
            .is_some_and(|id| !id.is_empty()),
        agent: value
            .and_then(|value| value.get("agent").or_else(|| value.get("subagent_type")))
            .and_then(|value| value.as_str())
            .unwrap_or("General")
            .to_owned(),
        description: string_arg(value, "description"),
        model: value
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        ..SubagentRender::default()
    };
    // Result text remains data, never state, identity or navigation authority.
    render.model_label = render
        .model
        .as_deref()
        .and_then(|value| subagent_model_label(value, None));
    render.error = matches!(state, "failed" | "unknown" | "cancelled")
        .then(|| output.unwrap_or_default().to_owned());
    render
}

pub(crate) fn subagent_model_label(
    value: &str,
    picker: Option<&crate::picker::ModelPicker>,
) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let (reference, variant) = value
        .split_once('#')
        .map_or((value, None), |(reference, variant)| {
            (reference, Some(variant))
        });
    let name = reference
        .split_once('/')
        .and_then(|(provider, id)| {
            picker
                .filter(|picker| picker.provider() == provider)
                .and_then(|picker| picker.model_name(id))
        })
        .unwrap_or(reference);
    Some(
        if let Some(variant) = variant.filter(|variant| !variant.is_empty()) {
            format!("{name} ({variant})")
        } else {
            name.to_owned()
        },
    )
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

/// Clock membership follows the renderer's spinner branches, not just the
/// operation state: live shells/children may outlive their original call.
pub(crate) fn tool_spinner_animated(card: &ToolCard) -> bool {
    if matches!(card.render, ToolRender::Subagent(_)) {
        return subagent_animated(card);
    }
    if card.state == "permission_pending" {
        return match &card.render {
            ToolRender::Inline(
                InlineRender::Read { .. }
                | InlineRender::Glob { .. }
                | InlineRender::Grep { .. }
                | InlineRender::WebFetch { .. }
                | InlineRender::Question { .. },
            ) => false,
            ToolRender::Inline(InlineRender::Execute) => execute::facts(card).is_some(),
            _ => true,
        };
    }
    if card.state == "argument_stream" {
        return !matches!(card.render, ToolRender::Inline(InlineRender::Execute))
            || execute::facts(card).is_some();
    }
    if card.question.is_some() {
        return false;
    }
    match &card.render {
        ToolRender::Shell(shell) => {
            is_running(&card.state) && shell.process_state.is_none() || shell.live_running
        }
        ToolRender::Patch(patch) => {
            card.patch_effects.is_none() && !patch.failed && is_running(&card.state)
        }
        ToolRender::Inline(InlineRender::Question { .. }) | ToolRender::Dcp(_) => false,
        ToolRender::Inline(InlineRender::Execute) => {
            execute::facts(card).is_some() && execute::running(card)
        }
        ToolRender::Inline(InlineRender::Generic { .. }) => is_running(&card.state),
        ToolRender::Inline(_) => is_running(&card.state) || card.state == "running",
        ToolRender::Subagent(_) => unreachable!("handled above"),
    }
}

/// Render one tool card as transcript rows. `width == 0` is the unbounded
/// text projection (no background padding).
pub fn tool_block(card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    let mut lines = tool_block_inner(card, theme, width);
    if tool_spinner_animated(card) {
        let (row, byte) = match &card.render {
            ToolRender::Shell(_) => (1, 5),
            ToolRender::Patch(_) if card.name == "write" && card.state == "argument_stream" => {
                (0, 3)
            }
            ToolRender::Patch(_) => (1, 5),
            ToolRender::Subagent(_) => (0, if width != 0 && width <= 5 { 0 } else { 3 }),
            ToolRender::Inline(_) | ToolRender::Dcp(_) => (0, 3),
        };
        mark_tool_animation(&mut lines, row, byte, card);
    }
    lines
}

fn mark_tool_animation(lines: &mut [Line], row: usize, byte: usize, card: &ToolCard) {
    if tool_spinner_animated(card)
        && let Some(line) = lines.get_mut(row)
    {
        *line = std::mem::take(line).animate(byte, styled::AnimationSource::Tool(card.op.clone()));
    }
}

fn tool_block_inner(card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    if let ToolRender::Dcp(view) = &card.render
        && !matches!(
            card.state.as_str(),
            "argument_stream" | "permission_pending"
        )
    {
        return crate::dcp_view::block(view, &card.state, &card.output_preview, theme, width);
    }
    if card.name == "apply_patch"
        && card.state == "denied"
        && card
            .output_preview
            .starts_with("The user declined this tool call")
        && card
            .patch_effects
            .as_ref()
            .is_none_or(|effects| effects.files.is_empty())
    {
        let frame = BlockFrame::new(theme, width);
        let title = if card.files.len() == 1 && !card.files_truncated {
            format!("{PATCH_FAILED} {}", card.files[0])
        } else {
            PATCH_FAILED.to_string()
        };
        let mut lines = vec![
            frame.row(&[]),
            frame.row(&[Span::styled(title, frame.body_style().fg(theme.error()))]),
            frame.row(&[]),
        ];
        lines.extend(card.output_preview.lines().map(|line| {
            frame.row(&[Span::styled(
                line.to_string(),
                frame.body_style().fg(theme.text_muted()),
            )])
        }));
        lines.push(frame.row(&[]));
        return lines;
    }
    if card.state == "permission_pending" {
        if let ToolRender::Subagent(subagent) = &card.render {
            return subagent_block(subagent, card, theme, width);
        }
        if card.name == "apply_patch" {
            let frame = BlockFrame::new(theme, width);
            let path = if card.files.len() == 1 && !card.files_truncated {
                format!(" {}", card.files[0])
            } else {
                String::new()
            };
            return vec![
                frame.row(&[]),
                frame.row(&[Span::styled(
                    format!("{} Patching{path}", card.spinner()),
                    frame.body_style().fg(theme.warning()),
                )]),
                frame.row(&[]),
            ];
        }
        if let ToolRender::Inline(inline) = &card.render {
            return inline_rows(inline, card, theme);
        }
        let frame = BlockFrame::new(theme, width);
        let label = match &card.render {
            ToolRender::Shell(shell) => format!("$ {}", shell.command),
            _ => card.display_name().to_string(),
        };
        return vec![
            frame.row(&[]),
            frame.row(&[Span::styled(
                format!("{} {label}", card.spinner()),
                frame.body_style().fg(theme.warning()),
            )]),
            frame.row(&[]),
        ];
    }
    if card.state == "argument_stream" {
        return argument_stream_block(card, theme, width);
    }
    if card.question.is_none()
        && let ToolRender::Inline(inline @ InlineRender::Question { .. }) = &card.render
    {
        return inline_rows(inline, card, theme);
    }
    if let Some(result) = &card.question {
        let mut frame = BlockFrame::new(theme, width);
        // BlockTool's layout padding has no child text foreground. Question
        // and answer roles apply only to their real text, including wrap spaces.
        frame.padding = ratatui::style::Style::default().bg(frame.bg);
        // Question's BlockTool has a muted title, separate question/answer
        // children and one-row gaps (`index.tsx:3519-3533`).
        let mut rows = vec![
            frame.row(&[]),
            frame.row(&[Span::styled("# Questions", frame.body_style())]),
            frame.row(&[]),
        ];
        let inner = if width == 0 {
            usize::MAX
        } else {
            (width as usize).saturating_sub(3).max(1)
        };
        for (index, question) in result.questions.iter().enumerate() {
            if index > 0 {
                rows.push(frame.row(&[]));
            }
            let answer = result
                .answers
                .get(index)
                .filter(|answers| !answers.is_empty())
                .map_or_else(|| "(no answer)".into(), |answers| answers.join(", "));
            for (text, style) in [
                (question.question.as_str(), frame.body_style()),
                (answer.as_str(), frame.body_style().fg(theme.text())),
            ] {
                for raw in text.split('\n') {
                    let safe: String = raw
                        .chars()
                        .map(|c| if c.is_control() { ' ' } else { c })
                        .collect();
                    for line in styled::wrap_source_space_line_limited(
                        &Line::styled(safe, style),
                        inner,
                        usize::MAX,
                    ) {
                        rows.push(frame.row(line.spans()));
                    }
                }
            }
        }
        rows.push(frame.row(&[]));
        return rows;
    }
    match &card.render {
        ToolRender::Shell(shell) => shell_block(shell, card, theme, width),
        ToolRender::Patch(patch) => {
            if let Some(effects) = &card.patch_effects {
                crate::patch_view::render(effects, card, theme, width)
            } else if card.name == "write" && card.state == "completed" {
                crate::patch_view::render(
                    &oc_core::patch::PatchEffects::default(),
                    card,
                    theme,
                    width,
                )
            } else {
                patch_block(patch, card, theme, width)
            }
        }
        ToolRender::Subagent(subagent) => subagent_block(subagent, card, theme, width),
        ToolRender::Dcp(view) => {
            crate::dcp_view::block(view, &card.state, &card.output_preview, theme, width)
        }
        ToolRender::Inline(inline @ InlineRender::Generic { .. }) => {
            generic_block_expanded(inline, card, theme, width, false)
        }
        ToolRender::Inline(inline) => inline_rows(inline, card, theme),
    }
}

/// Streaming arguments: upstream ToolPart passes an empty input until the call
/// is complete (`index.tsx:2416`), so each kind shows only its pending label.
/// The bounded raw argument preview is never displayed or parsed.
fn argument_stream_block(card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    let title = match &card.render {
        ToolRender::Subagent(subagent) => return subagent_block(subagent, card, theme, width),
        ToolRender::Shell(shell) => return shell_block(shell, card, theme, width),
        // GenericTool fallback: spinner and the bare tool name (`index.tsx:2567`).
        ToolRender::Dcp(_) => {
            let generic = InlineRender::Generic {
                args: Vec::new(),
                summary: String::new(),
                arguments_limited: false,
            };
            return generic_block_expanded(&generic, card, theme, width, false);
        }
        ToolRender::Inline(inline) => return inline_rows(inline, card, theme),
        // Write (`index.tsx:3016`) is an InlineTool without a path.
        ToolRender::Patch(_) if card.name == "write" => {
            let style = ratatui::style::Style::default().fg(theme.text_muted());
            return vec![Line::new(vec![
                Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING)),
                Span::styled(format!("{:<TOOL_ICON_WIDTH$}", card.spinner()), style),
                Span::styled("Preparing write…", style),
            ])];
        }
        // Edit and ApplyPatch are spinner-titled BlockTools (`index.tsx:3327,3440`).
        ToolRender::Patch(_) if card.name == "edit" => "Preparing edit…",
        ToolRender::Patch(_) => "Patching",
    };
    let frame = BlockFrame::new(theme, width);
    vec![
        frame.row(&[]),
        frame.row(&[Span::styled(
            format!("{} {title}", card.spinner()),
            frame.body_style(),
        )]),
        frame.row(&[]),
    ]
}

/// Block frame: left `┃` border, raised background, padding 1/2
/// (`index.tsx:2784-2866`).
struct BlockFrame<'t> {
    theme: &'t Theme,
    bg: ratatui::style::Color,
    padding: ratatui::style::Style,
    width: usize,
}

impl BlockFrame<'_> {
    fn new(theme: &Theme, width: u16) -> BlockFrame<'_> {
        BlockFrame {
            theme,
            // Block tools sit on `background.raised.base`
            // (`index.tsx:2784-2866`), the same surface as user rows.
            bg: theme.background_raised(),
            padding: ratatui::style::Style::default()
                .fg(theme.text_muted())
                .bg(theme.background_raised()),
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
        all.push(Span::styled(" ".repeat(TOOL_PADDING), self.padding));
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
    let mut frame = BlockFrame::new(theme, width);
    // ShellDisplay and its BlockTool parent do not give box padding the
    // captured output child's muted foreground (`index.tsx:2795–2804`).
    frame.padding = ratatui::style::Style::default().bg(frame.bg);
    let mut out = vec![frame.row(&[])];
    let running = is_running(&card.state) && shell.process_state.is_none() || shell.live_running;
    let admitted =
        is_running(&card.state) && shell.process_state.is_none() && card.output_preview.is_empty();
    if shell.command.is_empty() && (card.state == "argument_stream" || admitted) {
        // ShellDisplay's commandless fallback (`index.tsx:2941-2947`).
        let text = ratatui::style::Style::default()
            .fg(theme.text())
            .bg(frame.bg);
        out.push(frame.row(&[
            Span::styled(card.spinner(), text),
            Span::styled(" ", frame.padding),
            Span::styled("Writing command…", text),
        ]));
        out.push(frame.row(&[]));
        mark_tool_animation(&mut out, 1, 5, card);
        return out;
    }
    let mut header = vec![Span::styled(
        if running {
            format!("{} ", card.spinner())
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
    let input = if shell.command_limited {
        format!("{input} · Input preview")
    } else {
        input
    };
    let input = match capture_status(card) {
        Some(status) => format!("{input} · {status}"),
        None => input,
    };
    let inner = if width == 0 {
        usize::MAX
    } else {
        (width as usize).saturating_sub(3).max(1)
    };
    for (raw_index, raw) in input.split('\n').enumerate() {
        let source = Line::styled(
            raw,
            ratatui::style::Style::default()
                .fg(theme.text())
                .bg(frame.bg),
        );
        let rows = if expanded {
            styled::wrap_line_limited(&source, inner, usize::MAX)
        } else {
            // ShellDisplay's collapsed command uses wrapMode="char", not
            // word-wrap; do not drop its genuine continuation separator.
            styled::wrap_char_line_limited(&source, inner, usize::MAX)
        };
        for (wrapped_index, line) in rows.into_iter().enumerate() {
            if shell.direct_user
                && running
                && raw_index == 0
                && wrapped_index == 0
                && let Some(command) = line
                    .plain_text()
                    .strip_prefix(&format!("{} ", card.spinner()))
            {
                // Spinner and command are separate children; their layout gap
                // inherits the block foreground, not the command's text color.
                out.push(
                    frame.row(&[
                        Span::styled(
                            card.spinner(),
                            ratatui::style::Style::default()
                                .fg(theme.text())
                                .bg(frame.bg),
                        ),
                        Span::styled(" ", frame.padding),
                        Span::styled(
                            command,
                            ratatui::style::Style::default()
                                .fg(theme.text())
                                .bg(frame.bg),
                        ),
                    ]),
                );
            } else {
                out.push(frame.row(line.spans()));
            }
        }
    }

    let muted = frame.body_style();
    let error = ratatui::style::Style::default()
        .fg(theme.error())
        .bg(frame.bg);
    // Only an owner-associated preview may add live output. A converted process
    // keeps the original completed tool part, and hides output until expanded.
    if (!running || shell.direct_user || shell.live_running) && (!shell.background || expanded) {
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
        // A nonzero process exit is still muted ShellDisplay output, not a
        // BlockTool error. Native `failed` covers both cases; the exit separates them.
        let output_style = if is_error_state(&card.state)
            && !shell.direct_user
            && (card.state != "failed" || shell.exit.is_none())
        {
            error
        } else {
            muted
        };
        for (line, feedback_error) in output {
            let line = Line::styled(line, if feedback_error { error } else { output_style });
            for wrapped in styled::wrap_line_limited(&line, inner, usize::MAX) {
                out.push(frame.row(wrapped.spans()));
            }
        }
    }
    if shell.background && !shell.direct_user {
        out.push(frame.row(&[]));
        out.push(
            frame.row(&[Span::styled(
                " Background ",
                ratatui::style::Style::default()
                    .fg(theme.text_muted())
                    .bg(theme.decrease(frame.bg)),
            )]),
        );
    }
    out.push(frame.row(&[]));
    if running {
        mark_tool_animation(&mut out, 1, 5, card);
    }
    out
}

/// Tool/plugin/shell.ts:77–85 returns captured output followed by a notice;
/// ToolPart (:2471–2475) joins those text parts with a newline before collapse.
/// Owner fee0123 overrides original success presentation: a zero exit stays in
/// metadata, without a generated notice or empty-output placeholder. Recorded
/// stdout (including any status prose) remains untouched.
fn shell_output(shell: &ShellRender, card: &ToolCard) -> Vec<(String, bool)> {
    if (shell.direct_user
        || shell.live_running
        || shell.background
        || shell.process_state.is_some())
        && let Some(output) = card.output_presentation.as_ref().filter(|p| p.is_valid())
    {
        // Body is the capture owner's safe publication-order preview, not an
        // envelope to parse. Both streams are muted in pinned ShellDisplay.
        let mut rows = output
            .body
            .lines()
            .map(|line| (line.to_owned(), false))
            .collect::<Vec<_>>();
        append_shell_status(shell, card, &mut rows);
        return rows;
    }
    let mut output = shell
        .stdout
        .iter()
        .map(|line| (line.clone(), false))
        .collect::<Vec<_>>();
    if !shell.stderr.is_empty() {
        output.push(("[stderr]".to_string(), false));
        output.extend(shell.stderr.iter().map(|line| (line.clone(), false)));
    }
    append_shell_status(shell, card, &mut output);
    output
}

fn append_shell_status(shell: &ShellRender, card: &ToolCard, output: &mut Vec<(String, bool)>) {
    let mut status_error = is_error_state(&card.state) || shell.timed_out;
    let status = if card.state == "unknown" || shell.process_state.as_deref() == Some("unknown") {
        Some("Outcome unknown (operation was interrupted)".to_string())
    } else if shell.process_state.as_deref() == Some("failed")
        && shell.exit.is_none()
        && !shell.signal
    {
        Some("Command failed".to_string())
    } else if card.state == "cancelled" || shell.cancelled {
        Some(COMMAND_CANCELLED.to_string())
    } else if shell.timed_out {
        Some(COMMAND_TIMED_OUT.to_string())
    } else if let Some(code) = shell.exit.filter(|code| *code != 0) {
        status_error = false;
        Some(command_exited(code))
    } else if shell.signal {
        Some("exit signal".to_string())
    } else {
        None
    };
    if !shell.direct_user
        && output.is_empty()
        && (shell.exit.is_some_and(|code| code != 0) || shell.signal)
    {
        output.push(("(no output)".to_string(), false));
    }
    if status.is_some() && !output.is_empty() && shell.output_ends_with_newline {
        // Retain the captured terminal newline before ToolPart's join newline.
        output.push((String::new(), false));
    }
    if let Some(status) = status {
        let status = if shell.direct_user {
            if shell.cancelled || card.state == "cancelled" {
                "Command cancelled".into()
            } else if shell.timed_out {
                "Command timed out".into()
            } else {
                status.trim_end_matches('.').into()
            }
        } else {
            status
        };
        output.push((status, status_error));
    }
    if let Some(diagnostic) = &shell.diagnostic {
        output.push((diagnostic.clone(), true));
    }
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

/// Port of collapseTail (:83–95), retaining per-row feedback on the visible tail.
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
    if shell.live_running || shell.background {
        return true;
    }
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
    shell.direct_user
        || overflow
        || rows > TOOL_OUTPUT_LINES.saturating_sub(lines + 1).max(1)
        || chars
            > (TOOL_OUTPUT_LINES * (width as usize).saturating_sub(6).max(20))
                .saturating_sub(input.chars().count() + 2)
                .max(1)
}

fn patch_block(patch: &PatchRender, card: &ToolCard, theme: &Theme, width: u16) -> Vec<Line> {
    let mut frame = BlockFrame::new(theme, width);
    if patch.failed {
        // BlockTool's layout padding inherits the canvas foreground; the
        // failed header and actual error detail have independent text roles.
        frame.padding = ratatui::style::Style::default().bg(frame.bg);
    }
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
    }
    if !patch.failed && is_running(&card.state) {
        // Upstream running label (`index.tsx:3496`, §6).
        out.push(
            frame.row(&[Span::styled(
                format!("{} Patching", card.spinner()),
                ratatui::style::Style::default()
                    .fg(theme.text())
                    .bg(frame.bg),
            )]),
        );
    }
    for file in &patch.files {
        let (label, color) = ("Request preview (not confirmed):", theme.text_muted());
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
    if patch.failed && card.output_preview.lines().any(|line| !line.is_empty()) {
        // Pinned BlockTool separates the header and real error child by one
        // row; ApplyPatch keeps the header red but uses muted error detail.
        out.push(frame.row(&[]));
        for line in card.output_preview.lines() {
            if !line.is_empty() {
                out.push(frame.row(&[Span::styled(line.to_string(), muted)]));
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
    subagent_block_expanded(subagent, card, theme, width, false)
}

pub(crate) fn subagent_failed(card: &ToolCard) -> bool {
    matches!(card.state.as_str(), "failed" | "unknown")
        && !matches!(&card.render,ToolRender::Subagent(render) if render.live_running)
}

pub(crate) fn subagent_running(view: &SubagentRender, card: &ToolCard) -> bool {
    let terminal = card.child_job.as_ref().is_some_and(|job| {
        !matches!(
            job.state,
            oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
        )
    });
    view.live_running || (!terminal && matches!(card.state.as_str(), "started" | "running"))
}

pub(crate) fn subagent_pending(view: &SubagentRender, card: &ToolCard) -> bool {
    matches!(
        card.state.as_str(),
        "argument_stream" | "permission_pending"
    ) || view.description.is_empty()
}

pub(crate) fn subagent_animated(card: &ToolCard) -> bool {
    matches!(&card.render, ToolRender::Subagent(view)
        if !subagent_pending(view, card) && !view.continuation && subagent_running(view, card))
}

/// InlineTool/InlineToolRow, including an indivisible base-surface status badge.
/// Error detail is the bounded recorded error, not a child result transcript.
pub(crate) fn subagent_block_expanded(
    subagent: &SubagentRender,
    card: &ToolCard,
    theme: &Theme,
    width: u16,
    expanded: bool,
) -> Vec<Line> {
    let running = subagent_running(subagent, card);
    // Native background results retain the recorded `running` operation state
    // after returning the handle. Positive result bytes distinguish that real
    // publication from an unfinished foreground await; never parse its prose.
    let handle_returned =
        card.state == "completed" || (card.state == "running" && card.output_bytes > 0);
    let background = handle_returned && card.child_job.as_ref().is_some_and(|job| job.background);
    let failed = subagent_failed(card);
    let denied = card.state == "denied";
    let style = ratatui::style::Style::default().fg(
        if card.state == "permission_pending" || card.state == "cancelled" {
            theme.warning()
        } else if failed {
            theme.error()
        } else {
            theme.text_muted()
        },
    );
    let style = if denied {
        style.add_modifier(ratatui::style::Modifier::CROSSED_OUT)
    } else {
        style
    };
    let pending = subagent_pending(subagent, card);
    let icon = if pending || running && !subagent.continuation {
        subagent.spinner.unwrap_or(SPINNER)
    } else if subagent.continuation {
        "↳"
    } else if handle_returned {
        "✓"
    } else {
        "│"
    };
    let label = if pending {
        "Delegating…".to_owned()
    } else {
        let actor = if subagent.continuation {
            "Continue subagent".to_owned()
        } else {
            format!(
                "{} Subagent",
                crate::messages::Locale::titlecase(&subagent.agent)
            )
        };
        let description = if subagent.description.is_empty() {
            "Subagent"
        } else {
            &subagent.description
        };
        let mut label = format!("{actor} — {description}");
        if let Some(model) = &subagent.model_label {
            label.push_str(&format!(" · {model}"));
        }
        label
    };
    let label = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let indent = crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH;
    let label = Line::new(vec![Span::styled(label, style)]);
    let mut out = if width == 0 || width as usize > indent {
        let labels = if width == 0 {
            vec![label]
        } else {
            styled::wrap_source_space_line_limited(&label, width as usize - indent, usize::MAX)
        };
        labels
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let mut spans = vec![
                    Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING)),
                    if index == 0 {
                        Span::styled(icon, style)
                    } else {
                        Span::plain(" ".repeat(TOOL_ICON_WIDTH - 1))
                    },
                    // The pinned width-two icon box styles only the glyph;
                    // its remaining layout cell does not inherit its roles.
                    if ((pending && card.state != "permission_pending")
                        || running && !subagent.continuation)
                        && !background
                        && icon == SPINNER
                    {
                        Span::styled(" ", style)
                    } else {
                        Span::plain(" ")
                    },
                ];
                spans.extend(line.spans().iter().cloned());
                Line::new(spans)
            })
            .collect::<Vec<_>>()
    } else {
        styled::wrap_line(
            &Line::new(vec![
                Span::styled(icon, style),
                Span::plain(" "),
                Span::styled(label.plain_text(), style),
            ]),
            width as usize,
        )
    };
    // The mode belongs to the captured returned launch and persists after exit;
    // current child running state above is deliberately a different fact.
    if background {
        let badge = Span::styled(
            " Background ",
            ratatui::style::Style::default()
                .fg(theme.text_muted())
                .bg(theme.decrease(theme.background())),
        );
        // The wrapped caption occupies the full flex-item width, not merely
        // the used cells on its final line (InlineToolRow).
        let one_line = out.len() == 1;
        let last = out.last_mut().expect("inline header");
        if width == 0
            || one_line
                && last.spans().iter().map(styled::span_width).sum::<usize>()
                    + 1
                    + styled::span_width(&badge)
                    <= width as usize
        {
            let mut spans = last.spans().to_vec();
            spans.push(Span::plain(" "));
            spans.push(badge);
            *last = Line::new(spans);
        } else {
            out.push(Line::new(vec![
                Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH)),
                badge,
            ]));
        }
    }
    if failed
        && expanded
        && let Some(error) = &subagent.error
    {
        let pad = " ".repeat(crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH);
        for text in error.lines() {
            let text = text
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect::<String>();
            let line = Line::new(vec![Span::styled(text, style)]);
            let lines = if width == 0 {
                vec![line]
            } else {
                styled::wrap_line(&line, (width as usize).saturating_sub(indent).max(1))
            };
            for line in lines {
                let mut spans = vec![Span::plain(pad.clone())];
                spans.extend(line.spans().iter().cloned());
                out.push(Line::new(spans));
            }
        }
    }
    if (subagent_animated(card) || pending && subagent.spinner.is_some())
        && let Some(line) = out.first_mut()
    {
        *line = std::mem::take(line).animate(
            if width != 0 && width as usize <= indent {
                0
            } else {
                3
            },
            styled::AnimationSource::Tool(card.op.clone()),
        );
    }
    out
}

fn inline_rows(inline: &InlineRender, card: &ToolCard, theme: &Theme) -> Vec<Line> {
    inline_rows_expanded(inline, card, theme, 0, false)
}

fn inline_rows_expanded(
    inline: &InlineRender,
    card: &ToolCard,
    theme: &Theme,
    width: u16,
    expanded: bool,
) -> Vec<Line> {
    if matches!(inline, InlineRender::Execute) {
        return execute::block(card, theme, 0, &|_| false)
            .into_iter()
            .map(|row| row.line)
            .collect();
    }
    if matches!(inline, InlineRender::Generic { .. }) {
        return generic_block_expanded(inline, card, theme, 0, false);
    }
    let pad = Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING));
    // Streaming arguments render as the pending spinner label (`index.tsx:2416`).
    let running =
        is_running(&card.state) || matches!(card.state.as_str(), "running" | "argument_stream");
    let waiting = card.state == "permission_pending";
    let question =
        matches!(inline, InlineRender::Question { .. }) && card.state != "argument_stream";
    let cancelled_question =
        card.state == "cancelled" && matches!(inline, InlineRender::Question { count: Some(_) });
    let exploration = matches!(
        inline,
        InlineRender::Read { .. }
            | InlineRender::Glob { .. }
            | InlineRender::Grep { .. }
            | InlineRender::WebFetch { .. }
    );
    // Pinned TUI treats a cancelled question as a denied inline row, not a
    // web "Questions dismissed" notice. Use the validated input/state, never
    // words in output; turn cancellation shares this outcome with form dismissal.
    let denied = (exploration || matches!(inline, InlineRender::Skill { .. }))
        && card.state == "denied"
        || cancelled_question;
    let failed = is_error_state(&card.state) && !denied;
    let style = if waiting {
        ratatui::style::Style::default().fg(theme.warning())
    } else if failed {
        ratatui::style::Style::default().fg(theme.error())
    } else {
        ratatui::style::Style::default().fg(theme.text_muted())
    };
    let generic = matches!(inline, InlineRender::Generic { .. });
    // Named file tools retain their actual glyph in terminal/error states.
    let icon = if question {
        "→".to_string()
    } else if waiting {
        match inline {
            InlineRender::Read { .. } => "→",
            InlineRender::Glob { .. } | InlineRender::Grep { .. } => "✱",
            InlineRender::WebFetch { .. } => "⊙",
            _ => card.spinner(),
        }
        .to_string()
    } else if running {
        card.spinner().to_string()
    } else if exploration || matches!(inline, InlineRender::Skill { .. }) {
        inline.icon().to_string()
    } else if generic {
        if failed {
            "✗".to_string()
        } else {
            "✓".to_string()
        }
    } else {
        String::new()
    };
    // Preserve other failures/cancellations; only the typed question cancellation
    // joins the denied strike-through (`index.tsx:2712-2737`, message-parts:176-253).
    let style = if card.state == "denied" || cancelled_question {
        style.add_modifier(ratatui::style::Modifier::CROSSED_OUT)
    } else {
        style
    };
    let label = if question {
        inline_label(inline)
    } else if running {
        let mut label = String::new();
        if generic {
            label.push_str(card.display_name());
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
            label.push_str(card.display_name());
            label.push(' ');
        }
        label.push_str(&inline_label(inline));
        label
    };
    let mut spans = vec![pad.clone()];
    let spinning = !question && (running || waiting && !exploration);
    if !spinning {
        // A named glyph's remaining icon-column cell is layout, not FG/strike.
        spans.push(Span::styled(icon, style));
        spans.push(Span::plain(" ".repeat(TOOL_ICON_WIDTH - 1)));
    } else {
        spans.push(Span::styled(format!("{icon:<TOOL_ICON_WIDTH$}"), style));
    }
    spans.push(Span::styled(label, style));
    let mut out = vec![Line::new(spans)];
    if spinning {
        mark_tool_animation(&mut out, 0, crate::messages::MESSAGE_PADDING, card);
    }
    // Read's Loaded rows require upstream metadata.loaded (:3097–3121).
    // Our recorded operation has no such metadata; a path is not evidence of it.
    if failed && (!exploration || expanded || card.state == "unknown") {
        for line in card.output_preview.lines().take(TOOL_OUTPUT_LINES) {
            if line.is_empty() {
                continue;
            }
            let body = Line::new(vec![Span::styled(
                line.to_string(),
                ratatui::style::Style::default().fg(theme.error()),
            )]);
            let lines = if width == 0 {
                vec![body]
            } else {
                styled::wrap_line(
                    &body,
                    usize::from(width)
                        .saturating_sub(crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH)
                        .max(1),
                )
            };
            for body in lines {
                out.push(Line::new(
                    std::iter::once(Span::plain(
                        " ".repeat(crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH),
                    ))
                    .chain(body.spans().iter().cloned())
                    .collect(),
                ));
            }
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
    out
}

/// Expanded exploration groups show the original inline part immediately
/// below their header, without the standalone card's result summary or gap.
/// Group eligibility is enforced by the transcript before calling this.
pub(crate) fn exploration_member(
    card: &ToolCard,
    theme: &Theme,
    width: u16,
    expanded: bool,
) -> Vec<Line> {
    let ToolRender::Inline(inline) = &card.render else {
        unreachable!("only inline exploration parts are grouped");
    };
    inline_rows_expanded(inline, card, theme, width, expanded)
}

pub(crate) fn inline_error_expandable(card: &ToolCard) -> bool {
    matches!(
        card.render,
        ToolRender::Inline(
            InlineRender::Read { .. }
                | InlineRender::Glob { .. }
                | InlineRender::Grep { .. }
                | InlineRender::WebFetch { .. }
        )
    ) && matches!(card.state.as_str(), "failed" | "cancelled")
        && !card.output_preview.is_empty()
}

/// Terminal label for an inline tool (`index.tsx:3084-3168,3548,2620-2669`).
fn inline_label(inline: &InlineRender) -> String {
    match inline {
        InlineRender::Read { path, range } => {
            let suffix = range.map_or_else(String::new, |(start, end)| {
                format!(
                    ":{start}-{}",
                    end.map_or_else(String::new, |end| end.to_string())
                )
            });
            format!("Read {path}{suffix}")
        }
        InlineRender::Glob {
            pattern,
            path,
            matches,
        }
        | InlineRender::Grep {
            pattern,
            path,
            matches,
        } => {
            let name = if matches!(inline, InlineRender::Glob { .. }) {
                "Glob"
            } else {
                "Grep"
            };
            let path = if path.is_empty() {
                String::new()
            } else {
                format!(" in {path}")
            };
            let count = matches
                .filter(|count| *count > 0)
                .map(|count| {
                    format!(
                        " ({count} {})",
                        if count == 1 { "match" } else { "matches" }
                    )
                })
                .unwrap_or_default();
            format!("{name} \"{pattern}\"{path}{count}")
        }
        InlineRender::WebFetch { url } => format!("WebFetch {url}"),
        InlineRender::Skill { id } => format!("Skill \"{id}\""),
        InlineRender::Question { count: Some(count) } => format!(
            "Asked {count} question{}",
            if *count == 1 { "" } else { "s" }
        ),
        InlineRender::Question { count: None } => "Asking questions…".into(),
        InlineRender::Execute => "execute".into(),
        InlineRender::Generic { summary, .. } => summary.clone(),
    }
}

/// Shared inline geometry for generic and compress operation headers.
pub(crate) fn inline_tool_header(
    label: &str,
    icon: &str,
    style: ratatui::style::Style,
    label_style: ratatui::style::Style,
    width: u16,
) -> Vec<Line> {
    let padding = crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH;
    let available = if width == 0 {
        usize::MAX
    } else {
        (width as usize).saturating_sub(padding).max(1)
    };
    // A fitting source separator belongs to the preceding wrapped row; empty
    // canvas cells keep their default style, as in upstream InlineToolRow.
    styled::wrap_source_space_line_limited(&Line::styled(label, label_style), available, usize::MAX)
        .iter()
        .enumerate()
        .map(|(index, line)| {
            // Canonical neutral padding also matches indexed row reconstruction.
            let mut spans = vec![Span::plain(" ".repeat(if index == 0 {
                crate::messages::MESSAGE_PADDING
            } else {
                padding
            }))];
            if index == 0 {
                spans.push(Span::styled(icon, style));
                spans.push(Span::plain(" "));
            }
            spans.extend_from_slice(line.spans());
            Line::new(spans)
        })
        .collect()
}

/// The original inline header is the click/hover surface; expanded parameters
/// and output are text, not a second button or an implicit artifact reader.
pub(crate) fn generic_block_expanded(
    inline: &InlineRender,
    card: &ToolCard,
    theme: &Theme,
    width: u16,
    expanded: bool,
) -> Vec<Line> {
    let InlineRender::Generic { args, summary, .. } = inline else {
        return Vec::new();
    };
    let failed = is_error_state(&card.state);
    let style = ratatui::style::Style::default().fg(if card.state == "permission_pending" {
        theme.warning()
    } else if failed && card.state != "denied" {
        theme.error()
    } else {
        theme.text_muted()
    });
    let label_style = if card.state == "denied" {
        style.add_modifier(ratatui::style::Modifier::CROSSED_OUT)
    } else {
        style
    };
    let icon = if is_running(&card.state)
        || matches!(
            card.state.as_str(),
            "permission_pending" | "argument_stream"
        ) {
        card.spinner()
    } else if failed {
        "✗"
    } else {
        "✓"
    };
    let mut label = card.display_name().to_string();
    if !summary.is_empty() {
        label.push(' ');
        label.push_str(summary);
    }
    if let Some(status) = capture_status(card) {
        label.push_str(" · ");
        label.push_str(status);
    }
    if card.state == "unknown" {
        label.push_str(" · outcome unknown");
    }
    let mut rows = inline_tool_header(&label, icon, style, label_style, width);
    mark_tool_animation(&mut rows, 0, 3, card);
    if !expanded {
        return rows;
    }
    let padding = crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH;
    let mut detail = |key: &str, value: &str, color| {
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{key}: ")
        };
        let prefix_width = UnicodeWidthStr::width(prefix.as_str());
        let available = if width == 0 {
            usize::MAX
        } else {
            (width as usize)
                .saturating_sub(padding + prefix_width)
                .max(1)
        };
        let mut first = true;
        for logical in value.split('\n') {
            for line in styled::wrap_line_limited(
                &Line::styled(logical, ratatui::style::Style::default().fg(color)),
                available,
                usize::MAX,
            ) {
                let mut spans = vec![
                    Span::plain(" ".repeat(padding)),
                    if first {
                        Span::styled(
                            prefix.clone(),
                            ratatui::style::Style::default().fg(theme.text_muted()),
                        )
                    } else {
                        // GenericTool's continuation indent is layout, not key text.
                        Span::plain(" ".repeat(prefix_width))
                    },
                ];
                spans.extend_from_slice(line.spans());
                rows.push(Line::new(spans));
                first = false;
            }
        }
    };
    // OC2's InlineTool opens failure detail instead of GenericTool parameters;
    // denied is muted/struck through but its normal onClick still opens input.
    if failed && card.state != "denied" {
        let output = card.output_preview.trim();
        if !output.is_empty() {
            detail("", output, theme.error());
        }
    } else {
        for (key, value) in args {
            detail(key, value, theme.text());
        }
        let output = card.output_preview.trim();
        if !output.is_empty() {
            detail("output", output, theme.text());
        }
    }
    rows
}

pub(crate) fn generic_expandable(card: &ToolCard) -> bool {
    card.question.is_none()
        && matches!(&card.render, ToolRender::Inline(InlineRender::Generic { args, .. })
        if !args.is_empty() || !card.output_preview.trim().is_empty())
}

fn capture_status(card: &ToolCard) -> Option<&'static str> {
    use oc_core::tool_output::CaptureState;
    let presentation = card.output_presentation.as_ref()?;
    match presentation.capture.as_ref().map(|capture| capture.state) {
        Some(CaptureState::Active) => Some("Capturing output"),
        Some(CaptureState::Expired) => Some("Saved output expired · /cards"),
        Some(CaptureState::ProducerLimited) => Some("Producer limited · /cards"),
        Some(CaptureState::Complete) if presentation.producer_limited == Some(true) => {
            Some("Producer limited · /cards")
        }
        Some(CaptureState::Complete) => None,
        Some(_) => Some("Capture incomplete · /cards"),
        None if presentation.producer_limited == Some(true) => Some("Producer limited · /cards"),
        None => None,
    }
}

/// Recorded projection/capture facts, not a promise to read a cold artifact.
pub(crate) fn presentation_details(
    presentation: &oc_core::tool_output::Presentation,
) -> Vec<String> {
    let mut details = Vec::new();
    if presentation.body_limited {
        details.push(format!(
            "Body preview: {} / {} bytes",
            presentation.body.len(),
            presentation.body_bytes
        ));
    }
    if let Some(capture) = &presentation.capture {
        details.push(format!(
            "Capture {:?}: {} / {} bytes, {} / {} lines",
            capture.state,
            capture.retained_bytes,
            capture.admitted_bytes,
            capture.retained_lines,
            capture.admitted_lines
        ));
        details.push(format!(
            "Recorded reference (not a readability guarantee): {}",
            capture.reference.as_deref().unwrap_or("unavailable")
        ));
    } else if presentation.producer_limited == Some(true) {
        details.push("Producer limited; capture availability unknown".into());
    }
    details
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
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 0,
            op: "op-1".to_string(),
            name: name.to_string(),
            state: state.to_string(),
            input: Some(input.to_string()),
            output: output.map(str::to_string),
            output_bytes: output.map(str::len).unwrap_or(0) as i64,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        })
    }

    fn tool_row(card: &ToolCard) -> HistoryRow {
        HistoryRow {
            position: None,
            message_id: None,
            seq: i64::MAX,
            role: "tool".to_string(),
            text: String::new().into(),
            agent: Some("build".to_string()),
            agent_color_index: None,
            chips: Vec::<Chip>::new(),
            reasoning: None,
            child_notice: None,
            shell_notice: None,
            meta: None,
            tool: Some(std::sync::Arc::new(card.clone())),
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
    /// `cd <workdir> && ` prefix, muted stdout/stderr and
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
        // ShellDisplay keeps both captured streams muted.
        assert_eq!(buffer[(3, 4)].fg, theme.text_muted());
        assert_eq!(buffer[(3, 4)].bg, theme.user_message_background());
        assert_eq!(buffer[(3, 7)].symbol(), "w");
        assert_eq!(buffer[(3, 7)].fg, theme.text_muted());

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

    /// Legacy request previews are never labeled as confirmed mutations.
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
                "┃  Request preview (not confirmed): src/main.rs +1 -1".to_string(),
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
        assert_eq!(buffer[(4, 2)].fg, theme.text_muted());

        // Running: the upstream `Patching` label plus the already-known hunks.
        let running = make_card(
            "apply_patch",
            "started",
            serde_json::json!({"patchText": patch}),
            None,
        );
        let (rows, _) = render(&running, 60, 8);
        assert_eq!(rows[2], "┃  ⋯ Patching");
        assert_eq!(
            rows[3],
            "┃  Request preview (not confirmed): src/main.rs +1 -1"
        );
    }

    /// Legacy multi-file requests retain preview coordinates without success labels.
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
                "┃  Request preview (not confirmed): a.txt +2".to_string(),
                "┃  1 +first".to_string(),
                "┃  2 +second".to_string(),
                "┃  Request preview (not confirmed): b.txt +1 -1".to_string(),
                "┃  @@".to_string(),
                "┃  -old".to_string(),
                "┃  +new".to_string(),
                "┃  Request preview (not confirmed): c.txt".to_string(),
                "┃".to_string(),
                String::new(),
            ]
        );
        // Unconfirmed headers are muted; preview line numbers retain their role.
        assert_eq!(buffer[(3, 2)].symbol(), "R");
        assert_eq!(buffer[(3, 2)].fg, theme.text_muted());
        assert_eq!(buffer[(3, 3)].symbol(), "1");
        assert_eq!(buffer[(3, 3)].fg, theme.diff_line_number());
        assert_eq!(buffer[(3, 3)].bg, theme.diff_added_background());
        // A requested delete is not presented as a confirmed deletion.
        assert_eq!(buffer[(3, 9)].symbol(), "R");
        assert_eq!(buffer[(3, 9)].fg, theme.text_muted());

        // A failed patch shows the upstream `# Patch failed` header plus the
        // recorded error text, never a fabricated diff.
        let failed = make_card(
            "apply_patch",
            "failed",
            serde_json::json!({"patchText": patch}),
            Some("error: partial op 1 (b.txt): context mismatch"),
        );
        let (rows, buffer) = render(&failed, 60, 6);
        assert_eq!(
            rows,
            vec![
                String::new(),
                "┃".to_string(),
                "┃  # Patch failed".to_string(),
                "┃".to_string(),
                "┃  error: partial op 1 (b.txt): context mismatch".to_string(),
                "┃".to_string(),
            ]
        );
        assert_eq!(buffer[(3, 2)].fg, theme.error());
        assert_eq!(buffer[(3, 4)].fg, theme.text_muted());
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
            text.contains("done.txt") && !text.contains("+real") && !text.contains("# Created"),
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
        assert_eq!(rows[1], "   ⋯ Exploring: 1 read");
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
        assert_eq!(rows[1], "   → Explored: 1 read");
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
        assert_eq!(individual, ["   → Read src/main.rs"]);

        let glob = make_card(
            "glob",
            "completed",
            serde_json::json!({"pattern": "*.rs", "limit": 10}),
            Some(r#"{"items":["a.rs","b.rs"],"pagination":{"returned":2,"truncated":false}}"#),
        );
        let (rows, _) = render(&glob, 60, 2);
        assert_eq!(rows[1], "   → Explored: 1 search");

        let fetch = make_card(
            "webfetch",
            "completed",
            serde_json::json!({"url": "https://example.test/x"}),
            Some("body"),
        );
        let (rows, _) = render(&fetch, 60, 2);
        assert_eq!(rows[1], "   → Explored: 1 fetch");
        assert_eq!(
            tool_block(&fetch, theme, 60)[0].plain_text(),
            "   % WebFetch https://example.test/x"
        );

        let skill = make_card(
            "skill",
            "completed",
            serde_json::json!({"id": "review"}),
            Some("body"),
        );
        let (rows, _) = render(&skill, 60, 2);
        assert_eq!(rows[1], "   → Skill \"review\"");

        // Generic/MCP tool: `✗ <tool> <args>` plus the recorded error.
        let failed = make_card(
            "mcp_server_search",
            "failed",
            serde_json::json!({"query": "needle"}),
            Some("error: mcp transport failure"),
        );
        let (rows, buffer) = render(&failed, 60, 3);
        assert_eq!(rows[1], "   ✗ mcp_server_search [query=needle]");
        assert_eq!(
            rows[2], "",
            "failure detail starts collapsed, not hidden permanently"
        );
        assert_eq!(buffer[(3, 1)].symbol(), "✗");
        assert_eq!(buffer[(3, 1)].fg, theme.error());
        let ToolRender::Inline(inline) = &failed.render else {
            panic!("generic failure")
        };
        let expanded = generic_block_expanded(inline, &failed, theme, 60, true);
        assert_eq!(
            expanded[1].plain_text(),
            "     error: mcp transport failure"
        );
        assert_eq!(
            expanded[1].spans().last().unwrap().style().fg,
            Some(theme.error())
        );

        // Denied strikes the label only; the fixed icon column keeps its muted
        // foreground and no strikethrough (`message-parts.tsx:199-238`).
        let denied = make_card(
            "mcp_server_search",
            "denied",
            serde_json::json!({"query": "needle"}),
            Some("error: denied mcp_server_search"),
        );
        let (_, buffer) = render(&denied, 60, 3);
        assert!(
            buffer[(5, 1)]
                .modifier
                .contains(ratatui::style::Modifier::CROSSED_OUT)
        );
        assert!(
            !buffer[(3, 1)]
                .modifier
                .contains(ratatui::style::Modifier::CROSSED_OUT)
        );
        assert_eq!(buffer[(3, 1)].fg, theme.text_muted());

        // A cancelled inline call is error-colored without strikethrough.
        let cancelled = make_card(
            "read",
            "cancelled",
            serde_json::json!({"path": "src/main.rs"}),
            Some("error: cancelled"),
        );
        let (rows, _) = render(&cancelled, 60, 3);
        assert_eq!(rows[1], "   → Explored: 1 read");
        let member = exploration_member(&cancelled, theme, 60, true);
        assert_eq!(member[0].plain_text(), "   → Read src/main.rs");
        assert_eq!(member[1].plain_text(), "     error: cancelled");
        assert_eq!(member[0].spans()[1].style().fg, Some(theme.error()));
        assert!(
            !member[0].spans()[1]
                .style()
                .add_modifier
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

        let mut presentation = oc_core::tool_output::Presentation::new("", 0, false);
        presentation.shell = Some(oc_core::tool_output::Shell {
            background: false,
            process_state: None,
            stdout: String::new(),
            stderr: String::new(),
            stdout_limited: false,
            stderr_limited: false,
            exit: None,
            signal: Some(15),
            timed_out: true,
            cancelled: false,
        });
        let timed_out = card_from_row(&ToolOpView {
            child_job: None,
            output_presentation: Some(Box::new(presentation)),
            question: None,
            rowid: 1,
            op: "timeout".into(),
            name: "bash".into(),
            state: "failed".into(),
            input: Some(serde_json::json!({"argv": ["sleep", "100"]}).to_string()),
            output: Some("exit signal\n\n[timeout]".into()),
            output_bytes: 23,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
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

    /// Subagent InlineTool (`index.tsx:3173-3205`): no result blocks or prose
    /// identity. Logical pending/completed/error use the exact source captions.
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
                "   ⋯ Explore Subagent — find configs".to_string(),
                String::new(),
                String::new(),
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
                "   ✓ Explore Subagent — find configs · ludka2/x".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]
        );
        assert_eq!(buffer[(3, 1)].symbol(), "✓");
        assert_eq!(buffer[(3, 1)].fg, theme.text_muted());
        assert_eq!(buffer[(4, 1)].symbol(), " ");
        // This TestBackend's canvas is Reset; the real shell canvas supplies
        // white. The icon-box layout cell must inherit, not borrow muted.
        assert_eq!(buffer[(4, 1)].fg, ratatui::style::Color::Reset);
        assert!(buffer[(4, 1)].modifier.is_empty());
        assert!(
            completed.child_job.is_none(),
            "legacy result prose is not authority"
        );

        // A failed child shows the upstream-shaped reason in the error color.
        let failed = make_card(
            "subagent",
            "failed",
            serde_json::json!({"agent": "explore", "description": "find configs", "prompt": "x"}),
            Some("error: subagent failed (sessionID: s-parent-sub-2): child crashed"),
        );
        let (rows, buffer) = render(&failed, 60, 5);
        assert_eq!(rows[1], "   │ Explore Subagent — find configs");
        assert_eq!(rows[2..], ["", "", ""]);
        assert_eq!(buffer[(3, 1)].fg, theme.error());
        let ToolRender::Subagent(render) = &failed.render else {
            panic!("subagent")
        };
        let detail = subagent_block_expanded(render, &failed, theme, 60, true);
        assert!(
            detail
                .iter()
                .any(|line| line.plain_text().contains("child crashed"))
        );
    }

    #[test]
    fn actual_subagent_static_gap_and_background_badge_use_resolved_roles() {
        let mut asset: serde_json::Value =
            serde_json::from_str(crate::theme::OPENCODE_THEME_JSON).unwrap();
        asset["dark"]["text"]["muted"] = serde_json::json!("#123456");
        let custom = Theme::from_json(&asset.to_string(), crate::theme::ThemeMode::Dark).unwrap();
        for theme in [Theme::dark(), Theme::light(), &custom] {
            for background in [false, true] {
                let mut card = make_card(
                    "subagent",
                    "running",
                    serde_json::json!({
                        "agent":"explore", "description":"A long controlled task description", "prompt":"task"
                    }),
                    Some("actual returned handle"),
                );
                card.child_job = Some(Box::new(oc_core::queries::ChildJob {
                    parent: oc_core::domain::SessionId("parent".into()),
                    child: oc_core::domain::SessionId("child".into()),
                    operation: card.op.clone(),
                    generation: 1,
                    location: "/fixture".into(),
                    agent: "explore".into(),
                    model: "fixture/model".into(),
                    description: "A long controlled task description".into(),
                    delivery_id: "delivery".into(),
                    state: oc_core::queries::ChildState::Running,
                    background,
                    turn: None,
                    result: None,
                    message_id: None,
                }));
                let ToolRender::Subagent(view) = &mut card.render else {
                    panic!("typed child")
                };
                view.spinner = Some("⋯");
                let view = view.clone();
                let lines = subagent_block_expanded(&view, &card, theme, 60, false);
                assert_eq!(lines[0].spans()[1].style().fg, Some(theme.text_muted()));
                assert_eq!(
                    lines[0].spans()[2].style().fg,
                    (!background).then_some(theme.text_muted())
                );
                if background {
                    assert_eq!(
                        lines.len(),
                        2,
                        "badge wraps as one item at the real narrow boundary"
                    );
                    assert_eq!(lines[1].plain_text(), "      Background ");
                    let badge = lines[1].spans().last().unwrap();
                    assert_eq!(badge.content(), " Background ");
                    assert_eq!(badge.style().fg, Some(theme.text_muted()));
                    assert_eq!(badge.style().bg, Some(theme.decrease(theme.background())));
                    let mut wrapped = card.clone();
                    let description = "Сравнение subagent с upstream — проверка длинной строки и переноса целого статуса";
                    wrapped.child_job.as_mut().unwrap().description = description.into();
                    let ToolRender::Subagent(view) = &mut wrapped.render else {
                        unreachable!()
                    };
                    view.description = description.into();
                    for state in [
                        oc_core::queries::ChildState::Running,
                        oc_core::queries::ChildState::Completed,
                    ] {
                        wrapped.child_job.as_mut().unwrap().state = state;
                        let ToolRender::Subagent(view) = &wrapped.render else {
                            unreachable!()
                        };
                        let lines = subagent_block_expanded(view, &wrapped, theme, 76, false);
                        assert_eq!(
                            lines.len(),
                            3,
                            "wrapped caption and status are separate flex items"
                        );
                        assert!(!lines[1].plain_text().contains("Background"));
                        assert!(lines[0].plain_text().ends_with("длинной "));
                        assert_eq!(
                            lines[0].spans().last().unwrap().style().fg,
                            Some(theme.text_muted()),
                            "the fitting source separator retains its caption foreground"
                        );
                        assert_eq!(lines[2].plain_text(), "      Background ");
                        assert_eq!(
                            lines[2].spans()[1].style().bg,
                            Some(theme.decrease(theme.background()))
                        );
                    }
                } else {
                    assert_eq!(lines.len(), 1);
                }
            }
        }
    }

    #[test]
    fn golden_pending_delegation_uses_resolved_theme_and_animation_gap() {
        let mut asset: serde_json::Value =
            serde_json::from_str(crate::theme::OPENCODE_THEME_JSON).unwrap();
        asset["dark"]["text"]["muted"] = serde_json::json!("#123456");
        let custom = Theme::from_json(&asset.to_string(), crate::theme::ThemeMode::Dark).unwrap();
        for theme in [Theme::dark(), Theme::light(), &custom] {
            for glyph in ["⠋", "⋯"] {
                let mut card =
                    make_card("subagent", "argument_stream", serde_json::json!({}), None);
                let ToolRender::Subagent(view) = &mut card.render else {
                    panic!("typed pending")
                };
                view.spinner = Some(glyph);
                let view = view.clone();
                for width in [12, 80, 120] {
                    let lines = subagent_block_expanded(&view, &card, theme, width, false);
                    assert!(lines[0].plain_text().starts_with(&format!("   {glyph} ")));
                    if width >= 80 {
                        assert_eq!(lines[0].plain_text(), format!("   {glyph} Delegating…"));
                    }
                    let spans = lines[0].spans();
                    assert_eq!(spans[1].style().fg, Some(theme.text_muted()));
                    assert_eq!(
                        spans[2].style().fg,
                        if glyph == "⋯" {
                            Some(theme.text_muted())
                        } else {
                            None
                        }
                    );
                    assert!(spans.iter().all(|span| span.style().bg.is_none()));
                    let hovered = crate::messages::hover_subagent_content(&lines[0], theme);
                    assert_eq!(hovered.spans()[1].style().fg, Some(theme.text()));
                    assert_eq!(
                        hovered.spans()[2].style().fg,
                        if glyph == "⋯" {
                            Some(theme.text())
                        } else {
                            None
                        }
                    );
                }
            }
        }
    }

    /// Parsing is honest: unknown tools fall back to the generic renderer and
    /// missing fields stay absent.
    #[test]
    fn render_parsing_never_invents_fields() {
        let card = make_card("mcp_tool", "completed", serde_json::json!({}), Some("ok"));
        match &card.render {
            ToolRender::Inline(InlineRender::Generic { args, .. }) => assert!(args.is_empty()),
            other => panic!("expected generic render, got {other:?}"),
        }
        let card = make_card(
            "read",
            "completed",
            serde_json::json!("not an object"),
            None,
        );
        match &card.render {
            ToolRender::Inline(InlineRender::Read { path, range }) => {
                assert!(path.is_empty());
                assert!(range.is_none());
            }
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
                path: String::new(),
                range: None,
            }
            .pending_label(),
            "Reading file…"
        );
        assert_eq!(
            InlineRender::Glob {
                pattern: String::new(),
                path: String::new(),
                matches: None
            }
            .pending_label(),
            "Finding files…"
        );
        assert_eq!(
            InlineRender::Grep {
                pattern: String::new(),
                path: String::new(),
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
        assert_eq!(rows[1], "   → Read src/main.rs");
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

#[cfg(test)]
mod presentation_tests;
