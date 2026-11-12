import { Plugin } from "@opencode/plugin/tui"
import { spawn } from "node:child_process"
import { appendFileSync } from "node:fs"

// Qualification fixture only: never installed in an auto-discovery directory.
export default Plugin.define({
  id: "oc2-rust-tui-lifecycle-fixture",
  async setup(context) {
    const record = (event: string, data = {}) =>
      appendFileSync(process.env.OC2_LIFECYCLE_LOG!, JSON.stringify({ event, ...data }) + "\n")
    const [lifecycle, updateLifecycle] = context.storage.memory("lifecycle", { initial: { shutdown: false } })
    const host = context.lifecycle
    if (process.env.OC2_LIFECYCLE_HOST === "candidate" && !host) throw new Error("awaited lifecycle hook unavailable")
    const closing = () => lifecycle.shutdown || host?.shuttingDown
    if (closing() || context.renderer.isDestroyed) {
      record("setup.skipped")
      return
    }
    const shutdown = () => updateLifecycle((state) => { state.shutdown = true })
    const signals = ["SIGTERM", "SIGINT", "SIGHUP"] as const
    const location = context.location ?? context.data.location.default()
    const response = await context.client.agent.list({ location })
    if (closing() || context.renderer.isDestroyed) throw new Error("host is shutting down")
    if (process.env.OC2_LIFECYCLE_MODE === "seed") {
      const session = await context.client.session.create({ location })
      record("seed", { id: session.id })
    }
    record("setup", {
      version: context.app.version,
      location,
      clientLocation: response.location,
      route: context.ui.router.current(),
      awaitedLifecycle: Boolean(host),
    })
    const executable = process.env.OC2_LIFECYCLE_MODE === "missing" ? "/opt/missing-child"
      : process.env.OC2_LIFECYCLE_CHILD!
    if (closing() || context.renderer.isDestroyed) throw new Error("host is shutting down")
    for (const signal of signals) process.on(signal, shutdown)
    const child = spawn(executable, [], {
      stdio: ["inherit", "inherit", "inherit", "pipe", "pipe"],
    })
    child.once("spawn", () => record("spawn", { pid: child.pid }))
    const exited = new Promise<void>((resolve) => {
      child.once("exit", (code, signal) => { record("child.exit", { code, signal }); resolve() })
      child.once("error", (error) => { record("child.error", { message: error.message }); resolve() })
    })
    let suspended = false
    let suspension: Promise<void> | undefined
    let stopped: Promise<void> | undefined
    const resume = async () => {
      if (!suspended || context.renderer.isDestroyed || context.renderer.controlState !== "explicit_suspended") return
      suspended = false
      await context.renderer.resume()
      record("resume")
    }
    const stop = () => stopped ??= (async () => {
      record("cleanup.begin", { destroyed: context.renderer.isDestroyed })
      child.kill("SIGTERM")
      const timer = setTimeout(() => child.kill("SIGKILL"), 1000)
      await exited
      clearTimeout(timer)
      child.stdio[3]?.destroy()
      child.stdio[4]?.destroy()
      await suspension?.catch(() => {})
      if (!closing()) await resume()
      record("cleanup.end", { destroyed: context.renderer.isDestroyed })
    })()
    // Registered before handoff, independently of setup's returned cleanup.
    // Restore only our completed suspension, through the host renderer after stop/join.
    const unregister = host?.onShutdown(async () => {
      shutdown()
      record("host.shutdown")
      await stop()
      await resume()
    })
    // Earliest public renderer destruction notification. This is not awaited
    // by OpenTUI; the checkpoint tests whether it is sufficient nonetheless.
    const destroyed = () => { record("renderer.destroy"); void stop() }
    context.renderer.once("destroy", destroyed)
    try {
      if (!child.stdio[3] || !child.stdio[4]) throw new Error("child IPC descriptors unavailable")
      await new Promise<void>((resolve, reject) => {
        let frame = ""
        const timer = setTimeout(() => reject(new Error("handshake timeout")), 3000)
        child.stdio[4]!.on("data", (data: Buffer) => {
          frame += data.toString()
          if (frame.length > 4096) { clearTimeout(timer); reject(new Error("frame limit")); return }
          if (!frame.includes("\n")) return
          clearTimeout(timer)
          try {
            const hello = JSON.parse(frame)
            if (hello.type !== "hello" || hello.version !== 1) throw new Error("incompatible child")
            record("hello", { pid: hello.pid })
            resolve()
          } catch (error) { reject(error) }
        })
        child.once("error", reject)
        child.once("exit", () => { clearTimeout(timer); reject(new Error("child exited before handshake")) })
      })
      if (closing() || context.renderer.isDestroyed) throw new Error("host is shutting down")
      suspension = context.renderer.suspend()
      await suspension
      suspended = true
      record("suspend", { destroyed: context.renderer.isDestroyed })
      if (closing() || context.renderer.isDestroyed) throw new Error("host is shutting down")
      child.stdio[3]!.write(JSON.stringify({ type: "activate", version: 1 }) + "\n")
      record("activate")
      void exited.then(stop)
      return async () => {
        await stop()
        unregister?.()
        if (!host?.shuttingDown) context.renderer.off("destroy", destroyed)
        for (const signal of signals) process.off(signal, shutdown)
      }
    } catch (error) {
      record("setup.error", { message: String(error) })
      await stop()
      unregister?.()
      context.renderer.off("destroy", destroyed)
      for (const signal of signals) process.off(signal, shutdown)
      throw error
    }
  },
})
