// Development-only corpus from the real pinned OpenTUI worker and native style
// library. This never supplies a substitute lexer, parser, query or style oracle.
import fs from "node:fs/promises"
import path from "node:path"
import crypto from "node:crypto"

const root = path.resolve(import.meta.dir, "..")
const sdk = "/home/opencode/.cache/opencode-tmp/opencode/syntax-reference-sdk"
const assets = path.join(root, "crates/oc-tui/assets/syntax")
const wasm = "/home/opencode/.cache/opencode-tmp/opencode/syntax-reference-wasm"
const temporary = await fs.mkdtemp(path.join(sdk, "corpus-reference-"))
const sha = (bytes: Uint8Array) => crypto.createHash("sha256").update(bytes).digest("hex")
let client
try {
  const bundle = await Bun.build({
    entrypoints: [path.join(root, "opencode/packages/theme/src/tui/index.ts")],
    target: "bun", outdir: temporary, write: true,
    plugins: [{ name: "pinned-theme-sdk", setup(build) {
      build.onResolve({ filter: /^(@opentui\/core|effect)(\/.*)?$/ }, (args) => ({
        path: Bun.resolveSync(args.path, sdk), external: true,
      }))
    } }],
  })
  if (!bundle.success) throw new AggregateError(bundle.logs)
  const { migrateV1, resolveThemeDocument, generateSyntax } = await import(bundle.outputs[0].path)
  const { TreeSitterClient, treeSitterToTextChunks } = await import(Bun.resolveSync("@opentui/core", sdk))
  const manifestBytes = await fs.readFile(path.join(assets, "manifest.json"))
  const manifest = JSON.parse(manifestBytes.toString())
  const fixtureBytes = await fs.readFile(path.join(assets, "fixtures.json"))
  const fixtures = JSON.parse(fixtureBytes.toString())
  if (Object.keys(fixtures).length !== manifest.grammars.length) throw Error("Incomplete corpus")
  // Download exactly the real public/reference assets using the existing probe;
  // its query diagnostics are not substituted for this actual worker execution.
  const prepare = Bun.spawn({ cmd: ["node", path.join(root, "scripts/syntax_reference.mjs")],
    stdout: "pipe", stderr: "pipe" })
  const [prepared, errors, exit] = await Promise.all([
    new Response(prepare.stdout).text(), new Response(prepare.stderr).text(), prepare.exited,
  ])
  if (exit) throw Error(errors)
  const languages = prepared.trim().split("\n").map(line => JSON.parse(line))
  if (languages.length !== manifest.grammars.length || languages.some(e => e.query.status !== "loaded"))
    throw Error("Reference grammar/query unavailable: " + prepared)
  client = new TreeSitterClient({ dataPath: path.join(temporary, "worker") })
  await client.initialize()
  for (const entry of manifest.grammars) {
    client.addFiletypeParser({
      filetype: entry.name, aliases: entry.aliases,
      wasm: path.join(wasm, entry.name + ".wasm"),
      queries: {
        highlights: [path.join(assets, entry.queries.highlights.file)],
        ...(entry.queries.injections ? { injections: [path.join(assets, entry.queries.injections.file)] } : {}),
      },
      injectionMapping: entry.injection_mapping,
    })
  }
  const themeAsset = await Bun.file(path.join(root, "opencode/packages/tui/src/theme/assets/opencode.json")).json()
  const styles = new Map(["dark", "light"].map(mode =>
    [mode, generateSyntax(resolveThemeDocument(migrateV1(themeAsset), mode))]))
  const results = []
  try {
    for (const entry of manifest.grammars) {
      if (!Array.isArray(fixtures[entry.name]) || fixtures[entry.name].length < 2) throw Error(entry.name + ": incomplete fixtures")
      for (const [index, source] of fixtures[entry.name].entries()) {
        const result = await client.highlightOnce(source, entry.name)
        if (result.error || !result.highlights) throw Error(entry.name + ": " + result.error)
        const byte = (at: number) => Buffer.byteLength(source.slice(0, at), "utf8")
        const tokens = result.highlights.map(([start, end, scope, meta]) =>
          ({ start: byte(start), end: byte(end), scope, ...(meta ? { meta } : {}) }))
        const styled = Object.fromEntries([...styles].map(([mode, style]) => {
          const chunks = treeSitterToTextChunks(source, result.highlights, style, { enabled: false })
          if (chunks.map(c => c.text).join("") !== source) throw Error(entry.name + ": reference text loss")
          return [mode, chunks.map(c => ({ text: c.text, fg: c.fg?.toInts(), bg: c.bg?.toInts(), attributes: c.attributes ?? 0 }))]
        }))
        results.push({ language: entry.name, index, source, tokens, styled })
      }
    }
  } finally {
    for (const style of styles.values()) style.destroy()
  }
  const output = {
    version: 1, donor: manifest.donor, opentui: manifest.opentui,
    manifest_sha256: sha(manifestBytes), fixtures_sha256: sha(fixtureBytes),
    reference: languages.map(e => ({ name: e.name, wasm_sha256: e.wasm_sha256, abi: e.version, node_types: e.node_type_count })),
    cases: results,
  }
  await fs.writeFile(path.join(assets, "fixtures.reference.json"), JSON.stringify(output) + "\n")
  console.log(JSON.stringify({ status: "ACTUAL_REFERENCE_CORPUS", languages: languages.length,
    cases: results.length, styles: results.length * styles.size }))
} finally {
  if (client) await client.destroy()
  await fs.rm(temporary, { recursive: true })
}
