# oc2 — OpenCode Rust

**A native Rust coding agent for your terminal.** Work interactively in a TUI
or run tasks from the command line.

## Highlights

- **Interactive and headless:** use the terminal interface or run a single prompt
  with `oc2 run`.
- **Tools and integrations:** work with files and shell commands, connect MCP
  servers, and use agents and skills.
- **Sessions and context:** retain session history and manage the active context
  with built-in Dynamic Context Pruning (DCP).
- **Native runtime:** no Node, Bun, or JavaScript plugin host is required to run
  `oc2`. External MCP servers and development tools have their own dependencies.

The project is under development and does not claim full OpenCode parity.
See [project status and limitations](https://github.com/0FL01/oc/blob/HEAD/GOAL.md).

## Installation

Install from crates.io:

```sh
cargo install oc2 --locked
oc2 --version
```

Cargo builds from source. You need Rust **1.93 or later**, a C/C++ toolchain,
make, and CMake. The qualified platform is GNU/Linux x86_64 on Debian 13;
Fedora, macOS, and Windows have not been separately qualified. Make sure Cargo's
bin directory is on your `PATH`.

To install the current checkout instead, run this from the repository root:

```sh
cargo install --path crates/oc --locked --bin oc2
```

The published crate can differ from the current checkout; the examples below
describe the current source.

## Quick start

1. **Configure a provider and credentials.** For example, to enter an OpenAI
   API key in an interactive terminal:

   ```sh
   oc2 auth login openai --method key
   ```

   The key is entered at a prompt and stored locally, not passed as an argument.
   Saving it does not validate server authorization or select a model. Other
   providers use their own configuration and credentials.

2. **Start the TUI** from your project directory:

   ```sh
   oc2
   ```

   In the TUI, use `/models` to choose an available model, then enter your
   prompt. The model must be available to your provider account. Bare `oc2`
   requires a terminal on both stdin and stdout.

3. **For headless use**, set a default model in your project `opencode.json`
   or global configuration. Merge the field below into an existing file rather
   than overwriting it, and replace the example with an actual model reference:

   ```json
   {
     "model": "openai/your-model-id"
   }
   ```

   Then run a prompt from the project directory:

   ```sh
   oc2 run "Explain this project"
   ```

Use `oc2 --help` for the command reference. `oc2 models` lists enabled model
references without selecting one; it may fetch catalog metadata, and a listing
does not prove that a model is ready to accept requests.

## Configuration

`oc2` reads `opencode.json` or `opencode.jsonc` from the project and its global
configuration directory. The global directory is `OPENCODE_CONFIG_DIR` when
set, otherwise `$XDG_CONFIG_HOME/opencode` when set, or `~/.config/opencode`.
The supported schema is OpenCode 2 with declared native extensions. Unsupported
schema keys are rejected; OpenCode 1 configuration is not automatically migrated.

See the [configuration reference](https://github.com/0FL01/oc/blob/HEAD/docs/CONFIG.md)
for providers and options. The [example configuration](https://github.com/0FL01/oc/blob/HEAD/examples/opencode.jsonc)
is specific to its providers and MCP servers, not a universal starter config.
Do not put real credentials in source control.

TUI, run, auth, and sessions share a private native owner for the same user and
data root; there is no separate `oc service` to manage. A rebuilt binary or a
changed environment takes effect after the existing owner's leases finish and a
new owner starts.

## Security

Tools can read or change files and run commands with your user privileges, subject
to the configured permissions. **The shell is not a sandbox.** Shell and terminal
processes inherit the native owner's startup environment, which can include
credentials and SSH-agent access. Cancellation does not undo external effects.
Review the [security guidance](https://github.com/0FL01/oc/blob/HEAD/docs/SECURITY.md)
before granting access.

## Documentation

- [Tools and MCP](https://github.com/0FL01/oc/blob/HEAD/docs/TOOLS_MCP.md)
- [Architecture](https://github.com/0FL01/oc/blob/HEAD/docs/ARCHITECTURE.md)
- Development: [agent guide](https://github.com/0FL01/oc/blob/HEAD/AGENTS.md),
  [code map](https://github.com/0FL01/oc/blob/HEAD/docs/CODE_MAP.md), and
  [test plan](https://github.com/0FL01/oc/blob/HEAD/docs/TEST_PLAN.md)

Some linked documents are in Russian or use mixed Russian and English.

## License and attribution

Original project code is [MIT-licensed](https://github.com/0FL01/oc/blob/HEAD/LICENSE).
The `oc2`, `oc-adapters`, and `oc-tui` packages include source-derived DCP
components and declare `MIT AND AGPL-3.0-or-later`; this does not replace the MIT
license on original project code. See the [AGPL license text](https://github.com/0FL01/oc/blob/HEAD/crates/oc/LICENSE-AGPL-3.0-or-later).

Third-party licenses and attribution remain with the bundled components. See the
notices for [oc2](https://github.com/0FL01/oc/blob/HEAD/crates/oc/THIRD_PARTY_NOTICES.md),
[adapters](https://github.com/0FL01/oc/blob/HEAD/crates/oc-adapters/THIRD_PARTY_NOTICES.md),
and [TUI](https://github.com/0FL01/oc/blob/HEAD/crates/oc-tui/THIRD_PARTY_NOTICES.md),
plus [DCP](https://github.com/0FL01/oc/blob/HEAD/docs/DCP.md) and
[DCP presentation](https://github.com/0FL01/oc/blob/HEAD/docs/DCP_VIS38_PROVENANCE.md)
provenance.
