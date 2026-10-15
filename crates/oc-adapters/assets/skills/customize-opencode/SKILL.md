---
name: customize-opencode
description: "Configure the native Rust oc itself: runtime JSON/JSONC, providers/models, MCP, permissions, profiles, skills, commands, instructions, DCP and CLI preferences."
---
# Configure native oc

Use this manual to configure **the native Rust `oc2`**, not an arbitrary application
or the Node/Bun upstream. It is bundled in the binary; no checkout, runtime skill
file or network documentation fetch is needed. Load it explicitly with
`skill({"id":"customize-opencode"})`. Automatic discovery supplies metadata only.

## Safety and authority

- Establish the requested scope: current Location/project or the user's global
  profile. When scope is unspecified, prefer the narrow project change. This text
  grants no filesystem, credential, process, network or approval authority.
- Global/outside-Location writes need the user's authorization and ordinary tool
  admission. Never search other users' credentials, change the authoring agent's
  configuration, disable trust checks or widen permissions to make a config work.
- Read the existing relevant file first. Preserve unrelated keys, comments,
  credentials and custom definitions; make one minimal diff, not a whole-file
  replacement from a template. Do not run a formatter over unrelated user data.
- Never print expanded configuration, keys, auth headers or environment dumps.
  Prefer `{env:YOUR_KEY}` references. Missing env substitution becomes empty, not
  a valid key. `{file:...}` reads require source trust and no-follow admission;
  even a disabled entry is not a license to include an untrusted credential file.
- Disabled providers/MCP do not start their network/process effects. Enabling MCP
  can start a real process or connection; obtain approval for that dependency.
  Catalog presence is not proof of authentication, generation or tool support.

## Files, scopes and precedence

`G` is a nonempty `OPENCODE_CONFIG_DIR`, otherwise the platform OpenCode config
root (`$XDG_CONFIG_HOME/opencode`, normally `~/.config/opencode`). An override
**replaces** the default global root; do not edit both expecting two layers.

The native application admits the global root, the current Location workspace
directory, and its `.opencode` directory. Runtime config reads `opencode.json`
then `opencode.jsonc` within each admitted root, low to high in that order.
JSONC comments and trailing commas work. The published `https://opencode.ai/config.json`
is not a complete native schema and cannot prove that a setting is implemented.
Do not assume upstream's unrestricted
ancestor walk, an arbitrary external include, or an unimplemented CLI flag.
Source trust and credential/endpoint ownership are checked before publication.

| Domain | Native input and purpose |
| --- | --- |
| Runtime | `opencode.json` / `opencode.jsonc`: provider/model selection, MCP, permission rules, agent/command definitions, instructions and DCP inputs. |
| CLI preferences | `cli.json` / `cli.jsonc`, independently read through the same admitted-root reader, JSON before JSONC and later roots winning supplied preferences. These are not runtime-provider fields. Start with `G/cli.jsonc` for user-wide preferences; an admitted Location/`.opencode` CLI file may override them natively. |
| Definitions | Built-ins → inline `G` → Markdown `G` → inline Location → inline Location `.opencode` → Markdown Location `.opencode`. Markdown roots are `G` and Location `.opencode`, not a separate `G/.opencode` or bare Location directory. Singular directories load before plural at the same source. |
| Instructions | `AGENTS.md` and admitted configured instructions: guidance, **not** a config override or a permission grant. Native canonical Location/source boundaries still apply. |
| DCP | Inline `dcp`, then optional `dcp.json`, then `dcp.jsonc` within the existing global → Location → `.opencode` layers. No automatic config file creation. |
| Native profile | `oc-rs.toml` reference/examples are Proposed, not an application-input/support guarantee. Use only consumer-qualified controls; parser acceptance and `examples/oc-rs.toml` are not a shipped-feature inventory. |

Merge is domain-specific, not a universal deep merge: later same-ID MCP servers
replace whole entries; permission rules retain ordered authority; omitted agent
fields can retain earlier profile values; definition collisions retain provenance.
Invalid individual definitions have diagnostics and healthy siblings survive.
Malformed config/policy, trust failures and invalid mandatory references do not
silently become Allow or a fallback provider. A failed reload keeps the previous
complete generation, not a mixture of old clients and new policy.

## Minimal safe workflow

1. Identify the effective Location, global override and relevant domain. Read only
   necessary admitted files; inspect safe diagnostics rather than dumping values.
2. Confirm the requested provider/model IDs, endpoint, credentials owner and any
   external MCP dependency. Prefer the existing admitted entries and exact IDs.
3. Make the smallest authorized edit. Keep secrets as references. Check the diff
   for unintended permission, endpoint, argv, default-agent or selection changes.
4. Apply `/reload` at an idle boundary, or restart the **owned idle** application.
   Do not kill active jobs or replay unknown effects to apply a preference.
   Prepared requests keep their captured generation and model; picker drafts,
   committed selections and edited config defaults are not interchangeable.
5. Check actual safe Settings/provider/plugin/MCP diagnostics and the result of
   the requested behavior. `oc2 models` lists admitted exact model references
   without generation, session creation or MCP startup; dynamic discovery can
   make bounded metadata HTTP requests. Listing alone is not readiness proof.
6. If reload fails, retain the last healthy generation, fix the indicated safe
   source/field and retry the changed cause. Never clear data, saved selections
   or security policy to hide the failure.

There is no working `oc2 config explain`, `oc2 config check` or `oc2 config validate`
command. Use `oc2 --help`, `oc2 models`, TUI diagnostics and narrowly scoped tests.
Do not initiate a paid model request or OAuth flow merely to validate syntax.

## Provider and model example

This is an illustrative **canonical OpenCode 2 shape**, not credentials or a
promise that the placeholder model exists. OpenCode 1 keys (`provider`, `npm`,
`options`, `permission`, `tools`, `agent`, `command`, `plugin`, `mcp.<name>`,
`enabled_providers` …) are refused with one `legacy_config` report listing every
key and its canonical replacement; convert them, never re-add OpenCode 1 shapes. Replace endpoint/ID with the user's
actual admitted values and verified model metadata (the limits below illustrate
the shape, not a claim about an existing model); merge into the existing file.

```jsonc
{
  "providers": {
    "lab": {
      "package": "@ai-sdk/openai",
      "compatibility": { "supportsPromptCacheKey": true },
      "settings": {
        "baseURL": "https://api.example.invalid/v1",
        "apiKey": "{env:LAB_API_KEY}"
      },
      "models": {
        "exact-model-id": {
          "name": "My admitted model",
          "limit": { "context": 65536, "output": 4096 }
        }
      }
    }
  },
  "model": "lab/exact-model-id"
}
```

The `package` value is a **native protocol alias**, not npm execution:
`@ai-sdk/openai` (also `@opencode/ai/providers/openai`) selects Responses;
`@ai-sdk/openai-compatible` / `@opencode/ai/providers/openai-compatible` select
Chat Completions; `@ai-sdk/anthropic` / `@opencode/ai/providers/anthropic` select
Messages. Unknown packages are unsupported, not dynamically imported. Preserve
the exact configured base path; do not append a vendor route or infer protocol,
reasoning or capabilities from a model name. Use supported catalog/variant IDs;
static metadata and discovery do not prove wire support. File-tool schemas follow
the captured committed model view: use the advertised family, not stale tools.

Supported compatibility fields inherit from provider defaults to model overrides;
documents merge each level first, so an earlier model `false` still overrides a
later provider `true`. Variants have no compatibility object. For body prompt
cache keys, Responses permits an available key unless `supportsPromptCacheKey`
is false; Chat requires true. Do not add a Rust-only `settings.setCacheKey` switch:
that arbitrary package setting is ignored, not legacy syntax. Request `cache:none`
is an AI/SDK policy, not a native `settings.cache` JSON option. Header affinity and
Messages automatic cache blocks are separate from the cache-key capability.

Native OpenAI browser/device/API-key authentication has explicit operator steps.
Never substitute an OpenProxy credential for an ordinary OpenAI key or auto-confirm
someone's browser/device authorization. A custom-provider connection wizard and
arbitrary-provider `/models` discovery are planned work, not a reason to invent
working CLI commands or POST-based protocol auto-detection.

## MCP example and permissions

This canonical direct-mode entry is deliberately disabled until its actual URL,
credential ownership and connection are authorized. `oauth:false` avoids OAuth;
native omitted OAuth also means no OAuth, unlike the upstream default.

```jsonc
{
  "mcp": {
    "servers": {
      "docs": {
        "type": "remote",
        "url": "https://docs.example.invalid/mcp",
        "oauth": false,
        "codemode": false,
        "disabled": true,
        "headers": { "Authorization": "Bearer {env:DOCS_API_KEY}" }
      }
    }
  }
}
```

Entries live under `mcp.servers` and use `disabled`; timeouts are
`{startup, catalog, execution}` in ms. Local MCP uses an explicit `command`
argv array (plus admitted cwd/env); external `npx` is only a user-approved installed
dependency, never an internal JS host or automatic package installation.

Since Code Mode, omitted/true `codemode` puts the server's tools in `execute`'s
catalog (`tools.<server>.<tool>`); false keeps direct tools. These are different
advertised surfaces. Nested calls still pass ordinary permissions/approval and
MCP ownership; the native `?codemode=false` URL query is not added.

Configured, enabled, admitted and connected MCP tools have a native default Allow
before explicit narrowing. This does not enable disabled servers, authorize unknown
actions or override central/source/profile/parent ceilings. Plan/Explore are not
MCP effect sandboxes; tools are not inferred read-only. Match the actual registered
action/alias. A literal server name is **not** a wildcard for all its tools.

```jsonc
{
  "permissions": [
    { "action": "*", "resource": "*", "effect": "ask" },
    { "action": "read", "resource": "*", "effect": "allow" },
    { "action": "skill", "resource": "customize-opencode", "effect": "allow" }
  ]
}
```

Canonical rules are ordered. A later rule can override an earlier rule inside
its own authority, not another independent ceiling. Actions are canonical
(`edit` covers apply_patch/write/edit, `shell` the shell tool). Invalid security
policy is fatal. Outside-Location access still needs its external-directory
admission as well as action permission. Do not recommend broad Allow or CLI
`--auto` as a workaround; headless Ask without a consumer fails before execution.

## Agents, skills, commands and instructions

Native built-ins are `build`/`plan` primary and `general`/`explore` subagents.
Supplied configuration can override them under ordinary profile merge/ceilings;
mode, hidden/disabled, explicit selection and automatic eligibility are separate.
Use real Plan policy, not just the word “plan” in a prompt. Custom profiles use
`.opencode/{agent,agents}/<id>.md`; recursive agent IDs and the admitted one-level
`mode`/`modes` compatibility form also exist. For example `agents/reviewer.md`:

```markdown
---
description: Review the current workspace
mode: primary
permissions:
  - action: edit
    resource: "*"
    effect: deny
  - action: shell
    resource: "*"
    effect: ask
---
Review the requested changes; do not claim unexecuted checks passed.
```

The frontmatter narrows ordinary authority; body prose does not sandbox MCP or
grant writes. Model/variant pins must be exact supported references. Do not
invent an artificial successful-round limit or automatic agent fallback.

Skills are informational Markdown, never executable plugins. Both
`{skill,skills}/<id>/SKILL.md` and flat `{skill,skills}/<id>.md` are admitted under
the definition roots. IDs come from the directory/file stem. `name` and
`description` are metadata; `metadata."opencode/autoinvoke":false` hides automatic preview only,
not explicit loading. For example `.opencode/skills/project-guide/SKILL.md`:

```markdown
---
name: project-guide
description: Instructions for this project
metadata:
  opencode/autoinvoke: false
---
Read the project's existing instructions before changing its configuration.
```

`customize-opencode` is already built in; do not install a duplicate unless the
user actually wants an override. A valid global definition replaces the builtin,
and a later project definition replaces that winner with normal provenance.
A malformed replacement leaves the last valid body callable and emits a safe
diagnostic. Caps stay in force: at 256 skills an existing ID can be replaced,
but a new 257th ID cannot. Generation snapshots pin bodies; changing a file after
request preparation does not retroactively change the skill served to that request.
Only metadata enters automatic context; explicit `skill` loading still obeys
per-ID Allow/Ask/Deny and ordinary user output limits.

Commands use `.opencode/{command,commands}/<id>.md`, optional description and
literal prompt text with `$ARGUMENTS`/positional expansion. Agent/model/subagent
routing follows the admitted command/profile definitions. Supported shell
interpolation runs only during authorized invocation, never config loading or
replay. Do not add arbitrary file/URL/late-env includes or reserved command IDs.
An `AGENTS.md` is scoped guidance, not a replacement for permission configuration;
keep its configuration pointer short rather than copying this manual into it.

## DCP and CLI preferences

DCP is native compiled code, not an npm runtime. Known plugin aliases bind the
qualified repository revision; `@latest` does not fetch a package. Arbitrary JS,
version ranges, an updater and custom prompt engine are not enabled by parsing.
For a narrowly requested new-compression hard off, merge into runtime config:

```jsonc
{ "dcp": { "compress": { "enabled": false } } }
```

`compress.enabled:false` blocks new model/manual/API compression without deleting
committed summaries or raw history. Global off, Deny, Ask and child opt-out retain
authority. Commands can be disabled with `commands.enabled:false`; manual mode
is different from global/compress off. Debug emits safe metadata only, not prompt
or credential dumps. No standalone DCP file is necessary. Other accepted settings
need working consumers and their own evidence; a full DCP visual-parity claim is
not implied. Do not rewrite existing DCP config or install an updater.

CLI preferences load independently without rewriting their inputs. The explicit
permission-mode action is a narrow exception: it persists `session.permissions`
in the owner-selected CLI file while preserving unrelated bytes. This native
admitted example belongs in `cli.jsonc`, **not** in runtime `provider` or permission config:

```jsonc
{
  "debug": { "devtools": false },
  "session": { "sidebar": "hide", "tps": false, "permissions": "prompt" },
  "diffs": { "view": "auto", "wrap": "word" },
  "tabs": { "layout": "horizontal", "scope": "cwd", "indicators": "status" }
}
```

Supported enum values here are sidebar `auto|hide`, permissions
`prompt|autoaccept`, diff view `auto|unified|split`, wrap `word|none`, tab layout
`horizontal|vertical`, scope `cwd|global`, indicators `status|numbers`. Preserve
explicit user keybindings and use the effective command hints; never guess an
upstream key ID/shape. Autoaccept is an explicit mode choice, not a default
security fix. Do not advertise the entire upstream CLI schema as native support.

## Before finishing

Review the exact authorized diff and actual post-reload behavior. Report the
changed files/scope, applied or retained generation, checks really executed and
any unavailable external dependency. Do not claim parser-only, planned,
upstream-only, paid-live or pixel-parity outcomes as verified native behavior.
