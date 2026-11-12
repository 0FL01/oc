import { Plugin } from "@opencode/plugin/tui"
import type { OpenCodeEvent } from "@opencode/client"
import { spawn } from "node:child_process"
import { Readable, Writable } from "node:stream"
import { decode, knownRefusal, MAX_FRAME, MAX_PENDING, VERSION } from "./protocol"
import type { Request } from "./protocol"
import { form, inbox, message, model, modelRef, permission, session, text } from "./projection"

export default Plugin.define({
  id: "oc2-rust-tui",
  async setup(context) {
    const host = context.lifecycle
    if (!host?.onShutdown) {
      context.ui.toast.show({ variant: "warning", message: "Rust TUI requires OpenCode's awaited shutdown hook; stock TUI remains active. See the plugin's upstream setup instructions." })
      return
    }
    const client = context.client
    const location = context.location ?? context.data.location.default()
    const abort = new AbortController()
    const signal = () => AbortSignal.any([abort.signal, AbortSignal.timeout(15_000)])
    const inFlight = new Set<number>()
    const queued: object[] = []
    let queuedBytes = 0
    let epoch = 0
    let activated = false
    let suspended = false
    let suspension: Promise<void> | undefined
    let stopped: Promise<void> | undefined
    let child: ReturnType<typeof spawn> | undefined
    let exited = Promise.resolve()
    let input: Writable | undefined
    let pump: Promise<void> | undefined
    let returnSession: string | null = null

    const resume = async () => {
      if (!suspended || context.renderer.isDestroyed || context.renderer.controlState !== "explicit_suspended") return
      suspended = false
      await context.renderer.resume()
    }
    const stop = () => stopped ??= (async () => {
      abort.abort()
      child?.kill("SIGTERM")
      const timer = setTimeout(() => child?.kill("SIGKILL"), 1000)
      await exited
      clearTimeout(timer)
      child?.stdio[3]?.destroy()
      child?.stdio[4]?.destroy()
      await pump
      await suspension?.catch(() => {})
      if (!host.shuttingDown) {
        // Route changes can update terminal-owned chrome. Do them only after
        // join, and navigate only an identity confirmed by this captured client.
        if (returnSession) {
          const info = await client.session.get({ sessionID: returnSession }, { signal: AbortSignal.timeout(1000) }).catch(() => undefined)
          if (info) context.ui.router.navigate({ type: "session", sessionID: info.id })
        }
        await resume()
      }
    })()
    const unregister = host.onShutdown(async () => { await stop(); await resume() })
    const destroyed = () => { void stop() }
    context.renderer.once("destroy", destroyed)

    function send(frame: object) {
      if (stopped) return
      const encoded = JSON.stringify(frame) + "\n"
      if (Buffer.byteLength(encoded) > MAX_FRAME) throw new Error("Rust TUI bridge frame limit exceeded")
      if (!activated && "event" in frame) {
        queuedBytes += Buffer.byteLength(encoded)
        if (queued.length >= 256 || queuedBytes > MAX_FRAME) throw new Error("Rust TUI bridge startup event overflow")
        queued.push(frame)
        return
      }
      if (!input || input.writableLength + Buffer.byteLength(encoded) > MAX_FRAME * 2) throw new Error("Rust TUI bridge output overflow")
      input.write(encoded, (error) => { if (error) fail("Rust TUI bridge output closed") })
    }
    function fail(reason: string) {
      if (stopped) return
      context.ui.toast.show({ variant: "warning", message: `${reason}. Returning to stock TUI; no requests were resent or backend execution interrupted.` })
      void stop()
    }
    async function page(sessionID: string, cursor: string | null) {
      const captured = epoch
      const result = await client.message.list({ sessionID, limit: 50, ...(cursor ? { cursor } : { order: "desc" }) }, { signal: signal() })
      return { epoch: captured, messages: result.data.toReversed().map(message), older: result.cursor.next ?? null, newer: result.cursor.previous ?? null }
    }
    async function open(sessionID: string) {
      const info = await client.session.get({ sessionID }, { signal: signal() })
      const [history, active, pending, permissions, forms, metadata] = await Promise.all([
        page(sessionID, null),
        client.session.active({ signal: signal() }),
        client.session.inbox.list({ sessionID }, { signal: signal() }),
        client.permission.request.list({ location: info.location }, { signal: signal() }),
        client.form.list({ location: info.location }, { signal: signal() }),
        catalog(info.location),
      ])
      // Resolve request ancestry through public facts, never numeric UI IDs or
      // a guessed native invocation binding. This map lives for this read only.
      const parents = new Map<string, string | undefined>([[info.id, info.parentID]])
      async function belongs(id: string) {
        for (let depth = 0; depth < 32; depth++) {
          if (id === sessionID) return true
          if (!parents.has(id)) {
            if (parents.size >= 128) throw new Error("OpenCode request ancestry limit exceeded")
            const ancestor = await client.session.get({ sessionID: id }, { signal: signal() })
            parents.set(id, ancestor.parentID)
          }
          const parent = parents.get(id)
          if (!parent) return false
          id = parent
        }
        throw new Error("OpenCode request ancestry depth exceeded")
      }
      if (permissions.data.length + forms.data.length > 128) throw new Error("OpenCode pending request limit exceeded")
      const ownedPermissions = []
      for (const value of permissions.data) if (await belongs(value.sessionID)) ownedPermissions.push(permission(value))
      const ownedForms = []
      for (const value of forms.data) if (await belongs(value.sessionID)) ownedForms.push(form(value))
      return { session: session(info, sessionID in active), catalog: metadata, page: history, running: sessionID in active, inbox: pending.flatMap((item) => { const value = inbox(item); return value ? [value] : [] }), permissions: ownedPermissions, forms: ownedForms }
    }
    async function catalog(where: typeof location) {
      const [agents, models, selected] = await Promise.all([
        client.agent.list({ location: where }, { signal: signal() }),
        client.model.list({ location: where, types: ["chat"] }, { signal: signal() }),
        client.model.default({ location: where }, { signal: signal() }),
      ])
      return {
        models: models.data.filter((entry) => entry.enabled).map(model),
        agents: agents.data.filter((agent) => !agent.hidden && agent.mode !== "subagent").map((agent, color_index) => ({ id: agent.id, description: agent.description ?? "", model: agent.model?.id ?? null, variant: agent.model?.variant ?? null, color_index })),
        agent_models: Object.fromEntries(agents.data.flatMap((agent) => agent.model ? [[agent.id, modelRef(agent.model)]] : [])),
        selection: { agent: null, model: selected.data ? modelRef(selected.data) : null },
      }
    }
    async function request(value: Request) {
      const options = { signal: signal() }
      switch (value.type) {
        case "open": {
          const view = await open(value.session)
          if (view.forms.some((item) => item.questions === null)) {
            returnSession = view.session.id
            fail("An unsupported OpenCode form remains pending; answer it in stock TUI")
          }
          return { type: "view", view }
        }
        case "sessions": {
          const [result, active] = await Promise.all([
            client.session.list({ limit: 100, order: "desc", parentID: null, ...(value.search ? { search: value.search } : {}) }, options),
            client.session.active(options),
          ])
          return { type: "sessions", sessions: result.data.map((info) => session(info, info.id in active)) }
        }
        case "history": return { type: "history", page: await page(value.session, value.cursor) }
        case "create": {
          const info = await client.session.create({ location, ...(value.selection.agent ? { agent: value.selection.agent } : {}), ...(value.selection.model ? { model: { providerID: value.selection.model.provider, id: value.selection.model.id, ...(value.selection.model.variant ? { variant: value.selection.model.variant } : {}) } } : {}) }, options)
          return { type: "created", session: session(info) }
        }
        case "prompt": {
          const result = await client.session.prompt({ sessionID: value.session, text: value.text, delivery: value.delivery === "Queue" ? "queue" : "steer" }, options)
          return { type: "admitted", item: inbox(result) }
        }
        case "interrupt": await client.session.interrupt({ sessionID: value.session }, options); break
        case "cancel_input": await client.session.inbox.cancel({ sessionID: value.session, inboxID: value.id }, options); break
        case "select_agent": {
          await client.session.switchAgent({ sessionID: value.session, agent: value.agent }, options)
          const info = await client.session.get({ sessionID: value.session }, options).catch(() => { throw new Error("Selection committed but projection unavailable") })
          return { type: "selection", selection: session(info).selection }
        }
        case "select_model": {
          await client.session.switchModel({ sessionID: value.session, model: { providerID: value.model.provider, id: value.model.id, ...(value.model.variant ? { variant: value.model.variant } : {}) } }, options)
          const info = await client.session.get({ sessionID: value.session }, options).catch(() => { throw new Error("Selection committed but projection unavailable") })
          return { type: "selection", selection: session(info).selection }
        }
        case "reply_permission": await client.permission.reply({ sessionID: value.session, requestID: value.id, decision: value.decision, ...(value.message ? { message: value.message } : {}) }, options); break
        case "reply_form": await client.session.form.reply({ sessionID: value.session, formID: value.id, answer: value.answer }, options); break
        case "cancel_form": await client.session.form.cancel({ sessionID: value.session, formID: value.id }, options); break
      }
      return { type: "done" }
    }
    async function dispatch(id: number, value: Request) {
      if (inFlight.has(id) || inFlight.size >= MAX_PENDING) { fail("Rust TUI bridge request overflow or duplicate identity"); return }
      inFlight.add(id)
      try {
        const reply = await request(value)
        send({ type: "response", id, reply, error: null, unknown: false })
      } catch (error) {
        if (stopped) return
        // Only declared, decoded API refusals are known outcomes. Transport,
        // abort and malformed responses never authorize a mutation resend.
        const known = knownRefusal(error)
        send({ type: "response", id, reply: null, error: known ? "OpenCode refused this request; the draft was retained" : null, unknown: !known })
        if (!known) fail("OpenCode request outcome is unknown")
      } finally { inFlight.delete(id) }
    }
    async function canonical(sessionID: string, messageID: string) {
      const captured = epoch
      const info = await client.session.message.get({ sessionID, messageID }, { signal: signal() })
      send({ type: "event", event: { type: "message", session: sessionID, epoch: captured, message: message(info) } })
    }
    function cached(sessionID: string, messageID: string) {
      const info = context.data.session.message.get(sessionID, messageID)
      if (info) send({ type: "event", event: { type: "message", session: sessionID, epoch, message: message(info) } })
    }
    async function event(value: OpenCodeEvent) {
      epoch += 1
      switch (value.type) {
        case "server.connected": throw new Error("OpenCode event stream reconnected")
        case "session.text.started":
        case "session.reasoning.started":
          send({ type: "event", event: { type: "part_started", session: value.data.sessionID, epoch, message: value.data.assistantMessageID, ordinal: value.data.ordinal, reasoning: value.type === "session.reasoning.started" } }); return
        case "session.text.delta":
        case "session.reasoning.delta":
          send({ type: "event", event: { type: "delta", session: value.data.sessionID, epoch, message: value.data.assistantMessageID, ordinal: value.data.ordinal, reasoning: value.type === "session.reasoning.delta", delta: value.data.delta } }); return
        case "session.text.ended":
        case "session.reasoning.ended":
          send({ type: "event", event: { type: "part", session: value.data.sessionID, epoch, message: value.data.assistantMessageID, ordinal: value.data.ordinal, part: { type: value.type === "session.text.ended" ? "text" : "reasoning", text: text(value.data.text) } } })
          // An already-active view has no delta watermark. Hydrate its ordered
          // public content rather than guessing where this ended part belongs.
          await canonical(value.data.sessionID, value.data.assistantMessageID); return
        case "session.step.started":
          send({ type: "event", event: { type: "started", session: value.data.sessionID, epoch, message: {
            id: value.data.assistantMessageID, role: "assistant", text: "", agent: value.data.agent, model: modelRef(value.data.model), parts: [], status: "running", truncated: false,
          } } })
          send({ type: "event", event: { type: "running", session: value.data.sessionID, running: true } }); return
        case "session.step.streamed":
        case "session.step.ended":
        case "session.step.failed":
          await canonical(value.data.sessionID, value.data.assistantMessageID)
          send({ type: "event", event: { type: "refresh", session: value.data.sessionID } }); return
        case "session.tool.input.started":
          send({ type: "event", event: { type: "tool_started", session: value.data.sessionID, epoch, message: value.data.assistantMessageID, id: value.data.id, name: value.data.name } }); return
        case "session.tool.input.delta":
        case "session.tool.input.ended":
        case "session.tool.called":
        case "session.tool.progress": cached(value.data.sessionID, value.data.assistantMessageID); return
        case "session.tool.success":
        case "session.tool.failed": await canonical(value.data.sessionID, value.data.assistantMessageID); return
        case "session.execution.started":
          send({ type: "event", event: { type: "running", session: value.data.sessionID, running: true } }); break
        case "session.execution.succeeded":
        case "session.execution.failed":
        case "session.execution.interrupted":
          send({ type: "event", event: { type: "running", session: value.data.sessionID, running: false } }); break
        case "form.created": {
          if (form(value.data.form).questions === null) {
            returnSession = value.data.form.sessionID || null
            fail("An unsupported OpenCode form remains pending; answer it in stock TUI")
            return
          }
          send({ type: "event", event: { type: "requests_changed" } }); return
        }
        case "form.replied":
        case "form.cancelled":
        case "permission.asked":
        case "permission.replied":
          send({ type: "event", event: { type: "requests_changed" } }); return
        case "session.agent.selected":
        case "session.model.selected":
        case "session.renamed":
        case "session.inbox.enqueued":
        case "session.inbox.delivered":
        case "session.inbox.cancelled": break
        default: return
      }
      send({ type: "event", event: { type: "refresh", session: value.data.sessionID } })
    }

    try {
      if (host.shuttingDown || context.renderer.isDestroyed) throw new Error("OpenCode is shutting down")
      const iterator = client.event.subscribe({ signal: abort.signal })[Symbol.asyncIterator]()
      const connected = await iterator.next()
      if (connected.done || connected.value.type !== "server.connected") throw new Error("OpenCode event stream unavailable")
      pump = (async () => {
        try {
          for (;;) {
            const item = await iterator.next()
            if (item.done) { if (!stopped) fail("OpenCode event stream ended"); return }
            await event(item.value)
          }
        } catch { if (!stopped) fail("OpenCode event stream failed or rebound") }
      })()
      const [metadata, forms] = await Promise.all([
        catalog(location),
        client.form.list({ location }, { signal: signal() }),
      ])
      if (stopped || host.shuttingDown || context.renderer.isDestroyed) throw new Error("OpenCode is shutting down")
      const unsupported = forms.data.find((item) => form(item).questions === null)
      if (unsupported) {
        returnSession = unsupported.sessionID || null
        fail("An unsupported OpenCode form remains pending; answer it in stock TUI")
        return async () => { await stop(); unregister(); context.renderer.off("destroy", destroyed) }
      }
      // Preserve the existing display-rate probe and clipboard transport, not backend credentials/config.
      const environment = Object.fromEntries(Object.entries(process.env).filter(([key]) => ["PATH", "TERM", "COLORTERM", "LANG", "LC_ALL", "DISPLAY", "XAUTHORITY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "SSH_CONNECTION", "SSH_TTY"].includes(key)))
      child = spawn("oc2", ["__opencode-tui"], { stdio: ["inherit", "inherit", "inherit", "pipe", "pipe"], env: environment })
      exited = new Promise<void>((resolve) => {
        child!.once("exit", (code, exitSignal) => {
          if (activated && !stopped && (code !== 0 || exitSignal)) context.ui.toast.show({ variant: "warning", message: "Rust TUI stopped unexpectedly; returning to stock without resending requests or interrupting backend work." })
          resolve()
        })
        child!.once("error", () => resolve())
      })
      const output = child.stdio[4]
      const destination = child.stdio[3]
      if (!(output instanceof Readable) || !(destination instanceof Writable)) throw new Error("Rust TUI bridge descriptors unavailable")
      input = destination
      let buffer = ""
      let hello = false
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("Rust TUI handshake timeout")), 3000)
        child!.once("error", () => { clearTimeout(timer); reject(new Error("oc2 was not found on PATH")) })
        child!.once("exit", () => { clearTimeout(timer); if (!hello) reject(new Error("Rust TUI exited before handshake")) })
        output.setEncoding("utf8")
        output.on("data", (data: string) => {
          try {
            buffer += data
            for (;;) {
              const end = buffer.indexOf("\n")
              if (end < 0) break
              const line = buffer.slice(0, end)
              buffer = buffer.slice(end + 1)
              if (Buffer.byteLength(line) + 1 > MAX_FRAME) throw new Error("Rust TUI frame limit exceeded")
              const frame = decode(line)
              if (!hello) {
                if (frame.type !== "hello" || frame.version !== VERSION) throw new Error("Incompatible oc2 bridge version")
                hello = true
                clearTimeout(timer)
                resolve()
                continue
              }
              if (frame.type === "request" && activated) { void dispatch(frame.id, frame.request).catch(() => fail("Rust TUI request handler failed")); continue }
              if (frame.type === "return" && activated) {
                returnSession = frame.session
                continue
              }
              throw new Error("Unexpected Rust TUI bridge frame")
            }
            if (Buffer.byteLength(buffer) > MAX_FRAME) throw new Error("Rust TUI frame limit exceeded")
          } catch {
            clearTimeout(timer)
            if (!hello) reject(new Error("Invalid Rust TUI handshake"))
            else fail("Invalid Rust TUI bridge frame")
          }
        })
      })
      if (stopped || host.shuttingDown || context.renderer.isDestroyed) throw new Error("OpenCode is shutting down")
      suspension = context.renderer.suspend()
      await suspension
      suspended = true
      if (stopped || host.shuttingDown || context.renderer.isDestroyed) throw new Error("OpenCode is shutting down")
      const route = context.ui.router.current()
      activated = true
      send({ type: "activate", version: VERSION, init: { location: location.directory, session: route.type === "session" && route.sessionID !== "dummy" ? route.sessionID : null, catalog: metadata } })
      for (const frame of queued) send(frame)
      queued.length = 0
      void exited.then(() => stop())
      return async () => {
        await stop()
        unregister()
        context.renderer.off("destroy", destroyed)
      }
    } catch {
      await stop()
      unregister()
      context.renderer.off("destroy", destroyed)
      context.ui.toast.show({ variant: "warning", message: "Rust TUI could not start; stock TUI remains active. Check oc2 on PATH and the hook-enabled OpenCode build." })
    }
  },
})
