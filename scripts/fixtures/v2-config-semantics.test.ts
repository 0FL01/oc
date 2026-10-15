// Staged unchanged into the pinned oracle's core/test/config by the Python runner.
import { expect } from "bun:test"
import { Document, Info } from "@opencode/schema/config"
import { Effect, Schema } from "effect"
import { Config } from "@opencode/core/config"
import { ConfigProviderPlugin } from "@opencode/core/config/plugin/provider"
import { Model } from "@opencode/core/model"
import { Plugin } from "@opencode/core/plugin"
import { PluginHost } from "@opencode/core/plugin/host"
import { Provider } from "@opencode/core/provider"
import { testEffect } from "../lib/effect"
import { PluginTestLayer } from "../plugin/fixture"
import fixture from "../fixture/oc-native-v2-config.fixture.json"

const it = testEffect(PluginTestLayer)
const decode = Schema.decodeUnknownSync(Info)

for (const packageName of fixture.protocols) {
  for (const item of fixture.compatibility) {
    it.effect(`CFG14 ${packageName} / ${item.id}`, () =>
      Effect.gen(function* () {
        const documents = [fixture.base, ...item.documents, { providers: { fixture: { package: packageName } } }]
        const entries = documents.map((info) => new Document({ type: "document", info: decode(info) }))
        const plugin = yield* Plugin.Service
        const host = yield* PluginHost.make(plugin)
        yield* ConfigProviderPlugin.Plugin.effect(host).pipe(Effect.provide(Config.testLayer(entries)))
        const models = yield* Model.Service
        const model = yield* models.get(Provider.ID.make("fixture"), Model.ID.make("m"))
        expect(model).toBeDefined()
        expect(model?.compatibility ?? {}).toEqual(item.expected)
      }),
    )
  }
}
