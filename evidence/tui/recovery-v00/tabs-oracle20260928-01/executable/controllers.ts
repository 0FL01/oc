import { marqueeOverflows, marqueeCycleWidth } from './marquee'
import { createSignal, onCleanup, createAnimatable, tween, setTimeout, setInterval, clearTimeout, clearInterval } from './host'
const MARQUEE_DELAY = 600, MARQUEE_INTERVAL = 80
export function createMarquee(animations: () => boolean) {
  const [offset, setOffset] = createSignal(0)
  const [active, setActive] = createSignal<string>()
  const leading = createAnimatable({ opacity: 0 }, { enabled: animations, transition: tween({ duration: 0.25 }) })
  let delay: ReturnType<typeof setTimeout> | undefined
  let interval: ReturnType<typeof setInterval> | undefined
  let cycleWidth = 0

  const clear = () => {
    if (delay) clearTimeout(delay)
    if (interval) clearInterval(interval)
    delay = undefined
    interval = undefined
  }
  const scroll = () => {
    interval = setInterval(
      () =>
        setOffset((value) => {
          if (value + 1 < cycleWidth) return value + 1
          clear()
          leading.animate({ opacity: 0 })
          return 0
        }),
      MARQUEE_INTERVAL,
    )
  }
  const enter = (sessionID: string, title: string, width: number) => {
    if (!marqueeOverflows(title, width)) {
      reset()
      return
    }
    if (active() === sessionID) return
    clear()
    cycleWidth = marqueeCycleWidth(title)
    setActive(sessionID)
    setOffset(0)
    leading.jump({ opacity: 0 })
    delay = setTimeout(() => {
      setOffset(1)
      leading.animate({ opacity: 1 })
      scroll()
    }, MARQUEE_DELAY)
  }
  const leave = (sessionID: string) => {
    if (active() !== sessionID) return
    reset()
  }
  const reset = () => {
    clear()
    setActive(undefined)
    setOffset(0)
    leading.jump({ opacity: 0 })
  }
  onCleanup(clear)

  return { offset, active, enter, leave, reset, leading: () => leading.value().opacity }
}

export function createTabMarquee(animations: () => boolean) {
  const [hovered, setHovered] = createSignal<string>()
  const marquee = createMarquee(animations)
  let hoverClear: ReturnType<typeof setTimeout> | undefined

  const enter = (sessionID: string, title: string, width: number) => {
    if (hoverClear) clearTimeout(hoverClear)
    setHovered(sessionID)
    marquee.enter(sessionID, title, width)
  }
  const leave = (sessionID: string) => {
    if (hoverClear) clearTimeout(hoverClear)
    hoverClear = setTimeout(() => {
      if (hovered() !== sessionID) return
      setHovered(undefined)
      marquee.leave(sessionID)
    })
  }
  const leaveHovered = () => {
    const sessionID = hovered()
    if (sessionID) leave(sessionID)
  }
  const reset = () => {
    if (hoverClear) clearTimeout(hoverClear)
    hoverClear = undefined
    setHovered(undefined)
    marquee.reset()
  }
  onCleanup(() => {
    if (hoverClear) clearTimeout(hoverClear)
  })

  return { ...marquee, hovered, enter, leave, leaveHovered, reset }
}
