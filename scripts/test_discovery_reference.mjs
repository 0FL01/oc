// OFFLINE oracle tests. Node is only a development aid, not an oc dependency.
// These validate the supplied JS reference, NOT a Rust implementation.
import assert from 'node:assert/strict'
import test from 'node:test'
import OpenProxyModels from '../references/openproxy-models.user.mjs'

const model = (id = 'fixture/future-model', extra = {}) => ({
  id, context_length: 700000, max_completion_tokens: 32000,
  opencode: { limit: { input: 600000 }, tool_call: true,
    modalities: { input: ['text', 'image'], output: ['text'] } }, ...extra,
})
const config = (models = {}) => ({ provider: { ludka2: {
  npm: '@ai-sdk/openai', options: { baseURL: 'https://example.invalid/v1/',
    apiKey: 'unit-test-placeholder', headers: { 'x-fixture': 'yes' } }, models,
} } })
const response = (data) => new Response(JSON.stringify({ object: 'list', data }), {
  status: 200, headers: { 'content-type': 'application/json' },
})
async function run(c, responses) {
  const originalFetch = globalThis.fetch, originalWarn = console.warn
  const requests = [], warnings = []
  globalThis.fetch = async (url, options) => {
    requests.push({ url: String(url), options })
    assert.ok(responses.length, 'unexpected extra request')
    const next = responses.shift()
    if (next instanceof Error) throw next
    return next
  }
  console.warn = (text) => warnings.push(String(text))
  try { await (await OpenProxyModels()).config(c) }
  finally { globalThis.fetch = originalFetch; console.warn = originalWarn }
  return { requests, warnings }
}

test('new model ID needs no hardcoded registry; URL/auth and 500k clamp', async () => {
  const c = config()
  const { requests, warnings } = await run(c, [response([model()])])
  const m = c.provider.ludka2.models['fixture/future-model']
  assert.equal(m.limit.context, 500000)
  assert.equal(m.limit.input, 500000)
  assert.equal(m.limit.output, 32000)
  assert.equal(requests[0].url, 'https://example.invalid/v1/models')
  assert.equal(requests[0].options.headers.get('Authorization'), 'Bearer unit-test-placeholder')
  assert.equal(requests[0].options.redirect, 'error')
  assert.deepEqual(warnings, [])
})
test('local overrides/source suffix, remote variant allowlist suppresses defaults', async () => {
  const c = config({ 'fixture/future-model': { name: 'glm local', limit: { output: 42 },
    variants: { high: { reasoningEffort: 'custom-value' } } } })
  await run(c, [response([model(undefined, { opencode: { source: 'fixture-source',
    variants: { medium: { reasoningEffort: 'medium' } } } })])])
  const m = c.provider.ludka2.models['fixture/future-model']
  assert.equal(m.name, 'GLM local · fixture-source')
  assert.equal(m.limit.output, 42)
  assert.equal(m.variants.none.disabled, true)
  assert.equal(m.variants.high.reasoningEffort, 'custom-value')
})
test('successful publication removes local-only models', async () => {
  const c = config({ removed: { name: 'old' } })
  await run(c, [response([model()])])
  assert.deepEqual(Object.keys(c.provider.ludka2.models), ['fixture/future-model'])
})
test('whole response is atomic when any row is invalid', async () => {
  const c = config({ kept: { name: 'old' } }), before = structuredClone(c)
  await run(c, [response([model(), model('bad', { context_length: -1 })])])
  assert.deepEqual(c, before)
})
test('remote options/headers/npm cannot override configured connection', async () => {
  const c = config(), original = structuredClone(c.provider.ludka2.options)
  await run(c, [response([model(undefined, { npm: 'malicious', options: { apiKey: 'bad' },
    opencode: { options: { baseURL: 'https://bad.invalid' }, headers: { Authorization: 'bad' } } })])])
  assert.deepEqual(c.provider.ludka2.options, original)
  const m = c.provider.ludka2.models['fixture/future-model']
  assert.equal(m.options, undefined)
  assert.equal(m.headers, undefined)
  assert.equal(m.npm, undefined)
})
test('401 is terminal; configured models retained; sanitized warning', async () => {
  const c = config({ kept: { name: 'old' } }), before = structuredClone(c)
  const { requests, warnings } = await run(c, [new Response('sensitive-fixture-value', { status: 401 })])
  assert.equal(requests.length, 1)
  assert.deepEqual(c, before)
  assert.match(warnings[0], /HTTP 401/)
  assert.doesNotMatch(warnings[0], /sensitive-fixture-value|example\.invalid|unit-test-placeholder/)
})
test('disabled provider makes no network request', async () => {
  const c = config(); c.disabled_providers = ['ludka2']
  assert.equal((await run(c, [])).requests.length, 0)
})
test('missing enabled-provider selection makes no request', async () => {
  const c = config(); c.enabled_providers = ['other']
  assert.equal((await run(c, [])).requests.length, 0)
})
test('duplicates preserve the supplied Object.fromEntries last-wins behavior', async () => {
  const c = config()
  await run(c, [response([model('same'), model('same', { opencode: { name: 'Last' } })])])
  assert.equal(c.provider.ludka2.models.same.name, 'Last')
})
test('incomplete limit is deleted rather than fabricated', async () => {
  const c = config()
  await run(c, [response([{ id: 'fixture/no-limits', opencode: { limit: { input: 120 } } }])])
  assert.equal(c.provider.ludka2.models['fixture/no-limits'].limit, undefined)
})
test('invalid envelope is terminal rather than silently successful', async () => {
  const c = config({ kept: {} })
  const { requests } = await run(c, [new Response(JSON.stringify({ data: [model()] }))])
  assert.equal(requests.length, 1)
  assert.deepEqual(Object.keys(c.provider.ludka2.models), ['kept'])
})
test('empty cold list retries and then publishes validated result', async () => {
  const c = config({ old: {} })
  const { requests } = await run(c, [response([]), response([model()])])
  assert.equal(requests.length, 2)
  assert.deepEqual(Object.keys(c.provider.ludka2.models), ['fixture/future-model'])
})
test('invalid JSON retries while invalid metadata does not', async () => {
  const c = config()
  const { requests } = await run(c, [new Response('not json'), response([model()])])
  assert.equal(requests.length, 2)
})
test('503 is bounded to four attempts and preserves catalog', async () => {
  const c = config({ kept: {} })
  const { requests } = await run(c, Array.from({ length: 4 }, () => new Response('', { status: 503 })))
  assert.equal(requests.length, 4)
  assert.deepEqual(Object.keys(c.provider.ludka2.models), ['kept'])
})
test('credential-bearing URL is rejected before network', async () => {
  const c = config()
  c.provider.ludka2.options.baseURL = 'https://user:password@example.invalid/v1'
  const { requests, warnings } = await run(c, [])
  assert.equal(requests.length, 0)
  assert.doesNotMatch(warnings[0], /password|example\.invalid/)
})
