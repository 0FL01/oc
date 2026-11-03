import { afterEach, beforeEach, expect, jest, test } from "bun:test"
import { Effect, Exit, Queue, Scope, Stream } from "effect"
import plugin from "../plugins/opus-cache/index.js"

const interval = 45 * 60 * 1000
let cleanup

beforeEach(() => {
  jest.useFakeTimers()
  jest.setSystemTime(new Date("2026-11-02T11:40:00Z"))
})

afterEach(async () => {
  if (cleanup) await cleanup()
  cleanup = undefined
  jest.useRealTimers()
})

async function setup() {
  const hooks = {}
  const calls = []
  let hold = false
  let cancelled = false
  const queue = await Effect.runPromise(Queue.unbounded())
  const scope = await Effect.runPromise(Scope.make())
  const ctx = {
    session: {
      hook: (name, fn) => Effect.sync(() => { hooks[`session.${name}`] = fn }),
      generate: (input) => Effect.gen(function* () {
        calls.push(input)
        if (hold) yield* Effect.never.pipe(Effect.ensuring(Effect.sync(() => { cancelled = true })))
      }),
    },
    tool: { hook: (name, fn) => Effect.sync(() => { hooks[`tool.${name}`] = fn }) },
    event: { subscribe: ({ types }) => Stream.fromQueue(queue).pipe(Stream.filter((event) => types.includes(event.type))) },
  }
  await Effect.runPromise(plugin.effect(ctx).pipe(Effect.provideService(Scope.Scope, scope)))
  cleanup = () => Effect.runPromise(Scope.close(scope, Exit.void))
  const emit = async (type, data) => {
    await Effect.runPromise(Queue.offer(queue, { type, data }))
    await Effect.runPromise(Effect.yieldNow)
    await Effect.runPromise(Effect.yieldNow)
  }
  const context = () => Effect.runPromise(hooks["session.context"]({
    sessionID: "parent", agent: "Build-Opus", model: { providerID: "ludka2", id: "claude-agent/opus", variant: "high" },
  }))
  const before = (id) => Effect.runPromise(hooks["tool.execute.before"]({ tool: "subagent", sessionID: "parent", id }))
  const after = (id, metadata) => Effect.runPromise(hooks["tool.execute.after"]({
    tool: "subagent", sessionID: "parent", id, status: "completed", result: { metadata },
  }))
  const streamed = async () => {
    await emit("session.step.started", { sessionID: "parent", assistantMessageID: "step", started: Date.now() })
    await emit("session.step.streamed", { sessionID: "parent", assistantMessageID: "step" })
  }
  return { calls, emit, context, before, after, streamed, hold: () => { hold = true }, cancelled: () => cancelled }
}

test("warms the parent during a foreground subagent and stops after completion", async () => {
  const s = await setup()
  await s.context()
  await s.before("call")
  await s.streamed()
  jest.advanceTimersByTime(interval - 1)
  expect(s.calls).toHaveLength(0)
  jest.advanceTimersByTime(1)
  await Promise.resolve()
  expect(s.calls).toHaveLength(1)
  expect(s.calls[0]).toEqual({
    sessionID: "parent", prompt: "This is a keep-alive request. Do not perform any work or use tools. Reply with exactly: OK",
  })
  await s.after("call", { sessionID: "child", status: "completed" })
  jest.advanceTimersByTime(interval * 2)
  expect(s.calls).toHaveLength(1)
})

test("keeps one timer for background children and stops after their inbox notifications", async () => {
  const s = await setup()
  await s.context()
  await s.before("one")
  await s.before("two")
  await s.after("one", { sessionID: "child-one", status: "running" })
  await s.after("two", { sessionID: "child-two", status: "running" })
  await s.streamed()
  await s.emit("session.execution.succeeded", { sessionID: "parent" })
  await s.emit("session.inbox.enqueued", {
    sessionID: "parent", inboxID: "user-prompt", item: { type: "user", delivery: "steer", payload: { text: "Continue" } },
  })
  jest.advanceTimersByTime(interval)
  await Promise.resolve()
  expect(s.calls).toHaveLength(1)
  for (const childID of ["child-one", "child-two"]) {
    await s.emit("session.inbox.enqueued", {
      sessionID: "parent", inboxID: `notification-${childID}`,
      item: {
        type: "synthetic", delivery: "steer",
        payload: { text: "Child completed", metadata: { source: "subagent", childID, agent: "Worker", state: "completed" } },
      },
    })
  }
  await s.context()
  await s.streamed()
  await s.emit("session.execution.succeeded", { sessionID: "parent" })
  jest.advanceTimersByTime(interval * 2)
  expect(s.calls).toHaveLength(1)
})

test("interrupts a pending warming request when the foreground subagent finishes", async () => {
  const s = await setup()
  s.hold()
  await s.context()
  await s.before("call")
  await s.streamed()
  jest.advanceTimersByTime(interval)
  await Effect.runPromise(Effect.yieldNow)
  expect(s.calls).toHaveLength(1)
  await s.after("call", { sessionID: "child", status: "completed" })
  expect(s.cancelled()).toBe(true)
})
