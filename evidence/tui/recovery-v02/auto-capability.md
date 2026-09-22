# V02 decision: session autoaccept capability mapping

## Evidence

Pinned supplied upstream checkout: `2670273ff17da96f85c5826ced57aa1b368754fa`.
Targeted files were read locally; no backend behavior was inferred from a label:

- `packages/tui/src/component/prompt/index.tsx:1828` passes
  `auto={local.permission.mode === "autoaccept"}` to prompt metadata.
- `packages/tui/src/context/permission.tsx:5-15` derives that session mode from
  CLI `args.auto` or `config.data.session.permissions` (`prompt | autoaccept`).
- `packages/tui/src/routes/session/index.tsx:243-259` actually replies `once` to
  pending permission requests in autoaccept mode; it is not model selection,
  an agent mode, or a shorthand for tool allow rules.
- Native `RuntimePolicy::check` in `crates/oc-adapters/src/runtime.rs` accepts
  Allow and rejects Deny/Ask/missing rules. Native OC has no pending permission
  request queue or reply API implementing upstream autoaccept.

## Decision and consequences

The application projects `CatalogSnapshot.auto_accept: AutoAcceptState` with
explicit Unsupported/Disabled/Enabled alternatives. The current native owner
reports **Unsupported**, including configurations with allowed tools. The TUI
renders `auto` only for an explicit **Enabled** snapshot. Workspace reset clears
the capability state. Agent tool rules and profiles never manufacture this marker.

This resolves the V02 requirement to source the marker honestly. It does **not**
claim interactive permission approval/autoaccept backend parity. Implementing that
backend capability requires its own scoped application contract; V02 adds no
approval flow, permission widening, CLI switch, or fake API. The marker belongs
to session capability state, not `AgentEntry`.

The renderer is tested across all three states via catalog mutation. The actual
binary/config recovery test asserts Unsupported despite explicit tool Allow rules.

## Related generation presentation

Upstream `context/local.tsx:56-62,125-132` selects primary agents separately but
assigns categorical colors across all visible profiles, including children.
Native OC pins its admitted-generation categorical slots across all profiles.
`AgentEntry.color_index` supplies the current primary selector; `TurnLane` pins
the actual primary/child slot in the durable turn. Reloading a catalog cannot
recolor an existing child turn. No T43/T45 execution or permission behavior changes.
