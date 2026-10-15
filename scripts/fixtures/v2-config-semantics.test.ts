// Staged unchanged into the pinned oracle's core/test/config by the Python runner.
import { expect } from "bun:test"
import { Document, Info } from "@opencode/schema/config"
import { Effect, Exit, Schema } from "effect"
import { Config } from "@opencode/core/config"
import { ConfigProviderPlugin } from "@opencode/core/config/plugin/provider"
import { ConfigAgentPlugin } from "@opencode/core/config/plugin/agent"
import { Agent } from "@opencode/core/agent"
import { Permission } from "@opencode/core/permission"
import { Global } from "@opencode/util/global"
import { Model } from "@opencode/core/model"
import { ModelResolver } from "@opencode/core/model-resolver"
import { Plugin } from "@opencode/core/plugin"
import { PluginHost } from "@opencode/core/plugin/host"
import { Provider } from "@opencode/core/provider"
import { LLM, Message, ToolDefinition } from "@opencode/ai"
import { compileRequest } from "@opencode/ai/route/client"
import { testEffect } from "../lib/effect"
import { PluginTestLayer } from "../plugin/fixture"
import fixture from "../fixture/oc-native-v2-config.fixture.json"

const it = testEffect(PluginTestLayer)
const decode = Schema.decodeUnknownSync(Info)

const configuredModel = Effect.fn(function* (documents: unknown[]) {
  const entries = documents.map((info) => new Document({ type: "document", info: decode(info) }))
  const plugin = yield* Plugin.Service
  const host = yield* PluginHost.make(plugin)
  yield* ConfigProviderPlugin.Plugin.effect(host).pipe(Effect.provide(Config.testLayer(entries)))
  const models = yield* Model.Service
  const model = yield* models.get(Provider.ID.make("fixture"), Model.ID.make("m"))
  if (model === undefined) throw new Error("Fixture model was not materialized")
  return model
})

for (const item of fixture.schema_cases) {
  it.effect(`CFG11/14 canonical schema / ${item.id}`, () => Effect.sync(() => {
    expect(Exit.isSuccess(Schema.decodeUnknownExit(Info)(item.document))).toBe(item.schema_valid)
  }))
}

for (const item of fixture.permission_evaluator_cases) {
  it.effect(`CFG17 evaluator / ${item.id}`, () => Effect.sync(() => {
    const rules = Schema.decodeUnknownSync(Permission.Ruleset)(item.rules)
    expect(Permission.evaluate(item.action, item.resource, rules).effect).toBe(item.expected)
  }))
}

for (const item of fixture.permission_cases) {
  it.effect(`CFG17 documents / ${item.id}`, () => Effect.gen(function* () {
    const entries = [fixture.permission_base, ...item.documents].map(
      (info) => new Document({ type: "document", info: decode(info) }),
    )
    const plugin = yield* Plugin.Service
    const host = yield* PluginHost.make(plugin)
    yield* ConfigAgentPlugin.Plugin.effect(host).pipe(
      Effect.provide(Config.testLayer(entries)),
      Effect.provide(Global.layerWith({})),
    )
    const agents = yield* Agent.Service
    const agent = yield* agents.get(Agent.ID.make("fixture"))
    if (agent === undefined) throw new Error("Fixture agent was not materialized")
    expect(Permission.evaluate(item.action, item.resource, agent.permissions).effect).toBe(item.expected)
  }))
}

function cacheBlocks(value: unknown): number {
  if (Array.isArray(value)) return value.reduce((sum, item) => sum + cacheBlocks(item), 0)
  if (value === null || typeof value !== "object") return 0
  return Object.entries(value).reduce(
    (sum, [key, item]) => sum + Number(key === "cache_control" && item !== undefined) + cacheBlocks(item),
    0,
  )
}

for (const packageName of fixture.protocols) {
  for (const item of fixture.settings_cases) {
    it.effect(`CFG16 ${packageName} / ${item.id}`, () =>
      Effect.gen(function* () {
        const model = yield* configuredModel([
          fixture.base,
          { providers: { fixture: { package: packageName, settings: item.provider, models: {
            m: { settings: item.model, variants: [{ id: "v", settings: item.variant }] },
          } } } },
        ])
        const resolver = yield* ModelResolver.Service.pipe(Effect.provide(ModelResolver.layer))
        for (const variant of [undefined, Model.VariantID.make("v")]) {
          const selected = yield* ModelResolver.withVariant(model, variant)
          const resolved = yield* resolver.resolveModel(model, variant)
          expect({
            timeout: resolved.model.defaults?.http?.timeout ?? null,
            headerTimeout: resolved.model.defaults?.http?.headerTimeout,
            chunkTimeout: resolved.model.defaults?.http?.chunkTimeout,
          }).toEqual(item.expected_http)
          expect(resolved.transport ?? null).toEqual(item.expected_transport)
          expect(resolved.chunkTimeout ?? null).toEqual(item.provider.chunkTimeout ?? null)
          expect(selected.settings?.reasoningEffort).toBe(variant === undefined ? "medium" : "high")
        }
      }),
    )
  }
  for (const item of fixture.compatibility) {
    it.effect(`CFG14 ${packageName} / ${item.id}`, () =>
      Effect.gen(function* () {
        const documents = [fixture.base, ...item.documents, { providers: { fixture: { package: packageName } } }]
        const model = yield* configuredModel(documents)
        expect(model.compatibility ?? {}).toEqual(item.expected)
      }),
    )
  }
  for (const item of fixture.cache_requests) {
    it.effect(`CFG15 ${packageName} / ${item.id}`, () =>
      Effect.gen(function* () {
        const model = yield* configuredModel([
          fixture.base,
          ...item.documents,
          { providers: { fixture: { package: packageName } } },
        ])
        expect(model.headers).toEqual(fixture.base.providers.fixture.headers)
        const resolved = yield* ModelResolver.fromCatalogModel(model)
        const minimal = "minimal_input" in item && item.minimal_input === true
        const compiled = yield* compileRequest(LLM.request({
          model: resolved,
          cache: item.cache === "none" ? "none" : "auto",
          promptCacheKey: "provide_key" in item && item.provide_key === false ? undefined : fixture.prompt_cache_key,
          system: minimal ? undefined : "initial",
          messages: [Message.user("first"), Message.assistant("answer"), Message.user("last")],
          tools: minimal ? undefined : [ToolDefinition.make({ name: "read", description: "read", inputSchema: { type: "object" } })],
          generation: { maxTokens: 64 },
        }))
        const wire = fixture.wire_names[packageName as keyof typeof fixture.wire_names]
        const expected = item.expected_key[wire as keyof typeof item.expected_key]
        expect(compiled.body.prompt_cache_key).toEqual(expected ? fixture.prompt_cache_key : undefined)
        expect(cacheBlocks(compiled.body)).toBe(wire === "messages" ? item.expected_blocks : 0)
        if (wire === "messages" && minimal) {
          expect(compiled.body.system).toBeUndefined()
          expect(compiled.body.tools).toBeUndefined()
        }
      }),
    )
  }
}
