# Owner startup failure resolved from the trace

## Result

The traced owner run proved the 401 is a stale credential exported in the failing zsh: `LUDKA2_API_KEY fingerprint=1101b285` versus the working identity `54f454fc` available from `~/.config/opencode/secrets.env`, which the `oc` alias loads. Config root, sources, selected model, provider, base URL and `provider.headers configured=0` matched the working smoke exactly; the catalog answered 401 only for the stale key. The owner-approved `with-oc2-secrets` wrapper + `alias oc2` now load the same secrets file before `exec`ing the native binary; `~/.zshrc` syntax check and a PTY run with a deliberately stale exported key both pass (trace: `fingerprint=54f454fc`, `status=200`, Home, exit 0, stale value absent). Only presence and SHA-256 prefixes were ever computed; no credential value was read, copied, printed or stored.

## Checks

Owner's `~/.local/state/oc/startup-trace.log`; working smoke trace in `trace-report.md`; `sha256sum` prefix comparison of the secrets-file environment in a subshell; `zsh -n ~/.zshrc` exit 0; PTY wrapper check exit 0 (`wrapper_ok: True`); pre-edit `~/.zshrc.bak-oc2`. No paid turn, no raw secret, no real response body.

## Risks

The alias activates in new shells or after `source ~/.zshrc`; the current terminal keeps the stale export until then (the wrapper overrides it anyway). No visual, resource or READY gate follows from this fix. T44 remains paused.

## Next

Owner sources `~/.zshrc` (or opens a new terminal) and confirms `oc2` reaches Home. Then resume T44: V07 S05/S06/S08, S07 measurements, V08–V09 VIS01–VIS24 and product gates.
