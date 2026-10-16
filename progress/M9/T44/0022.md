# Owner startup failure resolved from the trace

## Result

The traced owner run proved the 401 is a stale credential exported in the failing zsh: `LUDKA2_API_KEY fingerprint=1101b285` versus the working identity `54f454fc` available from `~/.config/opencode/secrets.env`, which the `oc` alias loads. Config root, sources, selected model, provider, base URL and `provider.headers configured=0` matched the working smoke exactly; the catalog answered 401 only for the stale key. Only presence and SHA-256 prefixes were computed; no credential value was read, copied, printed or stored. Fix is credential sourcing, not a binary or config defect. Full evidence and plan in `owner-catalog-recon.md`.

## Checks

`~/.local/state/oc/startup-trace.log` from the owner's `OC_STARTUP_TRACE_FINGERPRINT=1 oc2` run; working smoke trace from `evidence/tui/recovery-startup/trace-report.md`; `sha256sum` prefix comparison of the secrets-file environment in a subshell. No paid turn, no raw secret, no real response body.

## Risks

The owner's shell still holds the stale value until it re-sources the secrets file; the durable `oc2` wrapper is proposed but not applied. No visual, resource or READY gate follows from this fix. T44 remains paused.

## Next

Owner runs `set -a; source ~/.config/opencode/secrets.env; set +a; OC_STARTUP_TRACE_FINGERPRINT=1 oc2` and confirms `fingerprint=54f454fc`, `status=200` and Home; on their word add the `oc2` wrapper to `~/.zshrc`. Then resume T44: V07 S05/S06/S08, S07 measurements, V08–V09 VIS01–VIS24 and product gates.
