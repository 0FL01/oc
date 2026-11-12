import type { FormInfo, ModelInfo, ModelRef, PermissionRequest, SessionInboxInfo, SessionInfo, SessionMessageInfo } from "@opencode/client"

const TEXT_LIMIT = 32 * 1024

export function text(value: string) {
  const clipped = value.slice(0, TEXT_LIMIT)
  // Do not split a UTF-16 surrogate pair at a bounded preview boundary.
  return /[\uD800-\uDBFF]$/.test(clipped) ? clipped.slice(0, -1) : clipped
}

export function modelRef(model?: ModelRef) {
  return model ? { provider: model.providerID, id: model.id, variant: model.variant ?? null } : null
}

export function session(info: SessionInfo, running = false) {
  return {
    id: info.id, parent: info.parentID ?? null, title: info.title ?? null,
    location: info.location.directory, selection: { agent: info.agent ?? null, model: modelRef(info.model) },
    created: new Date(info.time.created).toISOString(), updated: new Date(info.time.updated).toISOString(), running,
  }
}

export function model(info: ModelInfo) {
  const price = info.cost.find((cost) => !cost.tier)
  return {
    provider: info.providerID,
    entry: {
      id: info.id, display_name: info.name, provider_name: info.providerID,
      price: price ? { input: String(price.input), output: String(price.output) } : null,
      variants: info.variants.map((variant) => ({ name: variant.id, disabled: false, reasoning_effort: null })),
      variants_known: true,
      capabilities: { input: info.capabilities.input, output: info.capabilities.output, attachment: null, reasoning: null, tools: info.capabilities.tools, partial: { image_input: null, pdf_input: null, reasoning: null } },
      context: info.limit.context, context_known: true, output: info.limit.output, output_known: true,
      released: info.time.released, deprecated: info.status === "deprecated", input_free: price?.input === 0,
    },
  }
}

export function message(info: SessionMessageInfo) {
  if (info.type !== "assistant") {
    const value = info.type === "user" ? info.text : `OpenCode ${info.type} message — use stock TUI for details`
    return { id: info.id, role: info.type === "user" ? "user" : "system", text: text(value), agent: null, model: null, parts: [], status: "completed", truncated: value.length > TEXT_LIMIT }
  }
  const parts = info.content.map((part) => {
    if (part.type === "text" || part.type === "reasoning") return { type: part.type, text: text(part.text) }
    const state = part.state
    const input = state.status === "streaming" ? state.input : JSON.stringify(state.input)
    const output = "content" in state
      ? state.content?.map((item) => item.type === "text" ? item.text : "[file attachment — view in stock TUI]").join("\n") ?? ""
      : ""
    return { type: "tool", id: part.id, name: part.name, input: text(input), output: text(state.status === "error" ? `${state.error.message}\n${output}` : output), status: state.status }
  })
  const truncated = info.content.some((part) => part.type === "tool"
    ? JSON.stringify(part.state).length > TEXT_LIMIT
    : part.text.length > TEXT_LIMIT)
  return {
    id: info.id, role: "assistant", text: "", agent: info.agent, model: modelRef(info.model), parts,
    status: info.error ? "error" : !info.time.completed || info.content.some((part) => part.type === "tool" && (part.state.status === "streaming" || part.state.status === "running")) ? "running" : "completed",
    truncated,
  }
}

export function inbox(item: SessionInboxInfo) {
  if (item.type !== "user") return undefined
  return { id: item.id, delivery: item.delivery === "queue" ? "Queue" : "Steer", text: text(item.payload.text), images: item.payload.files?.length ?? 0, error: null }
}

export function permission(info: PermissionRequest) {
  return { id: info.id, session: info.sessionID, action: info.action, resources: info.resources, save: info.save ?? [] }
}

export function form(info: FormInfo) {
  const compatible = info.metadata?.kind === "question" && info.fields.length > 0 && info.fields.length <= 16 && info.fields.every((field) =>
    (field.type === "string" || field.type === "multiselect") && !field.hidden && !field.when?.length &&
    !("format" in field) && !("pattern" in field) && !("minLength" in field) && !("maxLength" in field) &&
    !("minItems" in field) && !("maxItems" in field) && field.custom === true &&
    field.options !== undefined && field.options.length <= 16 && field.options.every((option) => option.value === option.label) &&
    new Set(field.options.map((option) => option.label)).size === field.options.length)
  const projected = compatible ? info.fields.flatMap((field) => {
    if (field.type !== "string" && field.type !== "multiselect") return []
    return [{ key: field.key, label: field.description ?? field.title ?? field.key, multiple: field.type === "multiselect", custom: field.custom === true,
      options: field.options?.map((option) => ({ value: option.value, label: option.label, description: option.description ?? "" })) ?? [] }]
  }) : null
  // Match the existing Rust question widget's byte/field limits. An unsupported
  // form stays pending and returns to stock instead of being silently truncated.
  const questions = projected && projected.every((question) =>
    Buffer.byteLength(question.label) <= 8192 && question.options.every((option) =>
      Buffer.byteLength(option.label) <= 512 && Buffer.byteLength(option.description) <= 8192)) &&
    Buffer.byteLength(JSON.stringify({ questions: projected.map((question) => ({
      question: question.label, header: info.title, multiple: question.multiple,
      options: question.options.map((option) => ({ label: option.label, description: option.description })),
    })) })) <= 128 * 1024 ? projected : null
  return { id: info.id, session: info.sessionID, title: info.title, questions }
}
