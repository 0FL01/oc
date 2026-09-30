//! Native shell argument admission. Canonical command and legacy argv stay distinct.
use super::{BASH_TIMEOUT_CAP_MS, ToolCall, ToolError};
use std::collections::BTreeMap;
use std::time::Duration;

pub(super) const FOREGROUND_TIMEOUT_MS: u64 = 120_000;
// NUL cannot appear in a validated legacy argv resource, avoiding collisions
// with old literal grants as well as their wildcard patterns.
pub(crate) const COMMAND_GRANT_PREFIX: &str = "\0shell-command:v1:";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandInput {
    command: String,
    workdir: Option<String>,
    timeout: Option<u64>,
    background: Option<bool>,
}

#[derive(serde::Deserialize)]
// Historical argv admission ignored extra metadata (e.g. descriptions).
// It remains literal argv execution, never a command-text fallback.
struct ArgvInput {
    argv: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
}

pub(crate) struct ShellInvocation {
    pub argv: Vec<String>,
    pub cwd: String,
    pub timeout: Duration,
}

/// Shared by pre-intent admission/approval and the execution owner.
pub(crate) fn invocation(
    call: &ToolCall,
    env: &BTreeMap<String, String>,
) -> Result<ShellInvocation, ToolError> {
    let invalid = |reason: &str| ToolError::InvalidArgs {
        tool: call.name.clone(),
        reason: reason.into(),
    };
    let (argv, cwd, timeout) = match call.name.as_str() {
        "shell" => {
            if ["workdir", "timeout", "background"].iter().any(|key| {
                call.arguments
                    .get(key)
                    .is_some_and(serde_json::Value::is_null)
            }) {
                return Err(invalid(
                    "optional fields must have their declared types, not null",
                ));
            }
            let input: CommandInput =
                serde_json::from_value(call.arguments.clone()).map_err(|_| {
                    invalid("expected command, optional workdir and nonnegative timeout")
                })?;
            if input.background == Some(true) {
                return Err(ToolError::Unsupported {
                    tool: call.name.clone(),
                    feature: "background execution".into(),
                });
            }
            if input.command.is_empty() || input.command.contains('\0') {
                return Err(invalid("command must be nonempty and NUL-free"));
            }
            let timeout = input.timeout.unwrap_or(FOREGROUND_TIMEOUT_MS);
            if timeout > BASH_TIMEOUT_CAP_MS {
                return Err(invalid(
                    "timeout exceeds native execution resource ceiling (600000 ms)",
                ));
            }
            let argv = crate::shell::command_argv(env, &input.command)
                .map_err(|_| invalid("invalid command or unavailable Linux shell"))?;
            (
                argv,
                input.workdir.unwrap_or_else(|| ".".into()),
                Duration::from_millis(timeout),
            )
        }
        "bash" => {
            let input: ArgvInput = serde_json::from_value(call.arguments.clone())
                .map_err(|_| invalid("expected argv, optional cwd and nonnegative timeout_ms"))?;
            let timeout = input.timeout_ms.unwrap_or(30_000).min(BASH_TIMEOUT_CAP_MS);
            // Legacy explicit zero kept its immediate deadline; canonical zero
            // is the donor no-execution-timeout contract.
            (
                input.argv,
                input.cwd.unwrap_or_else(|| ".".into()),
                if timeout == 0 {
                    Duration::from_nanos(1)
                } else {
                    Duration::from_millis(timeout)
                },
            )
        }
        _ => return Err(invalid("not a shell invocation")),
    };
    crate::shell::validate_argv(&argv).map_err(|_| invalid("invalid or oversized argv/command"))?;
    if cwd.contains('\0') {
        return Err(invalid("workdir must be NUL-free"));
    }
    Ok(ShellInvocation { argv, cwd, timeout })
}
