// Read-only pinned-source slice oracle; no donor install or source changes.
// bun scripts/compaction_normalization_oracle.ts /absolute/path/to/effect/package
import { isDeepStrictEqual } from "node:util"
import { resolve } from "node:path"
const effectRoot = resolve(process.argv[2])
if ((await Bun.file(resolve(effectRoot,"package.json")).json()).version !== "4.0.0-rc.112")
  throw new Error("Oracle requires pinned Effect 4.0.0-rc.112")
const {Schema,Option,SchemaGetter} = await import(resolve(effectRoot,"dist/index.js"))
const transpiler = new Bun.Transpiler({loader:"ts"})
const schema = (await Bun.file(new URL("../opencode/packages/schema/src/schema.ts",import.meta.url)).text()).split("\n").slice(2,18).join("\n").replaceAll("export ", "")
const {NonNegativeInt,optional} = new Function("Schema","Option","SchemaGetter",`${transpiler.transformSync(schema)}\nreturn {NonNegativeInt,optional}`)(Schema,Option,SchemaGetter)
const compaction = (await Bun.file(new URL("../opencode/packages/schema/src/config/compaction.ts",import.meta.url)).text()).split("\n").slice(5).join("\n").replaceAll("export ", "")
const ConfigCompaction = new Function("Schema","NonNegativeInt","optional",`${transpiler.transformSync(compaction)}\nreturn {Info,Keep}`)(Schema,NonNegativeInt,optional)
const {isRecord} = await import("../opencode/packages/ai/src/utils/record.ts")
const source = await Bun.file(new URL("../opencode/packages/core/src/config/normalize.ts",import.meta.url)).text()
const lines = source.split("\n")
const slice = (start:number,end:number) => lines.slice(start-1,end).join("\n")
const code = [slice(318,375),slice(683,698),slice(711,717),slice(765,782),slice(798,800)].join("\n")
const executable = transpiler.transformSync(code)
const normalize = new Function("Schema","Option","ConfigCompaction","isRecord","isDeepStrictEqual", `const options = {errors:"all",onExcessProperty:"ignore",propertyOrder:"original"};\n${executable}\nreturn normalizeCompaction`)(Schema,Option,ConfigCompaction,isRecord,isDeepStrictEqual)
const fixture = await Bun.file(new URL("../crates/oc-adapters/tests/fixtures/compaction_normalization.json",import.meta.url)).json()
for (const test of fixture.cases) {
  const encoded = {}, diagnostics = []
  normalize({compaction:test.input},encoded,diagnostics)
  if (!isDeepStrictEqual(encoded,test.encoded) || !isDeepStrictEqual(diagnostics,test.diagnostics)) {
    console.error(JSON.stringify({case:test.name,encoded,diagnostics}))
    process.exit(1)
  }
}
console.log(`PASS pinned normalization ${fixture.cases.length} differential fixtures`)
