let now = 0, serial = 1
const jobs = new Map<number, { at: number; callback: () => void; repeat?: number }>()
const cleanups: (() => void)[] = []
export function createSignal<T>(initial?: T) { let value = initial; return [() => value, (next: T | ((value: T) => T)) => value = typeof next === 'function' ? (next as (value: T) => T)(value as T) : next] as const }
export function onCleanup(fn: () => void) { cleanups.push(fn) }
export const tween = (options: unknown) => options
export function createAnimatable<T>(initial: T, options: { enabled: () => boolean }) { if (options.enabled()) throw Error('Oracle only substitutes off-mode animation host'); let value = initial; return { value: () => value, jump: (next: T) => value = next, animate: (next: T) => value = next } }
export function setTimeout(callback: () => void, ms = 0) { const id = serial++; jobs.set(id, { at: now + ms, callback }); return id }
export function setInterval(callback: () => void, ms: number) { const id = serial++; jobs.set(id, { at: now + ms, callback, repeat: ms }); return id }
export const clearTimeout = (id: number) => jobs.delete(id), clearInterval = clearTimeout
export function advance(ms: number) { const end = now + ms; while (true) { const next = [...jobs].filter(([, job]) => job.at <= end).sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0]; if (!next) break; now = next[1].at; if (next[1].repeat) next[1].at += next[1].repeat; else jobs.delete(next[0]); next[1].callback() } now = end }
export function pending() { return jobs.size }
export function dispose() { for (const fn of cleanups.splice(0)) fn(); jobs.clear(); now = 0 }
