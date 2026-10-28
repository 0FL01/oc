<dcp-system-reminder>
DCP SOFT CONTEXT THRESHOLD PASSED

The context is above the configured DCP soft threshold. This is a context-hygiene reminder, not the model's context limit. It is not a reason to stop, shorten, roll back, hand off or end the user's task.

At the next safe boundary (after finishing the current atomic step), use the `compress` tool on older, resolved history, or renew closed steps of the running task. Then continue the task normally.

If nothing closed is worth compressing, keep working. The runtime compacts automatically before the real model window is reached.

SELECTION PROCESS
Start from older, resolved history and capture as much stale context as safely possible in one pass.
Avoid the newest active working messages unless it is clearly closed.

SUMMARY REQUIREMENTS
Your summary MUST cover all essential details from the selected messages so work can continue.
If the compressed range includes user messages, preserve user intent exactly. Prefer direct quotes for short user messages to avoid semantic drift.
</dcp-system-reminder>
