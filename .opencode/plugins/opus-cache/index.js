import { Effect, Fiber, Scope, Stream } from "effect"

const INTERVAL = 45 * 60 * 1000
const RETRY = 5 * 60 * 1000
const PROMPT = "This is a keep-alive request. Do not perform any work or use tools. Reply with exactly: OK"

export default {
  id: "local.opus-subagent-cache",
  effect: (ctx) => Effect.gen(function* () {
    const scope = yield* Scope.Scope
    const sessions = new Map()
    let listening = true

    const cancel = (state) => Effect.gen(function* () {
      const timer = state.timer
      state.timer = undefined
      if (timer) yield* Fiber.interrupt(timer)
    })

    const stop = (sessionID) => Effect.gen(function* () {
      const state = sessions.get(sessionID)
      if (!state) return
      sessions.delete(sessionID)
      yield* cancel(state)
    })

    const schedule = (sessionID, state) => Effect.gen(function* () {
      if (!listening || sessions.get(sessionID) !== state || state.busy || state.timer) return
      if (!state.calls.size && !state.children.size) return
      state.timer = yield* Effect.gen(function* () {
        yield* Effect.sleep(Math.max(0, state.last + INTERVAL - Date.now()))
        if (sessions.get(sessionID) !== state || state.busy) return
        state.last = Date.now()
        const success = yield* ctx.session.generate({ sessionID, prompt: PROMPT }).pipe(
          Effect.as(true),
          Effect.catch(() => Effect.sync(() => {
            console.warn("Opus cache warming failed", { sessionID })
            return false
          })),
        )
        if (sessions.get(sessionID) !== state) return
        if (!success) state.last = Date.now() - INTERVAL + RETRY
        state.timer = undefined
        yield* schedule(sessionID, state)
      }).pipe(Effect.forkIn(scope))
    })

    yield* ctx.session.hook("context", (event) => Effect.gen(function* () {
      const { sessionID, agent, model } = event
      if (!listening || !["Build-Opus", "Plan-Opus"].includes(agent) ||
          model.providerID !== "ludka2" || model.id !== "claude-agent/opus") {
        yield* stop(sessionID)
        return
      }
      let state = sessions.get(sessionID)
      if (!state) {
        state = { calls: new Set(), children: new Set(), finished: new Set(), busy: true, last: Date.now() }
        sessions.set(sessionID, state)
      }
      yield* cancel(state)
      state.agent = agent
      state.model = model
      state.busy = true
      state.last = Date.now()
      state.step = undefined
    }))

    yield* ctx.tool.hook("execute.before", (event) => Effect.gen(function* () {
      if (event.tool !== "subagent") return
      const state = sessions.get(event.sessionID)
      if (!state) return
      state.calls.add(event.id)
      yield* schedule(event.sessionID, state)
    }))

    yield* ctx.tool.hook("execute.after", (event) => Effect.gen(function* () {
      if (event.tool !== "subagent") return
      const state = sessions.get(event.sessionID)
      if (!state || !state.calls.delete(event.id)) return
      const metadata = event.status === "completed" ? event.result.metadata : undefined
      if (metadata?.status === "running" && typeof metadata.sessionID === "string") {
        if (!state.finished.delete(metadata.sessionID)) state.children.add(metadata.sessionID)
      }
      if (!state.calls.size) state.finished.clear()
      if (!state.calls.size && !state.children.size) yield* cancel(state)
      yield* schedule(event.sessionID, state)
    }))

    yield* ctx.event.subscribe({
      types: ["session.step.started", "session.step.streamed", "session.inbox.enqueued", "session.execution.failed",
        "session.execution.interrupted", "session.execution.succeeded", "session.deleted",
        "session.agent.selected", "session.model.selected"],
    }).pipe(
      Stream.runForEach((event) => Effect.gen(function* () {
        const { sessionID } = event.data
        const state = sessions.get(sessionID)
        if (!state) return
        if (event.type === "session.step.started") {
          if (event.data.started >= state.last) state.step = event.data.assistantMessageID
        } else if (event.type === "session.step.streamed") {
          if (state.step !== event.data.assistantMessageID) return
          state.busy = false
          yield* schedule(sessionID, state)
        } else if (event.type === "session.inbox.enqueued") {
          if (event.data.item.type !== "synthetic") return
          const metadata = event.data.item.payload.metadata
          if (metadata?.source !== "subagent" || typeof metadata.childID !== "string") return
          if (!state.children.delete(metadata.childID) && state.calls.size) state.finished.add(metadata.childID)
          if (!state.calls.size && !state.children.size) yield* cancel(state)
        } else if (event.type === "session.execution.succeeded") {
          if (!state.calls.size && !state.children.size) yield* stop(sessionID)
        } else if (event.type === "session.agent.selected") {
          if (event.data.agent !== state.agent) yield* stop(sessionID)
        } else if (event.type === "session.model.selected") {
          const model = event.data.model
          if (model.providerID !== state.model.providerID || model.id !== state.model.id ||
              model.variant !== state.model.variant) yield* stop(sessionID)
        } else {
          yield* stop(sessionID)
        }
      })),
      Effect.catch(() => Effect.sync(() => console.warn("Opus cache warming stopped: event stream failed"))),
      Effect.ensuring(Effect.gen(function* () {
        listening = false
        for (const sessionID of sessions.keys()) yield* stop(sessionID)
      })),
      Effect.forkIn(scope, { startImmediately: true }),
    )
  }),
}
