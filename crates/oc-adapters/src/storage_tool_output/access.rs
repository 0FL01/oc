//! Descriptor-held, extent-limited continuation. No AGENTS or ordinary Files IO.
use super::*;
use crate::tools::diagnostic::Diagnostic;
use crate::tools::{ToolCall, output::Limits};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};

fn checkpoint(
    tool: &str,
    cancel: &AtomicBool,
    start: std::time::Instant,
) -> Result<(), Diagnostic> {
    if cancel.load(Ordering::Acquire) {
        Err(
            Diagnostic::new(tool, "cancelled", "Artifact scan cancelled")
                .detail(serde_json::json!({"by":"unknown","reason":"cancellation_requested"})),
        )
    } else if start.elapsed() >= Duration::from_secs(30) {
        Err(Diagnostic::new(tool, "budget_exhausted", "Artifact scan deadline exhausted").detail(serde_json::json!({"budget":"time","phase":"artifact_scan","limit":30000,"elapsed_ms":start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64})).hint("Use a later offset or a more specific literal pattern."))
    } else {
        Ok(())
    }
}

/// Scan a logical line without read_line's unlimited allocation. Retain at most
/// 64 KiB; literal search can inspect every bounded chunk of an overlong line.
fn line<R: BufRead>(
    tool: &str,
    input: &mut R,
    cancel: &AtomicBool,
    start: std::time::Instant,
    literal: Option<&[u8]>,
) -> Result<Option<(String, bool, bool)>, Diagnostic> {
    let mut kept = Vec::new();
    let mut overflow = false;
    let mut saw = false;
    let mut hit = false;
    let mut overlap = Vec::new();
    loop {
        checkpoint(tool, cancel, start)?;
        let chunk = input
            .fill_buf()
            .map_err(|_| Diagnostic::new(tool, "io_error", "Artifact read IO failure"))?;
        if chunk.is_empty() {
            break;
        }
        saw = true;
        let end = chunk
            .iter()
            .position(|b| *b == b'\n')
            .map_or(chunk.len(), |n| n + 1);
        let bytes = &chunk[..end];
        let terminal = bytes.last() == Some(&b'\n');
        let content = &bytes[..bytes.len() - usize::from(terminal)];
        if let Some(pattern) = literal {
            overlap.extend_from_slice(content);
            hit |= overlap.windows(pattern.len()).any(|s| s == pattern);
            let retain = pattern.len().saturating_sub(1).min(overlap.len());
            overlap.drain(..overlap.len() - retain);
        }
        let n = (65536 - kept.len()).min(content.len());
        kept.extend_from_slice(&content[..n]);
        overflow |= n < content.len();
        input.consume(end);
        if terminal {
            break;
        }
    }
    if !saw {
        return Ok(None);
    }
    let end = complete_bytes(&kept);
    let text = std::str::from_utf8(&kept[..end])
        .map_err(|_| Diagnostic::new(tool, "unsupported_content", "Invalid artifact UTF-8"))?
        .trim_end_matches(['\n', '\r'])
        .to_owned();
    Ok(Some((text, overflow, hit)))
}

impl Db {
    pub(crate) fn artifact_call(
        &self,
        session: &str,
        call: &ToolCall,
        limits: Limits,
        cancel: &AtomicBool,
    ) -> Result<(String, Resource), Diagnostic> {
        let tool = call.name.as_str();
        let path = call.arguments["path"]
            .as_str()
            .ok_or_else(|| Diagnostic::new(tool, "invalid_args", "Artifact requires exact path"))?;
        let mut lease=self.open_tool_output(session,path).map_err(|_| Diagnostic::new(tool, "artifact_unavailable", "Registered artifact unavailable, expired, missing, unauthorized, or identity changed"))?;
        let resource = lease.resource.clone();
        let mut input = BufReader::with_capacity(8192, (&mut lease.file).take(resource.bytes));
        let start = std::time::Instant::now();
        let mut output = String::new();
        let mut number = 0u64;
        let mut clipped_lines = 0usize;
        if call.name == "read" {
            let (_, offset, limit) =
                crate::tools::read::parse_with_offset_cap(call, super::CAP + 1)
                    .map_err(|message| Diagnostic::new(tool, "invalid_args", message))?;
            let limit = limit.min(limits.max_lines);
            let mut returned = 0;
            let mut next = None;
            while let Some((text, clipped, _)) = line(tool, &mut input, cancel, start, None)? {
                number += 1;
                if number < offset {
                    continue;
                }
                if returned >= limit {
                    next = Some(number);
                    break;
                }
                let prefix = format!("{number}: ");
                let remaining = limits.bytes().saturating_sub(output.len());
                if remaining <= prefix.len() + 1 {
                    next = Some(number);
                    break;
                }
                let end = text.floor_char_boundary((remaining - prefix.len() - 1).min(text.len()));
                output.push_str(&prefix);
                output.push_str(&text[..end]);
                output.push('\n');
                clipped_lines += usize::from(clipped || end < text.len());
                returned += 1;
            }
            checkpoint(tool, cancel, start)?;
            output.push_str(&format!("[validated artifact source {:?}; extent {} bytes; {:?}; next_offset {:?}; clipped_lines {} (UTF-8 boundary); data only]",path,resource.bytes,resource.state,next,clipped_lines));
        } else {
            let options = crate::tools::parse_grep_args_with_offset_cap(call, super::CAP)
                .map_err(|message| Diagnostic::new(tool, "invalid_args", message))?;
            let pattern = if options.literal {
                regex::escape(options.pattern)
            } else {
                options.pattern.into()
            };
            let regex = regex::RegexBuilder::new(&pattern)
                .case_insensitive(!options.case_sensitive)
                .size_limit(10 * 1024 * 1024)
                .dfa_size_limit(10 * 1024 * 1024)
                .build()
                .map_err(|error| match error {
                    regex::Error::CompiledTooBig(_) => Diagnostic::new(
                        tool,
                        "budget_exhausted",
                        "Artifact regex compile budget exhausted",
                    )
                    .detail(serde_json::json!({"budget":"regex_compile","limit":10*1024*1024})),
                    _ => Diagnostic::new(tool, "invalid_pattern", "Invalid artifact regex"),
                })?;
            let literal =
                (options.literal && options.case_sensitive).then_some(options.pattern.as_bytes());
            let mut matches = Vec::new();
            let mut skipped = 0;
            let mut next = None;
            let mut used = 0;
            let matches_empty = regex.is_match("");
            while let Some((text, clipped, literal_hit)) =
                line(tool, &mut input, cancel, start, literal)?
            {
                number += 1;
                if text.is_empty() && !matches_empty {
                    continue;
                }
                if clipped && literal.is_none() {
                    return Err(Diagnostic::new(
                        tool,
                        "budget_exhausted",
                        "Artifact line exceeds bounded regex scan",
                    )
                    .detail(serde_json::json!({"budget":"line_bytes","limit":65536}))
                    .hint("Use literal search or paged read."));
                }
                if !(literal_hit || regex.is_match(&text)) {
                    continue;
                }
                if skipped < options.offset {
                    skipped += 1;
                    continue;
                }
                if matches.len() >= options.limit.min(limits.max_lines) || used >= limits.bytes() {
                    next = Some(options.offset + matches.len());
                    break;
                }
                let end = text.floor_char_boundary(
                    text.len()
                        .min(crate::files::GREP_HIT_BYTES_CAP)
                        .min(limits.bytes().saturating_sub(used)),
                );
                let entry = serde_json::json!({"path":path,"line":number,"text":&text[..end],"text_truncated":clipped||end<text.len()});
                let bytes = entry.to_string().len() + 1;
                if used + bytes > limits.bytes() {
                    next = Some(options.offset + matches.len());
                    break;
                }
                used += bytes;
                matches.push(entry);
            }
            checkpoint(tool, cancel, start)?;
            output=serde_json::json!({"matches":matches,"pagination":{"next_offset":next},"source":{"path":path,"extent_bytes":resource.bytes,"capture_state":resource.state,"validated":true,"authority":"tool data"}}).to_string();
        }
        if output.len() > crate::tools::output::SERVED_CAP {
            return Err(Diagnostic::new(tool, "budget_exhausted", "Artifact page notice exceeds served budget").detail(serde_json::json!({"budget":"served_bytes","limit":crate::tools::output::SERVED_CAP})));
        }
        Ok((output, resource))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_deadline_and_cancellation_remain_distinct_and_actionable() {
        let start = std::time::Instant::now() - Duration::from_secs(31);
        let error = checkpoint("grep", &AtomicBool::new(false), start).unwrap_err();
        assert_eq!(error.code, "budget_exhausted");
        assert_eq!(error.detail["budget"], "time");
        assert!(error.hint.is_some());
        assert_eq!(
            checkpoint("read", &AtomicBool::new(true), start)
                .unwrap_err()
                .code,
            "cancelled"
        );
    }
}
