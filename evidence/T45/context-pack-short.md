## Result
R8 first atomic qualified (bdd0bdc3d): optional context_message_ids resolve canonical active-branch parent user/assistant messages before child creation and prepend an escaped parent_context pack to the durable child task.
## Checks
Workspace 1557/0/10; fmt and strict Clippy PASS. Evidence: evidence/T45/context-pack.md.
## Risks
Pack is bounded by bytes/count only; full child model-context cost, bounded notice-free ID index, DCP-covered selection test, guidance/preview wording and attachment diagnostics remain pending. T44 PAUSED, T27 allowance unchanged.
## Next
Next R8 atomic: pre-admission model-context cost of the pack against the resolved child's fixed inputs and output reserve; then reverted/DCP-covered selection tests and the bounded ID index.
