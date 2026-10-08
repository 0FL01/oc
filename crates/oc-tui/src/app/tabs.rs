//! Presentation and animation clocks for the retained visible deck.

use super::*;

/// Bounded, application-supplied presentation for one retained real tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabPresentation {
    pub session: SessionId,
    pub title: Option<String>,
    /// Source project-name fallback from the tab's actual canonical Location.
    pub detail: Option<String>,
    pub home: bool,
    pub busy: bool,
    pub attention: Option<TabAttention>,
    /// U47 `sessionTabComplete`: actual unread activity while idle, not merely
    /// a successful turn receipt. The current owner has no unread projection.
    pub complete: bool,
    /// U46 user enqueue pulse for a retained, noncurrent root. Zero until the
    /// owner supplies that fact; local submission attempts are not this signal.
    pub prompt_pulse: u64,
}

impl TabPresentation {
    pub fn new(session: SessionId) -> Self {
        Self {
            session,
            title: None,
            detail: None,
            home: false,
            busy: false,
            attention: None,
            complete: false,
            prompt_pulse: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabAttention {
    Permission,
    Question,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TabIdentity {
    Session(SessionId),
    Home,
}

pub(crate) const TAB_SPINNER_FRAMES: [&str; 10] =
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TAB_STEP: Duration = Duration::from_millis(80);
const TAB_MARQUEE_DELAY: Duration = Duration::from_millis(600);
const TAB_FADE: Duration = Duration::from_millis(250);
const TAB_FADE_FRAME: Duration = Duration::from_nanos(16_666_667);
const TAB_RUN_ATTACK: Duration = Duration::from_millis(450);
const TAB_RUN_RELEASE: Duration = Duration::from_millis(500);
const TAB_GLOW_ATTACK: Duration = Duration::from_millis(600);
const TAB_GLOW_RELEASE: Duration = Duration::from_millis(900);
const TAB_COMPLETION: Duration = Duration::from_millis(1200);
const TAB_FLASH: Duration = Duration::from_millis(800);

fn tab_attack_decay(progress: f32, attack: f32, peak: f32, rest: f32) -> f32 {
    if progress < attack {
        peak * tab_smootherstep((progress / attack).clamp(0.0, 1.0))
    } else {
        peak - (peak - rest)
            * tab_smootherstep(((progress - attack) / (1.0 - attack)).clamp(0.0, 1.0))
    }
}

pub(crate) fn tab_glow_intensity(index: f32, tail: f32) -> f32 {
    tab_smootherstep((1.0 - (index - 1.0).max(0.0) / tail).clamp(0.0, 1.0))
}

/// U48's two gated voices, kept inside the existing visible-deck clock state.
/// Sustain is motionless; release captures the current (possibly attacking)
/// amplitude, so a short run/permission wait does not flash at full strength.
#[derive(Clone, Copy)]
enum TabGate {
    Idle,
    Attack(Duration),
    Sustain,
    Release(Duration, f32),
}

impl TabGate {
    fn level(self, glow: bool) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Sustain => 1.0,
            Self::Attack(clock) if glow => tab_attack_decay(
                clock.as_secs_f32() / TAB_GLOW_ATTACK.as_secs_f32(),
                0.3,
                1.5,
                1.0,
            ),
            Self::Attack(clock) => {
                tab_smootherstep(clock.as_secs_f32() / TAB_RUN_ATTACK.as_secs_f32())
            }
            Self::Release(clock, scale) => {
                // The resting glow drains in 200 ms while its spatial swell
                // continues for the whole 900 ms release.
                let duration = if glow {
                    0.2
                } else {
                    TAB_RUN_RELEASE.as_secs_f32()
                };
                scale * (1.0 - tab_smootherstep((clock.as_secs_f32() / duration).min(1.0)))
            }
        }
    }

    fn release(&mut self, glow: bool) {
        if !matches!(self, Self::Idle) {
            *self = Self::Release(Duration::ZERO, self.level(glow));
        }
    }

    fn advance(&mut self, delta: Duration, glow: bool) {
        match self {
            Self::Attack(clock) => {
                *clock += delta;
                if *clock
                    >= if glow {
                        TAB_GLOW_ATTACK
                    } else {
                        TAB_RUN_ATTACK
                    }
                {
                    *self = Self::Sustain;
                }
            }
            Self::Release(clock, _) => {
                *clock += delta;
                if *clock
                    >= if glow {
                        TAB_GLOW_RELEASE
                    } else {
                        TAB_RUN_RELEASE
                    }
                {
                    *self = Self::Idle;
                }
            }
            _ => {}
        }
    }

    fn remaining(self, glow: bool) -> Option<Duration> {
        match self {
            Self::Attack(clock) => Some(
                (if glow {
                    TAB_GLOW_ATTACK
                } else {
                    TAB_RUN_ATTACK
                }) - clock,
            ),
            Self::Release(clock, _) => Some(
                (if glow {
                    TAB_GLOW_RELEASE
                } else {
                    TAB_RUN_RELEASE
                }) - clock,
            ),
            _ => None,
        }
    }
}

/// U47 component-local smoothstep tint, not an independently running timer.
struct TabTint {
    level: f32,
    from: f32,
    target: f32,
    clock: Option<Duration>,
    duration: Duration,
}

impl TabTint {
    fn new(level: f32, milliseconds: u64) -> Self {
        Self {
            level,
            from: level,
            target: level,
            clock: None,
            duration: Duration::from_millis(milliseconds),
        }
    }

    fn target(&mut self, target: f32, enabled: bool) {
        if target == self.target {
            if !enabled {
                self.settle();
            }
            return;
        }
        self.from = self.level;
        self.target = target;
        self.clock = (enabled && self.level != target).then_some(Duration::ZERO);
        if !enabled {
            self.level = target;
        }
    }

    fn settle(&mut self) {
        self.level = self.target;
        self.clock = None;
    }

    fn ignite(&mut self, enabled: bool) {
        self.level = if enabled { 0.85 } else { 0.0 };
        self.from = self.level;
        self.target = 0.0;
        self.clock = enabled.then_some(Duration::ZERO);
    }

    fn advance(&mut self, delta: Duration) {
        let Some(clock) = &mut self.clock else { return };
        *clock += delta;
        let p = (clock.as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
        self.level = self.from + (self.target - self.from) * p * p * (3.0 - 2.0 * p);
        if *clock >= self.duration {
            self.settle();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TabPulseFrame {
    pub animations: bool,
    pub complete: bool,
    pub glows: bool,
    pub running: f32,
    pub sweep_clock: f32,
    pub completion: f32,
    pub flash: f32,
    pub glow: f32,
    pub release: Option<f32>,
    pub swell: f32,
    pub whitecap: f32,
    pub dim: f32,
    pub title_glow: f32,
    pub number_glow: f32,
}

impl TabPulseFrame {
    pub(crate) fn sweep(self, index: f32, width: u16) -> f32 {
        if self.running == 0.0 {
            return 0.0;
        }
        let coast = |p: f32| {
            if p < 0.2 {
                p * p / 0.32
            } else if p > 0.8 {
                1.0 - (1.0 - p) * (1.0 - p) / 0.32
            } else {
                (p - 0.1) / 0.8
            }
        };
        let cycles = self.sweep_clock / 2.8;
        let front = |p: f32| -4.0 + coast(p) * (f32::from(width) + 21.0);
        let intensity = |front: f32| {
            let distance = front - index;
            tab_smootherstep(
                if distance < 0.0 {
                    1.0 + distance / 4.0
                } else {
                    1.0 - distance / 18.0
                }
                .clamp(0.0, 1.0),
            )
        };
        intensity(front(cycles.fract())).max(intensity(front(if cycles < 0.5 {
            0.0
        } else {
            (cycles + 0.5).fract()
        }))) * self.running
    }

    pub(crate) fn glow_at(self, index: f32, width: u16, maximum_tail: f32) -> f32 {
        let tail = maximum_tail.min(f32::from(width.saturating_sub(2).max(1)));
        let resting = tab_glow_intensity(index, tail) * self.glow;
        let diffusing = self.release.map_or(0.0, |p| {
            tab_glow_intensity(index, tail + tab_smootherstep(p) * f32::from(width)) * self.swell
        });
        0.16 * resting.max(diffusing)
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct TabPulseTarget {
    animations: bool,
    runs: bool,
    complete: bool,
    glows: bool,
    prompt: u64,
    dimmed: bool,
    vertical: bool,
    compact: bool,
    numbers: bool,
}

impl TabPulseTarget {
    fn enabled(self) -> bool {
        self.animations && !self.compact
    }
}

struct TabPulse {
    target: TabPulseTarget,
    completion_pending: bool,
    run: TabGate,
    attention: TabGate,
    sweep_clock: Duration,
    completion: Option<Duration>,
    flash: Option<Duration>,
    flash_scale: f32,
    whitecap: TabTint,
    dim: TabTint,
    title: TabTint,
    number: TabTint,
    last: Instant,
}

impl TabPulse {
    fn new(target: TabPulseTarget, now: Instant) -> Self {
        let mut result = Self {
            target,
            completion_pending: false,
            run: if target.enabled() && target.runs {
                TabGate::Attack(Duration::ZERO)
            } else {
                TabGate::Idle
            },
            // Source mount keeps an existing glow at sustain, without ignition.
            attention: if target.glows {
                TabGate::Sustain
            } else {
                TabGate::Idle
            },
            sweep_clock: Duration::ZERO,
            completion: None,
            flash: None,
            flash_scale: 1.0,
            whitecap: TabTint::new(0.0, 700),
            dim: TabTint::new(if target.dimmed { 0.7 } else { 1.0 }, 200),
            title: TabTint::new(0.0, 400),
            number: TabTint::new(0.0, 400),
            last: now,
        };
        result.tint_targets();
        result
    }

    fn sync(&mut self, target: TabPulseTarget, now: Instant) {
        if self.target == target {
            return;
        }
        self.tick(now);
        self.last = now;
        let previous = self.target;
        let enabled = target.enabled();
        self.target = target;
        if previous.enabled() != enabled {
            if !enabled {
                self.run = TabGate::Idle;
                self.attention = if target.glows {
                    TabGate::Sustain
                } else {
                    TabGate::Idle
                };
                self.completion = None;
                self.flash = None;
                self.completion_pending = false;
                self.whitecap.settle();
                self.dim.settle();
                self.title.settle();
                self.number.settle();
            } else if previous.runs {
                // Re-enable attacks from the retained sweep position.
                self.run = TabGate::Attack(Duration::ZERO);
            }
        }
        if previous.runs != target.runs {
            self.whitecap.ignite(target.animations);
            if enabled {
                if target.runs {
                    self.sweep_clock = Duration::ZERO;
                    self.run = TabGate::Attack(Duration::ZERO);
                    self.completion = None;
                    self.completion_pending = false;
                } else {
                    self.run.release(false);
                    self.completion_pending = true;
                }
                // start(), not restart(): fast toggles retain the edge flash.
                if self.flash.is_none() {
                    self.flash = Some(Duration::ZERO);
                    self.flash_scale = 1.0;
                }
            }
        }
        if previous.prompt != target.prompt {
            self.whitecap.ignite(target.animations);
            if enabled {
                self.flash = Some(Duration::ZERO);
                self.flash_scale = 2.0;
            }
        }
        if previous.complete != target.complete {
            if !target.complete {
                self.completion = None;
                self.completion_pending = false;
            } else if self.completion_pending {
                self.completion_pending = false;
                if enabled {
                    self.completion.get_or_insert(Duration::ZERO);
                }
            }
        }
        if previous.glows != target.glows {
            if !enabled {
                self.attention = if target.glows {
                    TabGate::Sustain
                } else {
                    TabGate::Idle
                };
            } else if target.glows {
                self.attention = TabGate::Attack(Duration::ZERO);
            } else {
                self.attention.release(true);
            }
        }
        if !target.animations {
            self.whitecap.settle();
        }
        self.tint_targets();
    }

    fn tint_targets(&mut self) {
        let target = self.target;
        self.dim
            .target(if target.dimmed { 0.7 } else { 1.0 }, target.enabled());
        self.title.target(
            if target.vertical && !target.compact && target.glows {
                1.0
            } else {
                0.0
            },
            target.animations,
        );
        self.number.target(
            if target.vertical && (target.glows || target.complete) {
                1.0
            } else {
                0.0
            },
            target.animations,
        );
    }

    fn frame(&self) -> TabPulseFrame {
        let release = match self.attention {
            TabGate::Release(clock, _) => Some(clock.as_secs_f32() / 0.9),
            _ => None,
        };
        let scale = match self.attention {
            TabGate::Release(_, scale) => scale.max(1.0),
            _ => 0.0,
        };
        TabPulseFrame {
            animations: self.target.animations,
            complete: self.target.complete,
            glows: self.target.glows,
            running: self.run.level(false),
            sweep_clock: self.sweep_clock.as_secs_f32(),
            completion: self.completion.map_or(0.0, |clock| {
                0.18 * tab_attack_decay(clock.as_secs_f32() / 1.2, 0.12, 1.0, 0.0)
            }),
            flash: self.flash.map_or(0.0, |clock| {
                0.1 * self.flash_scale * tab_attack_decay(clock.as_secs_f32() / 0.8, 0.1, 1.0, 0.0)
            }),
            glow: self.attention.level(true),
            release,
            swell: release.map_or(0.0, |p| tab_attack_decay(p, 0.12, 1.25, 0.0) * scale),
            whitecap: self.whitecap.level,
            dim: self.dim.level,
            title_glow: self.title.level,
            number_glow: self.number.level,
        }
    }

    fn deadline(&self) -> Option<Instant> {
        if !self.target.animations {
            return None;
        }
        let remaining = [
            self.run.remaining(false),
            self.attention.remaining(true),
            self.completion.map(|clock| TAB_COMPLETION - clock),
            self.flash.map(|clock| TAB_FLASH - clock),
            self.whitecap
                .clock
                .filter(|_| !self.target.compact || self.target.numbers)
                .map(|clock| self.whitecap.duration - clock),
            self.dim.clock.map(|clock| self.dim.duration - clock),
            self.title.clock.map(|clock| self.title.duration - clock),
            self.number
                .clock
                .filter(|_| !self.target.compact || self.target.numbers)
                .map(|clock| self.number.duration - clock),
        ]
        .into_iter()
        .flatten()
        .min();
        ((self.target.enabled() && self.target.runs) || remaining.is_some())
            .then(|| self.last + remaining.unwrap_or(TAB_FADE_FRAME).min(TAB_FADE_FRAME))
    }

    fn tick(&mut self, now: Instant) -> bool {
        let before = self.frame();
        let delta = now.saturating_duration_since(self.last);
        self.last = self.last.max(now);
        if !self.target.animations {
            return false;
        }
        if !matches!(self.run, TabGate::Idle) {
            self.sweep_clock += delta;
        }
        self.run.advance(delta, false);
        self.attention.advance(delta, true);
        for (clock, duration) in [
            (&mut self.completion, TAB_COMPLETION),
            (&mut self.flash, TAB_FLASH),
        ] {
            if let Some(elapsed) = clock {
                *elapsed += delta;
                if *elapsed >= duration {
                    *clock = None;
                }
            }
        }
        self.whitecap.advance(delta);
        self.dim.advance(delta);
        self.title.advance(delta);
        self.number.advance(delta);
        if self.completion_pending {
            if self.target.complete {
                self.completion_pending = false;
                self.completion.get_or_insert(Duration::ZERO);
            } else if !matches!(self.run, TabGate::Release(..)) {
                self.completion_pending = false;
            }
        }
        before != self.frame()
    }
}

struct TabMotion {
    id: TabIdentity,
    at: Option<Instant>,
    frame: usize,
    pulse: TabPulse,
}

/// U56: one hovered identity retains a completed cycle until leave. The cycle
/// width is captured at enter; title metadata remains live in the renderer.
struct TabMarquee {
    id: TabIdentity,
    first: Instant,
    cycle: usize,
    offset: usize,
    done: bool,
    leading: f32,
    fade_at: Option<Instant>,
}

impl TabMarquee {
    fn end(&self) -> Instant {
        self.first + TAB_STEP * self.cycle.saturating_sub(1) as u32
    }

    fn deadline(&self) -> Option<Instant> {
        let step = (!self.done).then(|| self.first + TAB_STEP * self.offset as u32);
        let fade = self.fade_at.map(|at| {
            (at + TAB_FADE_FRAME).min(if self.done { self.end() } else { self.first } + TAB_FADE)
        });
        [step, fade].into_iter().flatten().min()
    }

    fn tick(&mut self, now: Instant, animations: bool) -> bool {
        if now < self.first {
            return false;
        }
        let previous = (self.offset, self.leading);
        let end = self.end();
        self.done = now >= end;
        self.offset = if self.done {
            0
        } else {
            1 + (now.duration_since(self.first).as_millis() / 80) as usize
        };
        let elapsed = now.saturating_duration_since(if self.done { end } else { self.first });
        let progress = (elapsed.as_secs_f32() / TAB_FADE.as_secs_f32()).min(1.0);
        // ui/animation.ts tween defaults to smoothstep, not smootherstep.
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.leading = if !animations {
            if self.done { 0.0 } else { 1.0 }
        } else if self.done {
            1.0 - eased
        } else {
            eased
        };
        self.fade_at = (animations && progress < 1.0).then_some(now);
        previous != (self.offset, self.leading)
    }
}

#[derive(Default)]
pub(super) struct TabView {
    area: Option<Rect>,
    vertical: bool,
    compact: bool,
    pub(super) hovered: Option<TabIdentity>,
    pub(super) leave: Option<Instant>,
    marquee: Option<TabMarquee>,
    motions: Vec<TabMotion>,
}

impl TabView {
    pub(super) fn reset_hover(&mut self) {
        self.hovered = None;
        self.leave = None;
        self.marquee = None;
    }
}

#[cfg(test)]
#[path = "tests/tabs.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TabPress {
    Add,
    Tab(usize),
    Close(usize),
}

/// Geometry and pointer from a genuine mouse release on a painted close cell.
/// Kept across the binary's replacement of the active view only on success.
pub struct TabCloseSnapshot {
    area: Rect,
    strip: crate::layout::HorizontalTabStrip,
    tabs: Vec<TabPresentation>,
    home: bool,
    closed: usize,
    pointer: (u16, u16),
}

pub(crate) struct TabCloseHold {
    pub(crate) area: Rect,
    pub(crate) strip: crate::layout::HorizontalTabStrip,
    pub(crate) until: Instant,
}

impl TuiState {
    /// Real tabs, selected deck index and application-owned add capability.
    /// On Home with retained tabs, the selected synthetic slot is at `tabs.len()`.
    /// An empty slice means the legacy single-session/stripless-Home view.
    pub fn tab_presentation(&self) -> (&[TabPresentation], usize, bool) {
        (
            &self.tabs,
            if self.home && !self.tabs.is_empty() {
                self.tabs.len()
            } else {
                self.active_tab
            },
            self.can_add_tab,
        )
    }

    /// Refresh the retained deck. The binary owns route selection and the add
    /// action; Home itself becomes a synthetic final slot only in the renderer.
    pub fn set_tab_strip(&mut self, tabs: Vec<TabPresentation>, active: usize, can_add: bool) {
        self.set_tab_strip_at(tabs, active, can_add, Instant::now());
    }

    /// A same-Location view replacement does not unmount the visible deck.
    /// Move its bounded component clocks; parked transcript views own no copy.
    /// The caller has already admitted the route. Keep an actual same-Location
    /// pointer; reconciliation validates its stable ID against the new layout.
    pub fn take_tab_clocks_from(&mut self, previous: &mut Self) {
        let view = self.tab_view.get_mut();
        *view = std::mem::take(previous.tab_view.get_mut());
        if self.chrome.location == previous.chrome.location {
            self.last_mouse = previous.last_mouse.take();
        } else {
            *view = TabView::default();
            self.last_mouse = None;
        }
        self.tab_scroll.set(previous.tab_scroll.get());
    }

    pub(super) fn set_tab_strip_at(
        &mut self,
        tabs: Vec<TabPresentation>,
        active: usize,
        can_add: bool,
        now: Instant,
    ) {
        if self.active_tab != active.min(tabs.len().saturating_sub(1)) {
            self.clear_transcript_selection();
            self.tab_scroll.set(active);
        }
        self.tab_down = None;
        let count = tabs.len().min(16);
        if self.tabs.len() != count
            || self.active_tab != active.min(count.saturating_sub(1))
            || self.can_add_tab != (can_add && count > 0)
        {
            self.close_hold = None;
        }
        self.tabs = tabs.into_iter().take(16).collect();
        self.active_tab = active.min(self.tabs.len().saturating_sub(1));
        self.can_add_tab = can_add && !self.tabs.is_empty();
        let area = self.tab_view.borrow().area;
        if let Some(area) = area {
            self.prepare_tabs(area, now);
        }
    }

    /// Clear a mouse surface when it is hidden or no longer admitted.
    pub fn clear_mouse_position(&mut self) {
        self.painted_prompt.borrow_mut().take();
        self.clear_transcript_selection();
        self.last_mouse = None;
        self.toast_down = false;
        self.set_toast_hover(false, Instant::now());
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        self.tab_down = None;
        self.reasoning_down = None;
    }

    /// Resize invalidates press targets, not a still-visible source marquee.
    /// Re-hit-test the same physical pointer against the admitted new layout.
    pub fn resize_mouse_position(&mut self, area: Rect) {
        self.painted_prompt.borrow_mut().take();
        self.clear_transcript_selection();
        self.toast_down = false;
        self.set_toast_hover(false, Instant::now());
        self.close_hold = None;
        self.tab_down = None;
        self.reasoning_down = None;
        self.last_mouse = self.last_mouse.and_then(|(x, y, _)| {
            let index = crate::shell::tab_strip(self, area)?.hit_test(x, y)?;
            let id = self.tab_identity(index)?;
            (self.tab_view.borrow().hovered.as_ref() == Some(&id)).then_some((x, y, area))
        });
        self.prepare_tabs(area, Instant::now());
    }

    /// Recover hover after a successful mouse tab activation (never on keys).
    pub fn restore_mouse_hover(&mut self, pointer: (u16, u16, Rect)) {
        let (x, y, area) = pointer;
        if self.panel != TuiPanel::None {
            return;
        }
        self.last_mouse = Some(pointer);
        self.enter_tab_at(area, x, y, Instant::now());
    }

    /// A keyboard close can replace the view that received the last real mouse
    /// event. Only restore the add hover if that same pointer hits the newly
    /// painted, owner-enabled control; keys never create a mouse close hold.
    pub fn restore_tab_hover_at(&mut self, pointer: (u16, u16, Rect)) {
        let (x, y, area) = pointer;
        if self.panel != TuiPanel::None || self.is_busy() {
            return;
        }
        if crate::shell::tab_strip(self, area)
            .and_then(|strip| strip.add)
            .is_some_and(|add| add.width == 3 && add.contains((x, y).into()))
        {
            self.last_mouse = Some(pointer);
            self.tab_view.get_mut().reset_hover();
        }
    }

    pub(crate) fn tab_add_hovered(&self, area: Rect, add: Rect) -> bool {
        self.panel == TuiPanel::None
            && !self.is_busy()
            && (crate::layout::vertical_tabs_width(area.width, self.chrome.vertical_tabs_width) > 0
                || add.width == 3)
            && self.last_mouse.is_some_and(|(x, y, pointer_area)| {
                pointer_area == area && add.contains((x, y).into())
            })
    }

    pub fn mouse_position(&self) -> Option<(u16, u16, Rect)> {
        self.last_mouse
    }

    /// Snapshot must be taken before the owner mutates the deck. A plain
    /// CloseTab intent (e.g. keyboard) cannot create a mouse hold.
    pub fn mouse_close_snapshot(&self, index: usize) -> Option<TabCloseSnapshot> {
        let (x, y, area) = self.last_mouse?;
        let region = crate::shell::tab_region(self, area);
        if self.panel != TuiPanel::None
            || self.is_busy()
            || region.height != 1
            || region.width != area.width
            || self.tab_hit(area, x, y) != Some(TabPress::Close(index))
        {
            return None;
        }
        Some(TabCloseSnapshot {
            area,
            strip: crate::shell::tab_strip(self, area)?,
            tabs: self.tabs.clone(),
            home: self.home,
            closed: index,
            pointer: (x, y),
        })
    }

    /// `session-tabs.tsx:237-274,1446-1467`: hold the adjacent visible
    /// survivor under the release column for up to five seconds. Reject a
    /// changed deck or a clipped cell; re-hit-test exactly what is painted.
    pub fn restore_mouse_close(&mut self, snapshot: TabCloseSnapshot) {
        let old_count = snapshot.tabs.len() + usize::from(snapshot.home);
        let new_count = self.tabs.len() + usize::from(self.home);
        if self.panel != TuiPanel::None || self.is_busy() || old_count != new_count + 1 {
            return;
        }
        let old_tabs = snapshot.tabs.len();
        let mut old = snapshot.tabs;
        if snapshot.closed < old.len() {
            old.remove(snapshot.closed);
        }
        if old != self.tabs || (snapshot.closed != old_tabs && snapshot.home != self.home) {
            return;
        }
        let target = if snapshot.closed < old_count - 1 {
            snapshot.closed
        } else if snapshot.closed > 0 {
            snapshot.closed - 1
        } else {
            return;
        };
        let region = crate::shell::tab_region(self, snapshot.area);
        if region.height != 1 || region.width != snapshot.area.width {
            return; // The surviving route changed orientation or terminal geometry.
        }
        if let Some(strip) = crate::layout::held_after_close(
            &snapshot.strip,
            region,
            snapshot.closed,
            new_count,
            target,
            snapshot.pointer.0,
            !self.home && self.can_add_tab,
        ) {
            self.close_hold = Some(TabCloseHold {
                area: snapshot.area,
                strip,
                until: Instant::now() + std::time::Duration::from_secs(5),
            });
        }
        self.restore_mouse_hover((snapshot.pointer.0, snapshot.pointer.1, snapshot.area));
    }

    pub(super) fn tab_hit(&self, area: Rect, x: u16, y: u16) -> Option<TabPress> {
        let strip = crate::shell::tab_strip(self, area)?;
        if strip
            .add
            .is_some_and(|rect| (strip.vertical || rect.width == 3) && rect.contains((x, y).into()))
        {
            return Some(TabPress::Add);
        }
        let index = strip.hit_test(x, y)?;
        if strip
            .tabs
            .iter()
            .find(|tab| tab.index == index)
            .filter(|tab| tab.rect.y == y)
            .and_then(|tab| self.tab_close_cell(area, tab.index, tab.rect))
            == Some(x)
        {
            return Some(TabPress::Close(index));
        }
        // The promoted Home slot is already selected; its click is not an
        // application action. Only retained real tabs have activation intents.
        (index < self.tabs.len()).then_some(TabPress::Tab(index))
    }

    fn tab_identity(&self, index: usize) -> Option<TabIdentity> {
        if let Some(tab) = self.tabs.get(index) {
            Some(TabIdentity::Session(tab.session.clone()))
        } else if self.home && !self.tabs.is_empty() && index == self.tabs.len() {
            Some(TabIdentity::Home)
        } else if self.tabs.is_empty() && index == 0 {
            self.session.clone().map(TabIdentity::Session)
        } else {
            None
        }
    }

    /// Fresh-Home promotion label, independent of durable title/history refresh.
    pub fn tab_title_fallback(&self) -> Option<&'static str> {
        self.new_session_tab
            .then_some(crate::shell::NEW_SESSION_TAB_TITLE)
    }

    pub(crate) fn tab_title(&self, index: usize) -> &str {
        if self.home && index == self.tabs.len() {
            crate::shell::NEW_SESSION_TAB_TITLE
        } else {
            self.tabs
                .get(index)
                .and_then(|tab| tab.title.as_deref())
                .or_else(|| {
                    self.tabs
                        .is_empty()
                        .then_some(
                            self.session_title
                                .as_deref()
                                .or_else(|| self.tab_title_fallback()),
                        )
                        .flatten()
                })
                .unwrap_or(crate::shell::UNTITLED_SESSION)
        }
    }

    pub(crate) fn tab_attention_for(&self, index: usize) -> Option<TabAttention> {
        let attention = self.tabs.get(index).and_then(|tab| tab.attention);
        if self.tab_attention.contains(&index) || attention == Some(TabAttention::Permission) {
            Some(TabAttention::Permission)
        } else {
            attention
        }
    }

    pub(crate) fn tab_busy(&self, index: usize) -> bool {
        self.tabs
            .get(index)
            .map_or_else(|| self.is_busy() && !self.home, |tab| tab.busy)
    }

    pub(crate) fn tab_number_width(&self) -> usize {
        (self.tabs.len() + usize::from(self.home))
            .to_string()
            .len()
            .max(2)
    }

    /// Reconcile mounts against the actual painted deck, never the archive.
    /// Source U56 resets when the active tab disappears or compactness changes;
    /// title/status/width changes alone do not re-enter an unchanged identity.
    pub(crate) fn prepare_tabs(&self, area: Rect, now: Instant) {
        let strip = crate::shell::tab_strip(self, area);
        if let Some(strip) = strip.as_ref().filter(|strip| strip.vertical) {
            self.tab_scroll.set(strip.start);
        }
        let mut view = self.tab_view.borrow_mut();
        let vertical = strip.as_ref().is_some_and(|strip| strip.vertical);
        let compact = strip.as_ref().is_some_and(|strip| strip.compact);
        if view.vertical != vertical || view.compact != compact {
            // The source Switch/Show mounts new indicator/pulse components.
            view.motions.clear();
        }
        if view.vertical != vertical || view.compact != compact || self.panel != TuiPanel::None {
            view.reset_hover();
        }
        view.area = Some(area);
        view.vertical = vertical;
        view.compact = compact;
        let visible: Vec<_> = strip
            .iter()
            .flat_map(|strip| &strip.tabs)
            .filter(|tab| tab.rect.width > 0 && tab.rect.height > 0)
            .filter_map(|tab| self.tab_identity(tab.index).map(|id| (id, tab.index)))
            .collect();
        if view
            .hovered
            .as_ref()
            .is_some_and(|id| !visible.iter().any(|(visible, _)| visible == id))
            || view
                .marquee
                .as_ref()
                .is_some_and(|marquee| !visible.iter().any(|(id, _)| id == &marquee.id))
        {
            view.reset_hover();
        }
        if let Some(id) = view.hovered.as_ref()
            && !self.last_mouse.is_some_and(|(x, y, _)| {
                strip
                    .as_ref()
                    .and_then(|strip| strip.hit_test(x, y))
                    .and_then(|index| self.tab_identity(index))
                    .as_ref()
                    == Some(id)
            })
        {
            view.reset_hover();
        }
        view.motions
            .retain(|spinner| visible.iter().any(|(id, _)| id == &spinner.id));
        for (id, index) in visible {
            let busy = self.tab_busy(index);
            let attention = self.tab_attention_for(index).is_some();
            let selected = if self.home {
                id == TabIdentity::Home
            } else {
                index == self.active_tab
            };
            let complete = self.tabs.get(index).is_some_and(|tab| tab.complete) && !busy;
            let glow = attention || (complete && !selected);
            let prompt = self.tabs.get(index).map_or(0, |tab| tab.prompt_pulse);
            let animations = self.chrome.animations != Some(false);
            let running = busy && !attention;
            let spinner_running = running
                && animations
                && self.chrome.tab_indicators == oc_core::queries::TabIndicators::Status;
            let target = TabPulseTarget {
                animations,
                runs: running,
                complete,
                glows: glow,
                prompt,
                dimmed: selected && attention,
                vertical,
                compact,
                numbers: self.chrome.tab_indicators == oc_core::queries::TabIndicators::Numbers,
            };
            let spinner =
                if let Some(position) = view.motions.iter().position(|motion| motion.id == id) {
                    &mut view.motions[position]
                } else {
                    view.motions.push(TabMotion {
                        id,
                        at: None,
                        frame: 0,
                        pulse: TabPulse::new(target, now),
                    });
                    view.motions.last_mut().expect("inserted tab motion")
                };
            spinner.pulse.sync(target, now);
            if spinner_running {
                spinner.at.get_or_insert(now);
            } else {
                spinner.at = None;
                spinner.frame = 0;
            }
        }
    }

    pub(super) fn enter_tab_at(&mut self, area: Rect, x: u16, y: u16, now: Instant) {
        self.prepare_tabs(area, now);
        let Some(strip) = crate::shell::tab_strip(self, area) else {
            return;
        };
        let Some(tab) = strip
            .tabs
            .iter()
            .find(|tab| tab.rect.contains((x, y).into()))
        else {
            // U56's zero-delay leave lets a nested close-control enter cancel it.
            let view = self.tab_view.get_mut();
            view.hovered = None;
            view.leave = Some(now);
            return;
        };
        let Some(id) = self.tab_identity(tab.index) else {
            return;
        };
        let width = crate::layout::tab_title_width(
            tab.rect.width,
            self.tab_number_width(),
            strip.vertical,
            strip.compact,
            true,
        );
        let title = self.tab_title(tab.index);
        let overflow =
            width.is_some_and(|width| unicode_width::UnicodeWidthStr::width(title) > width);
        let cycle = unicode_width::UnicodeWidthStr::width(title) + 3;
        let view = self.tab_view.get_mut();
        view.leave = None;
        view.hovered = Some(id.clone());
        if !overflow {
            view.marquee = None;
            return;
        }
        if view
            .marquee
            .as_ref()
            .is_some_and(|marquee| marquee.id == id)
        {
            return;
        }
        view.marquee = Some(TabMarquee {
            id,
            first: now + TAB_MARQUEE_DELAY,
            cycle,
            offset: 0,
            done: false,
            leading: 0.0,
            fade_at: None,
        });
    }

    pub(crate) fn hovered_tab(&self, area: Rect) -> Option<usize> {
        if self.panel != TuiPanel::None {
            return None;
        }
        let (x, y, _) = self.last_mouse?;
        let Some(index) =
            crate::shell::tab_strip(self, area).and_then(|strip| strip.hit_test(x, y))
        else {
            self.tab_view.borrow_mut().reset_hover();
            return None;
        };
        let id = self.tab_identity(index)?;
        (self.tab_view.borrow().hovered.as_ref() == Some(&id)).then_some(index)
    }

    pub fn tab_wheel_hit(&self, area: Rect, x: u16, y: u16) -> bool {
        crate::layout::vertical_tabs_width(area.width, self.chrome.vertical_tabs_width) > 0
            && crate::shell::tab_region(self, area).contains((x, y).into())
    }

    pub(crate) fn tab_animation(&self, index: usize) -> (usize, f32, usize) {
        let Some(id) = self.tab_identity(index) else {
            return (0, 0.0, 0);
        };
        let view = self.tab_view.borrow();
        let frame = view
            .motions
            .iter()
            .find(|spinner| spinner.id == id)
            .map_or(0, |spinner| spinner.frame);
        view.marquee
            .as_ref()
            .filter(|marquee| marquee.id == id)
            .map_or((0, 0.0, frame), |marquee| {
                (marquee.offset, marquee.leading, frame)
            })
    }

    pub(crate) fn tab_pulse(&self, index: usize) -> TabPulseFrame {
        let Some(id) = self.tab_identity(index) else {
            return TabPulseFrame::default();
        };
        self.tab_view
            .borrow()
            .motions
            .iter()
            .find(|spinner| spinner.id == id)
            .map_or_else(TabPulseFrame::default, |spinner| spinner.pulse.frame())
    }

    pub(super) fn next_tab_deadline(&self) -> Option<Instant> {
        let view = self.tab_view.borrow();
        let spinner = (self.chrome.animations != Some(false)
            && self.chrome.tab_indicators == oc_core::queries::TabIndicators::Status)
            .then(|| {
                view.motions
                    .iter()
                    .filter_map(|spinner| spinner.at.map(|at| at + TAB_STEP))
                    .min()
            })
            .flatten();
        [
            spinner,
            view.motions
                .iter()
                .filter_map(|spinner| spinner.pulse.deadline())
                .min(),
            view.leave,
            view.marquee.as_ref().and_then(TabMarquee::deadline),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(super) fn tick_tabs(&mut self, now: Instant) -> bool {
        let area = self.tab_view.borrow().area;
        if let Some(area) = area {
            self.prepare_tabs(area, now);
        }
        let view = self.tab_view.get_mut();
        let mut changed = false;
        if view.leave.is_some_and(|at| now >= at) {
            changed = view.hovered.is_some() || view.marquee.is_some();
            view.reset_hover();
        }
        for spinner in &mut view.motions {
            changed |= spinner.pulse.tick(now);
            let Some(at) = spinner.at else {
                continue;
            };
            let steps = now.saturating_duration_since(at).as_millis() / 80;
            if steps == 0 {
                continue;
            }
            let frame = (spinner.frame + (steps % 10) as usize) % 10;
            changed |= frame != spinner.frame;
            spinner.frame = frame;
            spinner.at = Some(
                now - Duration::from_nanos(
                    (now.saturating_duration_since(at).as_nanos() % TAB_STEP.as_nanos()) as u64,
                ),
            );
        }
        if let Some(marquee) = &mut view.marquee {
            changed |= marquee.tick(now, self.chrome.animations != Some(false));
        }
        changed
    }

    /// Same eligibility and cell for the painted overlay and mouse action.
    pub fn tab_close_cell(&self, area: Rect, index: usize, rect: Rect) -> Option<u16> {
        (!self.tabs.is_empty()
            && (index < self.tabs.len() || (self.home && index == self.tabs.len()))
            && self.hovered_tab(area) == Some(index)
            && !self.is_busy()
            && self.panel == TuiPanel::None
            && !(crate::layout::vertical_tabs_width(area.width, self.chrome.vertical_tabs_width)
                > 0
                && rect.width < crate::layout::SESSION_TABS_COMPACT_BREAKPOINT)
            && !self.tabs.get(index).is_some_and(|tab| tab.busy))
        .then(|| crate::layout::tab_close_cell(rect))
        .flatten()
    }
}
