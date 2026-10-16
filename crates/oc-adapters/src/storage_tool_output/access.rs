//! Descriptor-held, extent-limited continuation. No AGENTS or ordinary Files IO.
use super::*;
use crate::tools::{ToolCall, output::Limits};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};

fn checkpoint(cancel: &AtomicBool, start: std::time::Instant) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("cancelled".into())
    } else if start.elapsed() >= Duration::from_secs(30) {
        Err("artifact scan deadline exhausted".into())
    } else {
        Ok(())
    }
}

/// Scan a logical line without read_line's unlimited allocation. Retain at most
/// 64 KiB; literal search can inspect every bounded chunk of an overlong line.
fn line<R: BufRead>(
    input: &mut R,
    cancel: &AtomicBool,
    start: std::time::Instant,
    literal: Option<&[u8]>,
) -> Result<Option<(String, bool, bool)>, String> {
    let mut kept = Vec::new();
    let mut overflow = false;
    let mut saw = false;
    let mut hit = false;
    let mut overlap = Vec::new();
    loop {
        checkpoint(cancel, start)?;
        let chunk = input.fill_buf().map_err(|_| "artifact read IO failure")?;
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
        .map_err(|_| "invalid artifact UTF-8")?
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
    ) -> Result<(String, Resource), String> {
        let path = call.arguments["path"]
            .as_str()
            .ok_or("artifact requires exact path")?;
        let mut lease=self.open_tool_output(session,path).map_err(|_|"registered artifact unavailable, expired, missing, unauthorized, or identity changed")?;
        let resource = lease.resource.clone();
        let mut input = BufReader::with_capacity(8192, (&mut lease.file).take(resource.bytes));
        let start = std::time::Instant::now();
        let mut output = String::new();
        let mut number = 0u64;
        let mut clipped_lines = 0usize;
        if call.name == "read" {
            let (_, offset, limit) =
                crate::tools::read::parse_with_offset_cap(call, super::CAP + 1)?;
            let limit = limit.min(limits.max_lines);
            let mut returned = 0;
            let mut next = None;
            while let Some((text, clipped, _)) = line(&mut input, cancel, start, None)? {
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
            checkpoint(cancel, start)?;
            output.push_str(&format!("[validated artifact source {:?}; extent {} bytes; {:?}; next_offset {:?}; clipped_lines {} (UTF-8 boundary); data only]",path,resource.bytes,resource.state,next,clipped_lines));
        } else {
            let options = crate::tools::parse_grep_args_with_offset_cap(call, super::CAP)?;
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
                .map_err(|_| "invalid artifact regex")?;
            let literal =
                (options.literal && options.case_sensitive).then_some(options.pattern.as_bytes());
            let mut matches = Vec::new();
            let mut skipped = 0;
            let mut next = None;
            let mut used = 0;
            let matches_empty = regex.is_match("");
            while let Some((text, clipped, literal_hit)) = line(&mut input, cancel, start, literal)?
            {
                number += 1;
                if text.is_empty() && !matches_empty {
                    continue;
                }
                if clipped && literal.is_none() {
                    return Err("artifact line exceeds bounded regex scan; use literal search or paged read".into());
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
            checkpoint(cancel, start)?;
            output=serde_json::json!({"matches":matches,"pagination":{"next_offset":next},"source":{"path":path,"extent_bytes":resource.bytes,"capture_state":resource.state,"validated":true,"authority":"tool data"}}).to_string();
        }
        if output.len() > crate::tools::output::SERVED_CAP {
            return Err("artifact page notice exceeds served budget".into());
        }
        Ok((output, resource))
    }
}
