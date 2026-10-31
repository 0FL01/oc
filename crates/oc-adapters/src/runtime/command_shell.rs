//! Configured-command evaluation before input acceptance. Downstream consumers
//! receive only the frozen result and never evaluate a template during replay.
use super::*;
use futures_util::{FutureExt as _, StreamExt as _, stream::FuturesUnordered};

pub(crate) fn expressions(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut matches = Vec::new();
    let mut at = 0;
    while let Some(start) = text[at..].find("!`").map(|n| at + n) {
        let Some(end) = text[start + 2..].find('`').map(|n| start + 2 + n) else {
            break;
        };
        if end > start + 2 {
            matches.push((start..end + 1, text[start + 2..end].into()));
        }
        if matches.len() > 256 {
            break;
        }
        at = end + 1;
    }
    matches
}

pub(crate) struct Capture {
    published: Arc<ExecutionGeneration>,
    lane: TurnLane,
}

/// A dropped/failed pre-acceptance evaluation remains recoverable. Successful
/// acceptance deletes the record in the same transaction as turn/inbox input.
pub(crate) struct Resolution<'a> {
    db: &'a Db,
    pub id: String,
    pub generation: u64,
}
impl Drop for Resolution<'_> {
    fn drop(&mut self) {
        let _ = self.db.fail_command_resolution(&self.id);
    }
}

impl Runtime<'_> {
    pub(crate) fn capture_command(
        &self,
        workspace: WorkspacePublication,
        command: &NativeCommand,
    ) -> Result<Capture, RuntimeError> {
        let capture = self.capture_user_shell(workspace, "")?;
        let published = capture.0;
        let parent = self.primary_lane(&published);
        let mut lane = if let Some(child) = &command.child {
            let agent = published
                .workspace
                .subagents
                .as_ref()
                .and_then(|c| c.agents.get(&child.agent))
                .ok_or_else(|| RuntimeError::InvalidArgs("command agent unavailable".into()))?;
            self.child_lane_in(&published, agent, &parent, "command-interpolation")
        } else {
            parent
        };
        let mut ceiling = command.shell_parent_rules.clone();
        if let Some(home) = self.parent_env.get("HOME") {
            ceiling.expand_home(home);
        }
        ceiling.bind_plan_project(&self.roots.project);
        lane.permission_rules.extend(ceiling);
        Ok(Capture { published, lane })
    }

    pub(crate) async fn resolve_command_shell<'a>(
        &'a self,
        session: &str,
        original: &str,
        text: &str,
        capture: Capture,
        cancel: &AtomicBool,
    ) -> Result<(String, Resolution<'a>), RuntimeError> {
        let matches = expressions(text);
        if matches.len() > 256 {
            return Err(RuntimeError::InvalidArgs(
                "command interpolation count limit (256)".into(),
            ));
        }
        let commands: Vec<_> = matches.iter().map(|(_, command)| command.clone()).collect();
        let id = format!("command:{}", next_turn_id(session, millis()));
        self.db
            .begin_command_resolution(&id, &self.location, session, original, &commands)?;
        let resolution = Resolution {
            db: &self.db,
            id,
            generation: capture.published.id,
        };
        let shell = self
            .shell
            .for_command(
                &self.parent_env,
                capture.published.config.command_shell.as_deref(),
            )
            .map_err(|_| RuntimeError::InvalidArgs("command shell unavailable".into()))?;
        let policy =
            RuntimePolicy::with_rules(&capture.lane.permissions, &capture.lane.permission_rules)
                .with_root(&self.roots.project);
        let stopped = Arc::new(AtomicBool::new(false));
        let mut outputs = vec![String::new(); matches.len()];
        let mut failure = None;
        let evaluate = |ordinal: usize, source: String| {
            let shell = &shell;
            let capture = &capture;
            let policy = &policy;
            let resolution = &resolution;
            let stopped = &stopped;
            async move {
                if cancel.load(Ordering::Acquire) || stopped.load(Ordering::Acquire) { return Err(RuntimeError::Cancelled); }
                if self.generation_id() != capture.published.id { return Err(RuntimeError::InvalidArgs("command source changed".into())); }
                let call = crate::tools::ToolCall {id:format!("{}:{ordinal}",resolution.id),name:"shell".into(),arguments:serde_json::json!({"command":source})};
                crate::tools::validate_call(&call).map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
                let invocation = shell.invocation(&call, &self.parent_env).map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
                let ctx = ToolContext {files:&self.files,shell,parent_env:&self.parent_env,webfetch_auth:self.webfetch_auth.clone(),webfetch_allow_private:self.webfetch_allow_private,policy,subagent:None,snapshot:&capture.published.workspace.skills,cancel:stopped,roots:Some(self.roots.clone())};
                let admitted = self.admit_tool(&capture.published,session,&resolution.id,&call.id,&call,&ctx,policy,stopped,capture.lane.agent_id.clone(),capture.lane.agent_digest.clone(),false);
                tokio::pin!(admitted);
                let permitted = tokio::select! { biased;
                    () = crate::provider::wait_cancel(cancel) => { stopped.store(true, Ordering::Release); admitted.await },
                    result = &mut admitted => result,
                }.map_err(|error| match error {
                    AdmissionFailure::Cancelled => RuntimeError::Cancelled,
                    AdmissionFailure::Required => RuntimeError::ApprovalRequired {tool:"shell".into()},
                    _ => RuntimeError::PermissionDenied {tool:"shell".into()},
                })?;
                if self.generation_id() != capture.published.id || cancel.load(Ordering::Acquire) || stopped.load(Ordering::Acquire) { return Err(RuntimeError::Cancelled); }
                let pinned = permitted.approved_shell_cwd().ok_or(RuntimeError::Storage)?;
                let running = self.shell_jobs.command_output(shell.clone(),self.parent_env.clone(),invocation.argv,pinned,resolution.id.clone(),ordinal,capture.published.config.tool_output,mcp::mcp_redactions(&capture.published.config,&self.parent_env),stopped);
                tokio::pin!(running);
                let output = tokio::select! { biased;
                    () = crate::provider::wait_cancel(cancel) => { stopped.store(true,Ordering::Release); running.await },
                    result = &mut running => result,
                }?;
                Ok((ordinal,output))
            }.boxed()
        };
        let mut work = FuturesUnordered::new();
        let mut pending = commands.into_iter().enumerate();
        for _ in 0..2 {
            if let Some((ordinal, source)) = pending.next() {
                work.push(evaluate(ordinal, source));
            }
        }
        let mut source_check = tokio::time::interval(Duration::from_millis(10));
        while !work.is_empty() {
            let result = tokio::select! { biased;
                () = crate::provider::wait_cancel(cancel), if !stopped.load(Ordering::Acquire) => {
                    stopped.store(true,Ordering::Release);
                    continue;
                },
                _ = source_check.tick() => {
                    if self.generation_id()!=capture.published.id { stopped.store(true,Ordering::Release); }
                    continue;
                },
                result = work.next() => result.expect("active interpolation"),
            };
            match result {
                Ok((ordinal, output)) => outputs[ordinal] = output,
                Err(error) => {
                    stopped.store(true, Ordering::Release);
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
            if failure.is_none()
                && let Some((ordinal, source)) = pending.next()
            {
                work.push(evaluate(ordinal, source));
            }
        }
        drop(work);
        if let Some(error) = failure {
            return Err(RuntimeError::InvalidArgs(format!(
                "command interpolation failed: {error}; effects may have occurred; Settings → interrupted commands"
            )));
        }
        if cancel.load(Ordering::Acquire) || self.generation_id() != capture.published.id {
            return Err(RuntimeError::Cancelled);
        }
        let mut prompt = String::new();
        let mut end = 0;
        for ((range, _), output) in matches.iter().zip(outputs) {
            if prompt.len() + range.start - end + output.len() > COMMAND_BYTES_CAP {
                return Err(RuntimeError::InvalidArgs(
                    "expanded command too large".into(),
                ));
            }
            prompt.push_str(&text[end..range.start]);
            prompt.push_str(&output);
            end = range.end;
        }
        if prompt.len() + text.len() - end > COMMAND_BYTES_CAP {
            return Err(RuntimeError::InvalidArgs(
                "expanded command too large".into(),
            ));
        }
        prompt.push_str(&text[end..]);
        self.db.resolve_command_result(&resolution.id, &prompt)?;
        Ok((prompt, resolution))
    }
}
