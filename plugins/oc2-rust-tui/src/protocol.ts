import { Schema } from "effect"

export const VERSION = 1
export const MAX_FRAME = 4 * 1024 * 1024
export const MAX_PENDING = 64
// The captured client decodes declared API errors to these formal tags. Its
// package is not a plugin runtime module; use the same tags through host Effect.
export const knownRefusal = Schema.is(Schema.Struct({
  _tag: Schema.Literals(["InvalidRequestError", "UnauthorizedError", "SessionNotFoundError", "ConflictError"]),
}))
const Model = Schema.Struct({ provider: Schema.String, id: Schema.String, variant: Schema.NullOr(Schema.String) })
const Selection = Schema.Struct({ agent: Schema.NullOr(Schema.String), model: Schema.NullOr(Model) })
const Session = { session: Schema.String }
const RequestID = { ...Session, id: Schema.String }

const Request = Schema.Union([
  Schema.Struct({ type: Schema.Literal("open"), ...Session }),
  Schema.Struct({ type: Schema.Literal("sessions"), search: Schema.String }),
  Schema.Struct({ type: Schema.Literal("history"), ...Session, cursor: Schema.NullOr(Schema.String) }),
  Schema.Struct({ type: Schema.Literal("create"), selection: Selection }),
  Schema.Struct({ type: Schema.Literal("prompt"), ...Session, text: Schema.String, delivery: Schema.Literals(["Steer", "Queue"]) }),
  Schema.Struct({ type: Schema.Literal("interrupt"), ...Session }),
  Schema.Struct({ type: Schema.Literal("cancel_input"), ...RequestID }),
  Schema.Struct({ type: Schema.Literal("select_agent"), ...Session, agent: Schema.String }),
  Schema.Struct({ type: Schema.Literal("select_model"), ...Session, model: Model }),
  Schema.Struct({ type: Schema.Literal("reply_permission"), ...RequestID, decision: Schema.Literals(["once", "always", "reject"]), message: Schema.NullOr(Schema.String) }),
  Schema.Struct({ type: Schema.Literal("reply_form"), ...RequestID, answer: Schema.Record(Schema.String, Schema.Union([Schema.String, Schema.Array(Schema.String)])) }),
  Schema.Struct({ type: Schema.Literal("cancel_form"), ...RequestID }),
])

const Frame = Schema.Union([
  Schema.Struct({ type: Schema.Literal("hello"), version: Schema.Number }),
  Schema.Struct({ type: Schema.Literal("request"), id: Schema.Number, request: Request }),
  Schema.Struct({ type: Schema.Literal("return"), session: Schema.NullOr(Schema.String) }),
])

export const decode = Schema.decodeUnknownSync(Schema.fromJsonString(Frame))
export type Request = typeof Request.Type
