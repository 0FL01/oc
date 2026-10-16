// Development-only actual donor theme/native SyntaxStyle + OpenTUI converter.
// Token input comes from the real pinned Web Tree-sitter asset probe, not native
// expectations. No renderer mock, copied syntax rules or production JS host.
import path from "node:path"
import fs from "node:fs/promises"

const root = path.resolve(import.meta.dir, "..")
const sdk = "/home/opencode/.cache/opencode-tmp/opencode/syntax-reference-sdk"
const temporary = await fs.mkdtemp(path.join(sdk, "theme-reference-"))
try {
// Bundle unchanged donor modules in memory solely to resolve their workspace
// imports against the exact isolated SDK. Do not mutate the donor checkout.
const bundle = await Bun.build({
  entrypoints: [path.join(root, "opencode/packages/theme/src/tui/index.ts")],
  target: "bun",
  outdir: temporary, write: true,
  plugins: [{ name: "pinned-theme-sdk", setup(build) {
    build.onResolve({ filter: /^(@opentui\/core|effect)(\/.*)?$/ }, (args) => ({
      path: Bun.resolveSync(args.path, sdk), external: true,
    }))
  } }],
})
if (!bundle.success) throw new AggregateError(bundle.logs)
const { migrateV1, resolveThemeDocument, generateSyntax } = await import(
  bundle.outputs[0].path)
const { treeSitterToTextChunks } = await import(Bun.resolveSync("@opentui/core", sdk))
const language = process.argv[2] ?? "rust"
const source = process.argv[3] ?? "let x = 1; // note\n"
const probe = Bun.spawn({
  cmd: ["node", path.join(root, "scripts/syntax_reference.mjs"), language, source],
  stdout: "pipe", stderr: "pipe",
})
const [result, stderr, code] = await Promise.all([
  new Response(probe.stdout).text(), new Response(probe.stderr).text(), probe.exited,
])
if (code) throw new Error(stderr)
const reference = JSON.parse(result)
if (reference.query.status !== "loaded") throw new Error(reference.query.error)
const utf16 = (byte: number) => Buffer.from(source).subarray(0, byte).toString("utf8").length
const captures = reference.query.tokens.map((capture: {start: number, end: number, scope: string}) =>
  [utf16(capture.start), utf16(capture.end), capture.scope])
const asset = await Bun.file(path.join(root, "opencode/packages/tui/src/theme/assets/opencode.json")).json()
for (const mode of ["dark", "light"] as const) {
  const theme = resolveThemeDocument(migrateV1(asset), mode)
  const style = generateSyntax(theme)
  try {
    const chunks = treeSitterToTextChunks(source, captures, style, { enabled: false })
    console.log(JSON.stringify({ language, mode, reference, chunks: chunks.map((chunk) => ({
      text: chunk.text, fg: chunk.fg?.toInts(), bg: chunk.bg?.toInts(), attributes: chunk.attributes,
    })) }))
  } finally {
    style.destroy()
  }
}
} finally {
  await fs.rm(temporary, { recursive: true })
}
