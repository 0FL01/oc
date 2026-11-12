import { Plugin } from "@opencode/plugin"

// Presence in effective server configuration also discovers the ./tui export.
// The TS service remains the only backend; this entry starts no other runtime.
export default Plugin.define({ id: "oc2-rust-tui", setup() {} })
