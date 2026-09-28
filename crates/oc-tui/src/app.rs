//! Bounded chat state over the shared `CoreApp` handle.
//!
//! The view never touches storage: history pages, catalogs, skills and DCP
//! snapshots arrive as bounded application DTOs, and user choices leave as
//! [`PanelIntent`] values that the binary applies through the application
//! API (then reports acceptance or failure). Worker event draining stays in
//! the binary; the state only applies turn-scoped events, so a late event
//! for a stale turn can never corrupt the view.
//! Shared fixtures live in `app/tests.rs`; scenarios are grouped by input,
//! transcript, tabs and turn lifecycle in its child modules.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use oc_adapters::models::ModelCatalog;
use oc_core::core_app::{
    CoreApp, CoreEvent, MAX_SESSION_TITLE_BYTES, SubmissionReceipt, WorkerTurnId,
};
use oc_core::domain::SessionId;
use oc_core::queries::{
    AgentEntry, CatalogSnapshot, DcpSnapshot, FileSuggestionsSnapshot, HistoryPage, SkillCard,
    ToolOpView,
};
use oc_core::session::CoreError;
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use crate::commands::{CommandAction, dispatch};
use crate::dcp_panel::{DcpOutcome, DcpPanelState};
use crate::events::KeyAction;
use crate::history::{HistoryRow, HistoryWindow, ToolCard, WINDOW_BYTES, card_from_row};
use crate::messages::{AssistantMeta, ReasoningBlock};
use crate::picker::ModelPicker;
use crate::styled::Line;
use crate::theme::Theme;

/// Visible lines kept in the viewport (scroll window).
pub const VIEWPORT_LINES: usize = 20;
/// Bounded input buffer (bytes): the core input budget, so the view never
/// drops bytes the runtime would have accepted.
pub const MAX_INPUT_BYTES: usize = oc_core::session::MAX_INPUT_BYTES;
/// Max card rows retained by the Cards panel.
pub const CARDS_MAX: usize = 160;
/// Max live turn parts kept before the oldest is evicted (defensive: the
/// runtime caps rounds, so a real turn stays far below this).
pub const LIVE_PARTS_MAX: usize = 64;
/// Maximum rows fetched for one inline mention (owner traversal is separately bounded).
pub const MENTION_LIMIT: usize = 10;
/// Never send a clipboard payload larger than a bounded visible transcript.
pub const MAX_SELECTION_BYTES: usize = 64 * 1024;
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

/// Exact identity of one focused file query. A tab can park and return with
/// the same draft, so the key includes the view instance as well as its edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionRequest {
    pub query: String,
    pub location: String,
    pub view_id: u64,
    pub generation: u64,
    pub revision: u64,
    pub caret: usize,
    pub start: usize,
}

/// Provider-reported usage for the active turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TurnUsage {
    input_tokens: u64,
    output_tokens: u64,
    streamed_ms: u64,
}

/// TUI status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiStatus {
    /// Ready for input.
    Idle,
    /// Input queued; the application has not yet accepted a turn.
    PendingSubmission,
    /// Streaming a turn.
    Streaming,
    /// Last turn was cancelled.
    Cancelled,
    /// Should exit the event loop.
    Quit,
}

/// Upstream toast feedback roles (`ui/toast.tsx:7-16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteVariant {
    Info,
    Success,
    Warning,
    Error,
}

struct ToastExpiry {
    remaining: Duration,
    started: Option<Instant>,
}

/// Open TUI panel (bounded view state; one at a time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiPanel {
    Settings,
    MessageActions {
        message: oc_core::session::MessageId,
        seq: i64,
    },
    /// Genuine native command registry.
    Commands,
    /// No panel (chat view).
    None,
    /// Model picker (UI02).
    Model,
    /// Declared variants of the effective application model, including Default.
    Variant,
    /// Primary agent selector.
    Agents,
    /// Session list with resume (UI03).
    Sessions,
    /// Focused single-line session title editor.
    Rename,
    /// Skill catalog (UI06).
    Skills,
    /// Help, optionally for one topic.
    Help(Option<String>),
    /// DCP context panel (UI04).
    Dcp,
    /// Tool cards from the runtime (newest first, paged).
    Cards,
}

/// Work the panel asked the binary to apply through the application API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelIntent {
    SetPermissionMode {
        auto_once: bool,
    },
    ReplyApproval(oc_core::approval::ApprovalReply),
    CompactSession,
    ChangeConversation {
        action: oc_core::queries::ConversationAction,
    },
    ForkMessage {
        message: oc_core::session::MessageId,
    },
    CopyMessage {
        message: oc_core::session::MessageId,
        seq: i64,
    },
    /// Load the model/agent catalog snapshot.
    LoadCatalog,
    /// Rebuild the current Location without replacing the session or draft.
    ReloadConfiguration,
    /// Load the session list snapshot.
    LoadSessions,
    /// Load the skill card snapshot.
    LoadSkills,
    /// Load the newest tool-card page.
    LoadCards,
    /// Continue reading one card's durable result through the owning application.
    LoadCardOutput {
        op: String,
        offset: usize,
    },
    /// Create an empty application session and attach its Home route.
    NewSession,
    /// Activate a retained real tab by its zero-based deck index.
    ActivateTab {
        index: usize,
    },
    /// Close a retained tab; `tabs.len()` denotes the synthetic Home slot.
    CloseTab {
        index: usize,
    },
    /// Apply the trimmed title to the attached session through the owner.
    RenameSession {
        title: String,
    },
    /// Apply a slash-supplied title without opening the editor; ACK clears the slash draft.
    RenameSessionDirect {
        title: String,
    },
    RenameSelectedSession {
        id: String,
        title: String,
    },
    DeleteSelectedSession {
        id: String,
    },
    /// Bare slash command: ask the owner to generate a fresh title.
    RegenerateTitle,
    /// Select a model, restoring the owner's remembered variant preference.
    SelectModel {
        /// Exact model id.
        id: String,
    },
    /// Apply an exact model + variant choice.
    ChooseModel {
        /// Exact model id.
        id: String,
        /// Optional variant name.
        variant: Option<String>,
    },
    /// Cycle from the owner's current model/variant, never a stale UI snapshot.
    CycleVariant,
    /// Apply a primary agent choice.
    SelectAgent {
        /// Agent profile id.
        id: String,
    },
    /// Resume another session.
    SwitchSession {
        /// Target session id.
        id: String,
    },
    /// Switch the whole application to another Location (project path).
    SwitchLocation {
        /// Target project path.
        path: String,
    },
    /// Load one older history page.
    LoadOlder,
    /// Load one newer history page.
    LoadNewer,
    /// Request a manual DCP compression.
    Compress {
        /// Bounded focus instruction (possibly empty).
        focus: String,
    },
}

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
enum TabIdentity {
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

pub(crate) fn tab_smootherstep(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

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
struct TabView {
    area: Option<Rect>,
    vertical: bool,
    compact: bool,
    hovered: Option<TabIdentity>,
    leave: Option<Instant>,
    marquee: Option<TabMarquee>,
    motions: Vec<TabMotion>,
}

impl TabView {
    fn reset_hover(&mut self) {
        self.hovered = None;
        self.leave = None;
        self.marquee = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabPress {
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

/// One key handling result: optional status note, optional intent for the
/// binary to apply, and whether the input buffer was consumed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyOutcome {
    /// Status note for the user (error or hint).
    pub note: Option<String>,
    /// Intent the binary must apply through the application API.
    pub intent: Option<PanelIntent>,
    /// True when the key was fully handled and the input was cleared.
    pub consumed_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMode {
    Select,
    Manual,
    /// A published Location no longer matches this view; wait for an owner
    /// catalog before allowing either automatic or explicit clipboard writes.
    Disabled,
}

impl Default for ClipboardMode {
    fn default() -> Self {
        if cfg!(windows) {
            Self::Manual
        } else {
            Self::Select
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TextPoint {
    row: usize,
    byte: usize,
}

#[derive(Clone)]
struct PaintedTranscript {
    area: Rect,
    rows: Vec<Line>,
    user_targets: Vec<Option<crate::messages::UserMessageTarget>>,
    total: usize,
    scroll: usize,
}

struct TranscriptSelection {
    anchor: TextPoint,
    focus: TextPoint,
    painted: PaintedTranscript,
    dragging: bool,
}

impl TranscriptSelection {
    fn bounds(&self) -> (TextPoint, TextPoint) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

#[derive(Clone, Copy)]
struct TranscriptClick {
    x: u16,
    y: u16,
    at: Instant,
    count: u8,
}

/// One live part of the streaming turn, in arrival order (upstream message
/// parts: reasoning, text, tool — `routes/session/index.tsx:1433-1481`).
///
/// Text/reasoning segments freeze when a tool call starts so tool cards keep
/// their upstream position between the text parts; the transient recorded
/// input stays with an in-flight card until its outcome arrives, then the
/// card is rebuilt and the input dropped (bounded live state).
#[derive(Debug, Clone, PartialEq, Eq)]
enum LivePart {
    /// Removed transient slot; keeps subsequent live reasoning identities stable.
    Vacant,
    /// Frozen assistant text segment.
    Text(String),
    /// Frozen reasoning segment with its measured window.
    Reasoning {
        text: String,
        duration_ms: Option<u64>,
    },
    /// One tool call card plus the recorded input of an in-flight call.
    Tool { card: Box<ToolCard>, input: String },
}

/// One disposable, byte-bounded tool-output page in the Cards dialog.
pub(crate) struct CardOutput {
    pub op: String,
    pub offset: usize,
    pub page: oc_core::queries::ToolOutputPage,
}

impl LivePart {
    /// Bounded bytes retained by this part (the transient in-flight input is
    /// excluded: it is dropped as soon as the outcome arrives).
    fn retained_bytes(&self) -> usize {
        match self {
            LivePart::Vacant => 0,
            LivePart::Text(text) | LivePart::Reasoning { text, .. } => text.len(),
            LivePart::Tool { card, input } => card.retained_bytes() + input.len(),
        }
    }

    /// Render this part as a transcript row.
    fn to_row(
        &self,
        agent: Option<String>,
        identity: Option<crate::messages::ReasoningIdentity>,
    ) -> HistoryRow {
        match self {
            LivePart::Vacant => unreachable!("vacant parts are never projected"),
            LivePart::Text(text) => HistoryRow {
                message_id: None,
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: text.clone(),
                agent,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: None,
            },
            LivePart::Reasoning { text, duration_ms } => HistoryRow {
                message_id: None,
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: String::new(),
                agent,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: Some(ReasoningBlock {
                    text: text.clone(),
                    duration_ms: *duration_ms,
                    running: false,
                    expanded: false,
                    toggleable: true,
                    identity,
                }),
                meta: None,
                tool: None,
            },
            LivePart::Tool { card, .. } => HistoryRow {
                message_id: None,
                seq: i64::MAX,
                role: "tool".to_string(),
                text: String::new(),
                agent,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: Some((**card).clone()),
            },
        }
    }
}

/// Numeric view-owned state sampled only by the opt-in terminal metrics loop.
/// Text/reasoning include open buffers and frozen segments; part count is the
/// frozen `live_parts` list (tool cards included), not the render projection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LiveViewMetrics {
    pub text_bytes: usize,
    pub reasoning_bytes: usize,
    pub part_count: usize,
    pub markdown_cache_retained_bytes: usize,
}

impl std::ops::AddAssign for LiveViewMetrics {
    fn add_assign(&mut self, other: Self) {
        self.text_bytes += other.text_bytes;
        self.reasoning_bytes += other.reasoning_bytes;
        self.part_count += other.part_count;
        self.markdown_cache_retained_bytes += other.markdown_cache_retained_bytes;
    }
}

/// Immutable submission identity plus the editable draft's revision at enqueue.
struct PendingSubmission {
    request_id: u64,
    generation: u64,
    session: SessionId,
    fresh: bool,
    draft: String,
    mentions: Vec<(usize, usize)>,
    revision: u64,
    receipt: SubmissionReceipt,
    agent: Option<String>,
    agent_color_index: Option<usize>,
    cancelling: bool,
    compress: bool,
}

/// Upstream Home normal-mode examples (`routes/home.tsx:19-23`).
pub(crate) const HOME_EXAMPLES: [&str; 3] = [
    "Fix a TODO in the codebase",
    "What is the tech stack of this project?",
    "Fix broken tests",
];

#[derive(Clone, Copy, PartialEq, Eq)]
struct TranscriptViewport {
    width: u16,
    terminal_width: u16,
    height: u16,
    total: usize,
    requested_scroll: usize,
    displayed_scroll: usize,
}

/// One warm screen, never another retained-history/archive cache. Selection
/// highlighting is applied after this projection on every paint.
struct VisibleTranscriptProjection {
    revision: u64,
    window_revision: u64,
    viewport: Option<TranscriptViewport>,
    live_lengths: (usize, usize),
    dimensions: (u16, u16, u16),
    requested_scroll: usize,
    chrome: oc_core::queries::TuiChrome,
    theme: crate::theme::ThemeMode,
    thinking: bool,
    reasoning: BTreeSet<crate::messages::ReasoningIdentity>,
    exploration: BTreeSet<String>,
    compaction_frame: usize,
    lines: Vec<Line>,
    total: usize,
    scroll: usize,
    targets: Vec<Option<crate::messages::UserMessageTarget>>,
}

/// Only actual visible chip cells from the latest prompt paint are actionable.
struct PaintedPrompt {
    frame: Rect,
    main: Rect,
    home: bool,
    session: Option<SessionId>,
    revision: u64,
    generation: u64,
    cursor: usize,
    anchor: Option<usize>,
    chips: Vec<(Rect, usize)>,
}

struct CompletionAnchor {
    message: std::sync::Arc<oc_core::session::MessageId>,
    part: usize,
    row: usize,
    requested_scroll: usize,
    pending: bool,
}

/// Native row-quantized presentation of a wheel target. OC2/OpenTUI 0.5.10
/// applies wheel displacement immediately (ScrollBox.onMouseEvent); it has no
/// wheel inertia. We distribute that displacement over one OC2 paint budget
/// (app.tsx targetFps: 60), never retain a velocity or replay an input queue.
#[derive(Clone, Copy)]
struct WheelMotion {
    started: Instant,
    distance: usize,
    applied: usize,
    up: bool,
}

const WHEEL_PRESENTATION: Duration = Duration::from_nanos(16_666_667);

/// Bounded chat state on the shared handle, optionally attached to a session.
pub struct TuiState {
    pub approval_roots: BTreeSet<String>,
    pub tab_attention: BTreeSet<usize>,
    pub approvals: crate::approval_view::ApprovalView,
    pub chrome: oc_core::queries::TuiChrome,
    pub parent_id: Option<String>,
    /// New interactive launch, distinct from an explicitly attached session.
    pub home: bool,
    /// Sampled once per UI instance; never changes during a redraw.
    pub(crate) home_example: &'static str,
    viewport: std::cell::Cell<Option<TranscriptViewport>>,
    completion_anchor: std::cell::RefCell<Option<CompletionAnchor>>,
    painted_transcript: std::cell::RefCell<Option<PaintedTranscript>>,
    painted_prompt: std::cell::RefCell<Option<PaintedPrompt>>,
    prompt_width: std::cell::Cell<Option<usize>>,
    paint_generation: std::cell::Cell<u64>,
    message_down: Option<(oc_core::session::MessageId, u64, Rect)>,
    clipboard_mode: ClipboardMode,
    /// Owner availability, when known. Fresh/reopened views let the owner decide.
    conversation_available: Option<(bool, bool)>,
    reverted: Option<oc_core::queries::RevertedConversation>,
    reverted_down: Option<(oc_core::queries::RevertedConversation, u64, Rect)>,
    conversation_bindings: [Option<String>; 2],
    leader_key: String,
    /// Last successful owner projection, independent of manual/test overrides.
    owner_clipboard_mode: Option<oc_core::queries::TerminalCopyMode>,
    pending_copy: Option<String>,
    selection: Option<TranscriptSelection>,
    selection_gesture: bool,
    click: Option<TranscriptClick>,
    /// Current session's durable human title, refreshed with history.
    pub session_title: Option<String>,
    /// Session autoaccept capability supplied by the application.
    pub auto_accept: oc_core::queries::AutoAcceptState,
    app: CoreApp,
    session: Option<SessionId>,
    status: TuiStatus,
    scanner_frame: usize,
    compactions: Vec<oc_core::compaction::CompactionSnapshot>,
    compaction_at: Option<Instant>,
    compaction_frame: usize,
    compaction_turn_messages: std::collections::BTreeMap<String, String>,
    scanner_at: Option<Instant>,
    panel: TuiPanel,
    pub(crate) select: crate::dialog::SelectList,
    /// Press origin prevents drag-release across the backdrop from dismissing a dialog.
    mouse_down: Option<crate::dialog::DialogHit>,
    tab_down: Option<TabPress>,
    tab_view: std::cell::RefCell<TabView>,
    pub(crate) tab_scroll: std::cell::Cell<usize>,
    last_mouse: Option<(u16, u16, Rect)>,
    pub(crate) close_hold: Option<TabCloseHold>,
    tabs: Vec<TabPresentation>,
    active_tab: usize,
    can_add_tab: bool,
    /// Only operation IDs whose exploration headers were explicitly opened.
    exploration_expanded: BTreeSet<String>,
    exploration_down: Option<(String, u16, u16)>,
    reasoning_expanded: BTreeSet<crate::messages::ReasoningIdentity>,
    reasoning_down: Option<(
        crate::messages::ReasoningIdentity,
        u16,
        u16,
        Rect,
        usize,
        usize,
        usize,
    )>,
    /// An unfinished press/drag cannot masquerade as a standalone header UP,
    /// even when scrolling, resizing or an overlay invalidates its hit target.
    reasoning_pointer_down: bool,
    reasoning_epoch: u64,
    /// Number of evicted frozen parts in this turn; ordinals never shift.
    live_part_offset: usize,
    leader: Option<Instant>,
    input: String,
    editor: crate::editor::Editor,
    rename_input: String,
    rename_editor: crate::editor::Editor,
    rename_pending: Option<String>,
    rename_selected: Option<String>,
    session_delete_confirm: Option<String>,
    session_project_name: Option<String>,
    session_scope_pending: Option<bool>,
    rename_direct_pending: Option<(String, u64)>,
    regenerate_pending: Option<u64>,
    window: HistoryWindow,
    markdown_cache: std::cell::RefCell<crate::messages::MarkdownCache>,
    transcript_revision: u64,
    visible_projection: std::cell::RefCell<Option<VisibleTranscriptProjection>>,
    #[cfg(test)]
    transcript_row_copies: std::cell::Cell<usize>,
    #[cfg(test)]
    visible_projection_builds: std::cell::Cell<usize>,
    live_text: String,
    /// Reasoning text streamed for the active turn (never persisted).
    live_reasoning: String,
    thinking_expanded: bool,
    /// Frozen live parts (text/reasoning segments and tool cards) of the
    /// active turn, in arrival order.
    live_parts: Vec<LivePart>,
    pending_tool_seen: Vec<String>,
    pending_tool_round: u32,
    /// Explicit durable live identities; replaced at each application checkpoint.
    pub live_part_states: Vec<oc_core::queries::PartState>,
    live_agent_color_index: Option<usize>,
    live_terminal_status: Option<String>,
    live_model_label: Option<String>,
    live_preview_truncated: bool,
    /// First reasoning delta of the active turn, for the collapsed header's
    /// duration (`part.time.created` upstream).
    reasoning_started: Option<Instant>,
    /// When text streaming followed reasoning (`part.time.completed`).
    reasoning_finished: Option<Instant>,
    /// Provider usage reported for the active turn (never synthesized).
    turn_usage: Option<TurnUsage>,
    scroll: usize,
    wheel_motion: Option<WheelMotion>,
    note: Option<(String, NoteVariant)>,
    toast_expiry: Option<ToastExpiry>,
    toast_down: bool,
    active_turn: Option<WorkerTurnId>,
    /// First Esc arms the running turn for five seconds; only a second press
    /// interrupts it (pinned prompt/index.tsx:499–528).
    interrupt_armed_until: Option<Instant>,
    pending: Option<PendingSubmission>,
    compress_turn: Option<WorkerTurnId>,
    request_id: u64,
    generation: u64,
    input_revision: u64,
    slash_selected: usize,
    slash_dismissed: Option<u64>,
    view_id: u64,
    mention_selected: usize,
    mention_dismissed: Option<MentionRequest>,
    mention_result: Option<(MentionRequest, FileSuggestionsSnapshot)>,
    mention_owner_epoch: Option<u64>,
    /// Model picker (present while the Model panel lives).
    pub(crate) picker: Option<ModelPicker>,
    catalog_loaded: bool,
    /// Agent profiles from the catalog snapshot.
    pub(crate) agents: Vec<AgentEntry>,
    /// Effective agent id from the catalog snapshot (the metadata row shows
    /// the active agent, never the picker cursor).
    active_agent: Option<String>,
    /// Agents cursor.
    pub(crate) agents_cursor: usize,
    /// Session ids for the Sessions panel.
    pub(crate) sessions: Vec<String>,
    session_entries: Vec<oc_core::queries::SessionListEntry>,
    sessions_all_projects: bool,
    /// Sessions cursor.
    pub(crate) sessions_cursor: usize,
    sessions_loaded: bool,
    /// Skill catalog cards (metadata only, no bodies).
    pub(crate) skills: Vec<SkillCard>,
    /// Skills cursor.
    pub(crate) skills_cursor: usize,
    skills_loaded: bool,
    /// DCP panel state: snapshot in, request out, transient outcome (UI04).
    pub(crate) dcp: DcpPanelState,
    /// Command ids known to the application (templates stay there).
    pub(crate) commands: Vec<String>,
    /// Metadata for the current catalog generation only.
    command_descriptions: BTreeMap<String, String>,
    /// Newest tool cards from the runtime (bounded page).
    pub(crate) cards: Vec<HistoryRow>,
    card_ops: Vec<String>,
    /// Cards cursor.
    pub(crate) cards_cursor: usize,
    pub(crate) card_output: Option<CardOutput>,
    card_scroll: usize,
    card_seen: std::cell::Cell<usize>,
    detail_area: std::cell::Cell<ratatui::layout::Rect>,
    cards_loaded: bool,
    cards_has_older: bool,
}

impl TuiState {
    /// Bind to a session; the session must already exist on the handle.
    pub fn new(app: CoreApp, session: SessionId) -> Self {
        Self::with_session(app, Some(session))
    }

    /// Start a Home composer without creating or retaining a session ID.
    pub fn new_home(app: CoreApp) -> Self {
        Self::with_session(app, None)
    }

    fn with_session(app: CoreApp, session: Option<SessionId>) -> Self {
        let index = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |now| now.subsec_nanos() as usize % HOME_EXAMPLES.len());
        let home = session.is_none();
        Self {
            chrome: Default::default(),
            parent_id: None,
            home,
            home_example: HOME_EXAMPLES[index],
            viewport: std::cell::Cell::new(None),
            completion_anchor: std::cell::RefCell::new(None),
            painted_transcript: std::cell::RefCell::new(None),
            painted_prompt: std::cell::RefCell::new(None),
            prompt_width: std::cell::Cell::new(None),
            paint_generation: std::cell::Cell::new(0),
            message_down: None,
            clipboard_mode: ClipboardMode::default(),
            conversation_available: None,
            reverted: None,
            reverted_down: None,
            conversation_bindings: [None, None],
            leader_key: "ctrl+x".into(),
            owner_clipboard_mode: None,
            pending_copy: None,
            selection: None,
            selection_gesture: false,
            click: None,
            session_title: None,
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            approvals: Default::default(),
            approval_roots: Default::default(),
            tab_attention: Default::default(),
            app,
            session,
            status: TuiStatus::Idle,
            scanner_frame: 0,
            compactions: Vec::new(),
            compaction_at: None,
            compaction_frame: 0,
            compaction_turn_messages: Default::default(),
            scanner_at: None,
            panel: TuiPanel::None,
            select: Default::default(),
            mouse_down: None,
            tab_down: None,
            tab_view: Default::default(),
            tab_scroll: std::cell::Cell::new(0),
            last_mouse: None,
            close_hold: None,
            tabs: Vec::new(),
            active_tab: 0,
            can_add_tab: false,
            exploration_expanded: BTreeSet::new(),
            exploration_down: None,
            reasoning_expanded: BTreeSet::new(),
            reasoning_down: None,
            reasoning_pointer_down: false,
            reasoning_epoch: 0,
            live_part_offset: 0,
            leader: None,
            input: String::new(),
            editor: Default::default(),
            rename_input: String::new(),
            rename_editor: Default::default(),
            rename_pending: None,
            rename_selected: None,
            session_delete_confirm: None,
            session_project_name: None,
            session_scope_pending: None,
            rename_direct_pending: None,
            regenerate_pending: None,
            window: HistoryWindow::new(),
            markdown_cache: std::cell::RefCell::new(Default::default()),
            transcript_revision: 0,
            visible_projection: Default::default(),
            #[cfg(test)]
            transcript_row_copies: Default::default(),
            #[cfg(test)]
            visible_projection_builds: Default::default(),
            live_text: String::new(),
            live_reasoning: String::new(),
            thinking_expanded: false,
            live_parts: Vec::new(),
            pending_tool_seen: Vec::new(),
            pending_tool_round: 0,
            live_part_states: Vec::new(),
            live_agent_color_index: None,
            live_terminal_status: None,
            live_model_label: None,
            live_preview_truncated: false,
            reasoning_started: None,
            reasoning_finished: None,
            turn_usage: None,
            scroll: 0,
            wheel_motion: None,
            note: None,
            toast_expiry: None,
            toast_down: false,
            active_turn: None,
            interrupt_armed_until: None,
            pending: None,
            compress_turn: None,
            request_id: 0,
            generation: 0,
            input_revision: 0,
            slash_selected: 0,
            slash_dismissed: None,
            view_id: NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            mention_selected: 0,
            mention_dismissed: None,
            mention_result: None,
            mention_owner_epoch: None,
            picker: None,
            catalog_loaded: false,
            agents: Vec::new(),
            active_agent: None,
            agents_cursor: 0,
            sessions: Vec::new(),
            session_entries: Vec::new(),
            sessions_all_projects: true,
            sessions_cursor: 0,
            sessions_loaded: false,
            skills: Vec::new(),
            skills_cursor: 0,
            skills_loaded: false,
            dcp: DcpPanelState::default(),
            commands: Vec::new(),
            command_descriptions: BTreeMap::new(),
            cards: Vec::new(),
            card_ops: Vec::new(),
            cards_cursor: 0,
            card_output: None,
            card_scroll: 0,
            card_seen: std::cell::Cell::new(0),
            detail_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
            cards_loaded: false,
            cards_has_older: false,
        }
    }

    /// Attached session ID, if this view has an accepted or resumed session.
    pub fn attached_session(&self) -> Option<&SessionId> {
        self.session.as_ref()
    }

    /// Attached session ID. Only call after checking `attached_session()`.
    pub fn session(&self) -> &SessionId {
        self.attached_session()
            .expect("Home has no attached session")
    }

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

    fn set_tab_strip_at(
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

    pub(crate) fn set_detail_area(&self, area: ratatui::layout::Rect) {
        self.detail_area.set(area);
    }

    pub(crate) fn detail_area(&self) -> ratatui::layout::Rect {
        self.detail_area.get()
    }

    pub(crate) fn card_scroll(&self) -> usize {
        self.card_scroll
    }

    pub(crate) fn card_seen(&self) -> usize {
        self.card_seen.get()
    }

    /// Only rows actually painted in a frame count as accessible. An End key
    /// or repeated key events without drawing cannot skip a result window.
    pub(crate) fn card_rows_painted(&self, start: usize, end: usize) {
        if start <= self.card_seen.get() {
            self.card_seen.set(self.card_seen.get().max(end));
        }
    }

    /// Switch to another session after an accepted switch: clears view state
    /// and the history window; status returns to `Idle` unless quitting.
    /// Drop every generation-bound cache after a Location switch.
    ///
    /// Panel data (catalog, agents, sessions, skills, cards, DCP snapshot and
    /// workspace commands) belongs to the previous Location: the next panel
    /// open must reload from the new generation instead of showing it.
    pub fn reset_workspace(&mut self) {
        self.invalidate_transcript();
        self.close_panel();
        self.editor.forget_accepted_mentions();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.clear_mentions();
        self.tabs.clear();
        *self.tab_view.get_mut() = TabView::default();
        self.tab_scroll.set(0);
        self.tab_attention.clear();
        self.clear_mouse_position();
        self.active_tab = 0;
        self.can_add_tab = false;
        self.exploration_expanded.clear();
        self.reasoning_expanded.clear();
        self.reasoning_down = None;
        self.chrome = Default::default();
        self.parent_id = None;
        self.auto_accept = oc_core::queries::AutoAcceptState::Unsupported;
        self.session_title = None;
        self.generation += 1;
        self.invalidate_submission();
        self.picker = None;
        self.catalog_loaded = false;
        self.agents.clear();
        self.active_agent = None;
        self.agents_cursor = 0;
        self.sessions.clear();
        self.sessions_cursor = 0;
        self.sessions_loaded = false;
        self.skills.clear();
        self.skills_cursor = 0;
        self.skills_loaded = false;
        self.commands.clear();
        self.command_descriptions.clear();
        self.cards.clear();
        self.card_ops.clear();
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.detail_area.set(ratatui::layout::Rect::default());
        self.cards_cursor = 0;
        self.cards_loaded = false;
        self.cards_has_older = false;
        self.dcp = DcpPanelState::default();
    }

    pub fn set_session(&mut self, session: SessionId) {
        self.invalidate_transcript();
        *self.tab_view.get_mut() = TabView::default();
        self.clear_mouse_position();
        self.compactions.clear();
        self.compaction_turn_messages.clear();
        self.compaction_at = None;
        self.compaction_frame = 0;
        self.completion_anchor.get_mut().take();
        self.conversation_available = None;
        self.reverted = None;
        self.close_panel();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.clear_mentions();
        self.exploration_expanded.clear();
        self.reasoning_expanded.clear();
        self.reasoning_down = None;
        self.cards.clear();
        self.card_ops.clear();
        self.cards_cursor = 0;
        self.cards_loaded = false;
        self.cards_has_older = false;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.detail_area.set(ratatui::layout::Rect::default());
        self.viewport.set(None);
        self.parent_id = None;
        self.home = false;
        self.session_title = None;
        self.generation += 1;
        self.invalidate_submission();
        self.session = Some(session);
        self.input.clear();
        self.editor.clear();
        self.window = HistoryWindow::new();
        *self.markdown_cache.get_mut() = Default::default();
        self.live_text.clear();
        self.live_reasoning.clear();
        self.live_parts.clear();
        self.pending_tool_seen.clear();
        self.pending_tool_round = 0;
        self.live_part_offset = 0;
        self.reasoning_down = None;
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.scroll = 0;
        self.wheel_motion = None;
        self.active_turn = None;
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
    }

    fn invalidate_submission(&mut self) {
        self.reset_scanner();
        self.live_part_states.clear();
        self.live_agent_color_index = None;
        self.live_terminal_status = None;
        self.live_model_label = None;
        self.live_preview_truncated = false;
        // Called only after an accepted switch (binary refuses busy switches).
        // Invalidate local receipts even if a caller has an old completion queued.
        self.pending = None;
        self.active_turn = None;
        self.compress_turn = None;
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
    }

    /// Current status.
    pub fn status(&self) -> &TuiStatus {
        &self.status
    }

    /// Owner receipts need reconciliation until accepted, independently from
    /// streaming and animation. A settled view has no receipt polling work.
    pub fn has_pending_submission(&self) -> bool {
        self.pending.is_some()
    }

    /// Active view deadlines only; input/provider paints never reset these
    /// clocks. Hover-paused toasts and settled wheel motion have no deadline.
    pub fn next_ui_deadline(&self) -> Option<Instant> {
        let scanner = (self.status == TuiStatus::Streaming
            && self.chrome.animations != Some(false))
        .then(|| {
            self.scanner_at
                .map(|at| at + Duration::from_millis(crate::scanner::FRAME_MS))
        })
        .flatten();
        let toast = self
            .toast_expiry
            .as_ref()
            .and_then(|expiry| expiry.started.map(|at| at + expiry.remaining));
        [
            scanner,
            self.compaction_at
                .filter(|_| self.chrome.animations != Some(false))
                .map(|at| at + Duration::from_millis(80)),
            toast,
            self.interrupt_armed_until,
            self.next_scroll_animation_deadline(),
            self.leader_deadline(),
            self.next_tab_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// True only when a deadline actually changes visible state.
    pub fn tick_ui(&mut self, now: Instant) -> bool {
        let expired = self.interrupt_armed_until.is_some_and(|until| now >= until)
            || self.toast_expiry.as_ref().is_some_and(|expiry| {
                expiry
                    .started
                    .is_some_and(|at| now.saturating_duration_since(at) >= expiry.remaining)
            });
        self.tick_toast(now);
        let scanner = self.tick_scanner(now);
        let wheel = self.tick_scroll_animation(now);
        let compaction = self.tick_compaction(now);
        let tabs = self.tick_tabs(now);
        let leader = self.leader_deadline().is_some_and(|until| now >= until);
        if leader {
            self.leader = None;
        }
        expired || scanner || wheel || compaction || leader || tabs
    }

    fn leader_deadline(&self) -> Option<Instant> {
        self.leader
            .and_then(|at| at.checked_add(Duration::from_millis(self.chrome.leader_timeout_ms())))
    }

    /// The composer and key resolver observe the same pending sequence.
    pub fn leader_pending(&self) -> bool {
        self.leader.is_some()
    }

    fn reset_scanner(&mut self) {
        self.scanner_frame = 0;
        self.scanner_at = None;
        self.interrupt_armed_until = None;
    }

    pub(crate) fn interrupt_armed(&self) -> bool {
        self.interrupt_armed_until
            .is_some_and(|until| Instant::now() < until)
    }

    /// Advance the running prompt scanner from a caller-supplied monotonic clock.
    /// The event loop calls this alongside its other view ticks, per view. Returns
    /// whether a frame changed and a redraw is useful; the first tick starts at 0.
    pub fn tick_scanner(&mut self, now: Instant) -> bool {
        if self.status != TuiStatus::Streaming || self.chrome.animations == Some(false) {
            return false;
        }
        let Some(at) = self.scanner_at else {
            self.scanner_at = Some(now);
            return false;
        };
        let elapsed = now.saturating_duration_since(at).as_millis();
        let steps = elapsed / u128::from(crate::scanner::FRAME_MS);
        if steps == 0 {
            return false;
        }
        let previous = self.scanner_frame;
        self.scanner_frame = (self.scanner_frame
            + (steps % crate::scanner::FRAMES as u128) as usize)
            % crate::scanner::FRAMES;
        self.scanner_at = now.checked_sub(Duration::from_millis(
            (elapsed % u128::from(crate::scanner::FRAME_MS)) as u64,
        ));
        self.scanner_frame != previous
    }

    pub(crate) fn scanner_frame(&self) -> usize {
        self.scanner_frame
    }

    /// Open panel, if any.
    pub fn panel(&self) -> &TuiPanel {
        &self.panel
    }

    /// Safe options from actual snapshots. Filtering never changes runtime selection.
    pub fn modal_options(&self) -> std::rc::Rc<Vec<crate::dialog::SelectOption>> {
        use crate::dialog::SelectOption;
        let item =
            |value: String, title: String, category: &str, footer: String, current| SelectOption {
                value,
                title,
                category: category.into(),
                footer,
                current,
                running: false,
                destructive: false,
            };
        let options = match &self.panel {
            TuiPanel::MessageActions { .. } => [
                ("jump", "Jump to", "view message in session"),
                ("revert", "Revert", "undo messages and restore prompt"),
                ("copy", "Copy", "message text to clipboard"),
                ("fork", "Fork", "create a new session"),
            ]
            .into_iter()
            .map(|(value, title, description)| {
                item(value.into(), title.into(), "", description.into(), false)
            })
            .collect(),
            TuiPanel::Commands => {
                let mut options = Vec::new();
                // Use the last rendered terminal width and the same rail allocation
                // and auto breakpoint as shell::shell_regions/session_main.
                let sidebar_visible = !self.home
                    && self.parent_id.is_none()
                    && !self.chrome.sidebar_hidden
                    && self.viewport.get().is_some_and(|view| {
                        let session = crate::layout::configured_shell_regions(
                            Rect::new(0, 0, view.terminal_width, view.height),
                            self.chrome.devtools_visible(),
                            self.chrome.vertical_tabs_width,
                        )
                        .session;
                        crate::layout::sidebar_auto(session.width)
                    });
                if self.select.query.is_empty() {
                    options.extend(
                        crate::commands::REGISTRY
                            .iter()
                            .filter(|c| {
                                c.id == "model.list"
                                    || ((c.id == "session.list" || c.id == "session.new")
                                        && !self.home)
                            })
                            .map(|c| {
                                item(
                                    c.id.into(),
                                    c.title.into(),
                                    "Suggested",
                                    self.command_footer(c),
                                    false,
                                )
                            }),
                    );
                }
                options.extend(
                    crate::commands::REGISTRY
                        .iter()
                        .filter(|c| {
                            c.in_palette(self.picker.as_ref().is_some_and(|p| p.has_variants()))
                                && (c.action != CommandAction::CloseTab || !self.tabs.is_empty())
                                && (!matches!(c.action, CommandAction::RenameSession { .. })
                                    || self.command_unavailable(&c.action).is_none())
                        })
                        .map(|c| {
                            item(
                                c.id.into(),
                                if c.id == "session.toggle.thinking" && self.thinking_expanded {
                                    "Collapse thinking".into()
                                } else if c.id == "session.sidebar.toggle" {
                                    if sidebar_visible {
                                        "Hide sidebar".into()
                                    } else {
                                        "Show sidebar".into()
                                    }
                                } else {
                                    c.title.into()
                                },
                                c.group,
                                self.command_footer(c),
                                false,
                            )
                        }),
                );
                options
            }
            TuiPanel::Model => {
                return self.select.filter_for(
                    self.picker
                        .as_ref()
                        .map(|p| p.options())
                        .unwrap_or_default(),
                    &self.panel,
                );
            }
            TuiPanel::Settings => vec![item(
                "permissions".into(),
                "Permissions".into(),
                "Session",
                if self.chrome.permissions_auto {
                    "auto accept"
                } else {
                    "prompt"
                }
                .into(),
                false,
            )],
            TuiPanel::Variant => self
                .picker
                .as_ref()
                .map(|p| p.variant_options())
                .unwrap_or_default(),
            TuiPanel::Agents => self
                .agents
                .iter()
                .map(|a| {
                    item(
                        a.id.clone(),
                        a.id.clone(),
                        "Agents",
                        a.description.clone(),
                        self.active_agent.as_ref() == Some(&a.id),
                    )
                })
                .collect(),
            TuiPanel::Sessions => self
                .sessions
                .iter()
                .map(|s| {
                    let entry = self.session_entries.iter().find(|entry| &entry.id.0 == s);
                    let mut option = item(
                        s.clone(),
                        entry.map_or_else(
                            || "New session — metadata unavailable".into(),
                            |entry| entry.title.clone(),
                        ),
                        entry.map_or("Update time unavailable", |entry| &entry.date_group),
                        entry
                            .and_then(|entry| entry.worktree.clone())
                            .unwrap_or_default(),
                        self.attached_session().is_some_and(|id| s == &id.0),
                    );
                    if self.session_delete_confirm.as_deref() == Some(s) {
                        option.title = "Press ctrl+d again to confirm".into();
                        option.destructive = true;
                    }
                    option.running = entry.is_some_and(|entry| entry.running);
                    option
                })
                .collect(),
            TuiPanel::Skills => self
                .skills
                .iter()
                .map(|s| {
                    item(
                        s.id.clone(),
                        format!("{} — {}", s.id, s.name),
                        "Skills",
                        s.description.clone(),
                        false,
                    )
                })
                .collect(),
            TuiPanel::Rename => Vec::new(),
            TuiPanel::None => Vec::new(),
            _ => crate::views::panel_lines(self)
                .into_iter()
                .enumerate()
                .map(|(i, s)| item(i.to_string(), s, "", String::new(), false))
                .collect(),
        };
        self.select
            .filter_for(std::rc::Rc::new(options), &self.panel)
    }

    pub fn command_unavailable(&self, action: &CommandAction) -> Option<&'static str> {
        if matches!(
            action,
            CommandAction::UndoConversation | CommandAction::RedoConversation
        ) {
            if self.session.is_none() {
                return Some("no session yet");
            }
            if self.parent_id.is_some() {
                return Some("child session is read-only");
            }
            if self.status == TuiStatus::PendingSubmission {
                return Some("submission pending");
            }
            if let Some((undo, redo)) = self.conversation_available {
                if *action == CommandAction::UndoConversation && !undo {
                    return Some("nothing to undo");
                }
                if *action == CommandAction::RedoConversation && !redo {
                    return Some("nothing to redo");
                }
            }
        }
        if *action == CommandAction::CloseTab {
            let (tabs, index, _) = self.tab_presentation();
            if tabs.is_empty() || (index >= tabs.len() && !self.home) {
                return Some("no tab to close");
            }
            if tabs.get(index).is_some_and(|tab| tab.busy) {
                return Some("tab busy; action unavailable");
            }
        }
        if matches!(action, CommandAction::RenameSession { .. }) {
            if self.home || self.session.is_none() {
                return Some("no session yet");
            }
            if self.parent_id.is_some() {
                return Some("child session is read-only");
            }
            if self.tabs.get(self.active_tab).is_some_and(|tab| tab.busy) {
                return Some("tab busy; action unavailable");
            }
        }
        if self.session.is_none()
            && matches!(
                action,
                CommandAction::OpenCards
                    | CommandAction::DcpCompress { .. }
                    | CommandAction::CompactSession
            )
        {
            return Some("no session yet");
        }
        crate::commands::spec(action).unavailable(
            self.is_busy(),
            self.picker.as_ref().is_some_and(|p| p.has_variants()),
        )
    }

    fn command_footer(&self, command: &crate::commands::CommandSpec) -> String {
        self.command_unavailable(&command.action)
            .map(str::to_string)
            .unwrap_or_else(|| match command.action {
                CommandAction::UndoConversation => self.conversation_shortcut(true),
                CommandAction::RedoConversation => self.conversation_shortcut(false),
                _ => self.command_shortcuts(command).join(" "),
            })
    }

    /// The Commands hints and builtin chord resolver use the same effective
    /// leader projection. Direct keys and explicit owner overrides stay literal.
    fn command_shortcuts(&self, command: &crate::commands::CommandSpec) -> Vec<String> {
        let override_binding = match command.action {
            CommandAction::UndoConversation => Some(self.conversation_shortcut(true)),
            CommandAction::RedoConversation => Some(self.conversation_shortcut(false)),
            CommandAction::OpenCommands => self.chrome.command_palette_shortcut.clone(),
            _ => None,
        };
        if let Some(binding) = override_binding {
            return binding
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty() && !key.eq_ignore_ascii_case("none"))
                .map(str::to_owned)
                .collect();
        }
        command
            .shortcuts
            .iter()
            .flat_map(|shortcut| {
                if let Some(suffix) = shortcut.strip_prefix("ctrl+x ") {
                    self.chrome
                        .conversation_shortcuts
                        .leader
                        .split(',')
                        .map(str::trim)
                        .filter(|leader| !leader.is_empty() && !leader.eq_ignore_ascii_case("none"))
                        .map(|leader| format!("{leader} {suffix}"))
                        .collect::<Vec<_>>()
                } else {
                    vec![(*shortcut).to_string()]
                }
            })
            .collect()
    }

    /// Effective owner-configured shortcuts; Some("") disables a binding.
    pub fn set_conversation_shortcuts(&mut self, undo: Option<String>, redo: Option<String>) {
        let previous = [
            self.conversation_shortcut(true),
            self.conversation_shortcut(false),
        ];
        self.conversation_bindings = [undo, redo];
        if previous
            != [
                self.conversation_shortcut(true),
                self.conversation_shortcut(false),
            ]
        {
            self.leader = None;
            self.clear_transcript_selection();
        }
    }

    fn conversation_shortcut(&self, undo: bool) -> String {
        self.conversation_bindings[usize::from(!undo)]
            .clone()
            .unwrap_or_else(|| {
                crate::commands::spec(&if undo {
                    CommandAction::UndoConversation
                } else {
                    CommandAction::RedoConversation
                })
                .shortcuts
                .join(" ")
            })
    }

    /// Resolve configured direct keys before the generic editor mapping.
    /// Modal focus keeps ownership; leader sequences are resolved in handle_key.
    pub fn conversation_key(&mut self, event: crossterm::event::KeyEvent) -> Option<KeyAction> {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if event.kind != KeyEventKind::Press {
            return None;
        }
        let key = match event.code {
            KeyCode::Char(key) => key.to_ascii_lowercase().to_string(),
            KeyCode::Enter => "enter".into(),
            KeyCode::Esc => "esc".into(),
            KeyCode::Tab | KeyCode::BackTab => "tab".into(),
            KeyCode::Backspace => "backspace".into(),
            KeyCode::Delete => "delete".into(),
            KeyCode::Insert => "insert".into(),
            KeyCode::Home => "home".into(),
            KeyCode::End => "end".into(),
            KeyCode::PageUp => "pageup".into(),
            KeyCode::PageDown => "pagedown".into(),
            KeyCode::Left => "left".into(),
            KeyCode::Right => "right".into(),
            KeyCode::Up => "up".into(),
            KeyCode::Down => "down".into(),
            KeyCode::F(number) => format!("f{number}"),
            _ => return None,
        };
        let mut binding = String::new();
        for (flag, prefix) in [
            (KeyModifiers::CONTROL, "ctrl+"),
            (KeyModifiers::ALT, "alt+"),
            (KeyModifiers::SHIFT, "shift+"),
        ] {
            if event.modifiers.contains(flag) {
                binding.push_str(prefix);
            }
        }
        binding.push_str(&key);
        if self
            .leader_deadline()
            .is_some_and(|until| Instant::now() >= until)
        {
            self.leader = None;
        }
        let chord = self
            .leader
            .map(|_| format!("{} {binding}", self.leader_key));
        let action = [true, false]
            .into_iter()
            .find(|undo| {
                self.panel == TuiPanel::None
                    && self.conversation_shortcut(*undo).split(',').any(|value| {
                        value
                            .trim()
                            .eq_ignore_ascii_case(chord.as_deref().unwrap_or(&binding))
                    })
            })
            .map(|undo| {
                if undo {
                    KeyAction::UndoConversation
                } else {
                    KeyAction::RedoConversation
                }
            });
        if action.is_some() {
            self.leader = None;
            return action;
        }
        if self.panel == TuiPanel::None
            && self
                .chrome
                .command_palette_shortcut
                .as_ref()
                .is_some_and(|shortcut| {
                    shortcut.split(',').any(|value| {
                        value
                            .trim()
                            .eq_ignore_ascii_case(chord.as_deref().unwrap_or(&binding))
                    })
                })
        {
            self.leader = None;
            return Some(KeyAction::Commands);
        }
        if self.leader.is_some() {
            let printable = match event.code {
                KeyCode::Char(value)
                    if !event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(value)
                }
                _ => None,
            };
            return Some(KeyAction::SequenceKey(binding, printable));
        }
        if self
            .chrome
            .conversation_shortcuts
            .leader
            .split(',')
            .any(|value| value.trim().eq_ignore_ascii_case(&binding))
            || [true, false].into_iter().any(|undo| {
                self.conversation_shortcut(undo).split(',').any(|value| {
                    value
                        .trim()
                        .split_once(' ')
                        .is_some_and(|(prefix, _)| prefix.eq_ignore_ascii_case(&binding))
                })
            })
        {
            self.leader_key = binding;
            return Some(KeyAction::Leader);
        }
        None
    }

    pub fn terminal_key(&mut self, event: crossterm::event::KeyEvent) -> Option<KeyAction> {
        if self.approvals.active().is_some() {
            return self
                .approvals
                .terminal_key(event, &self.chrome.permission_shortcuts);
        }
        self.conversation_key(event).or_else(|| {
            crate::events::map_key(event).filter(|action| {
                *action != KeyAction::Leader
                    && !(*action == KeyAction::Commands
                        && self.panel == TuiPanel::None
                        && self.chrome.command_palette_shortcut.is_some())
            })
        })
    }

    /// Called only after a successful application selection. The original applies
    /// the model before replacing its dialog; Escape here never rolls it back.
    pub fn model_choice_applied(&mut self, snapshot: CatalogSnapshot) {
        let selecting_model = self.panel == TuiPanel::Model;
        self.apply_catalog(snapshot);
        if selecting_model
            && self.picker.as_ref().is_some_and(|p| {
                p.has_variants() && p.selection().is_some_and(|s| s.variant.is_none())
            })
        {
            self.open_variants();
        } else {
            self.close_panel();
        }
    }

    /// Owner ACK returns Settings to its unfiltered main list.
    pub fn permission_mode_applied(&mut self) {
        if self.panel == TuiPanel::Settings {
            self.select.reset();
        }
    }

    fn open_variants(&mut self) {
        self.panel = TuiPanel::Variant;
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        self.last_mouse = None;
        // A press belongs to the dialog where it began, not the replacement.
        self.mouse_down = None;
        self.select.reset();
        self.toast_down = false;
        self.select.cursor = self
            .modal_options()
            .iter()
            .position(|o| o.current)
            .unwrap_or(0);
    }

    fn changed_modal_query(&mut self) {
        self.session_delete_confirm = None;
        self.select.changed_query();
        if self.panel == TuiPanel::Variant && self.select.query.is_empty() {
            self.select.cursor = self
                .modal_options()
                .iter()
                .position(|o| o.current)
                .unwrap_or(0);
        }
        self.sync_modal_cursor();
    }

    fn sync_modal_cursor(&mut self) {
        let options = self.modal_options();
        self.select.cursor = self.select.cursor.min(options.len().saturating_sub(1));
        let Some(option) = options.get(self.select.cursor) else {
            return;
        };
        match self.panel {
            TuiPanel::Model => {
                if let Some(p) = &mut self.picker {
                    p.focus_id(&option.value);
                }
            }
            TuiPanel::Agents => {
                self.agents_cursor = self
                    .agents
                    .iter()
                    .position(|a| a.id == option.value)
                    .unwrap_or(0)
            }
            TuiPanel::Sessions => {
                self.sessions_cursor = self
                    .sessions
                    .iter()
                    .position(|s| s == &option.value)
                    .unwrap_or(0)
            }
            TuiPanel::Skills => {
                self.skills_cursor = self
                    .skills
                    .iter()
                    .position(|s| s.id == option.value)
                    .unwrap_or(0)
            }
            _ => {}
        }
    }

    /// Current input buffer.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Prompt layout and the insertion caret share the same grapheme/cell model.
    pub fn prompt_layout(&self, width: usize) -> (Vec<crate::editor::PromptRow>, (usize, usize)) {
        self.editor.layout(&self.input, width)
    }

    pub(crate) fn clear_prompt_paint(&self) {
        self.painted_prompt.borrow_mut().take();
    }

    pub(crate) fn observe_prompt_paint(
        &self,
        frame: Rect,
        input: Rect,
        top: usize,
        rows: &[crate::editor::PromptRow],
    ) {
        self.prompt_width.set(Some(input.width as usize));
        if self.panel != TuiPanel::None || self.approvals.active().is_some() {
            return;
        }
        let toast = crate::shell::toast_rect(self, frame);
        let mut chips = Vec::new();
        for (index, row) in rows
            .iter()
            .skip(top)
            .take(input.height as usize)
            .enumerate()
        {
            for &(from, to, start) in &row.chip_hits {
                // Clip to both the viewport and final toast overpaint. The map
                // contains painted intervals only, never virtual separator cells.
                let mut run = None;
                let y = input.y + index as u16;
                for column in from..to.min(input.width as usize) {
                    let x = input.x + column as u16;
                    if toast.is_some_and(|rect| rect.contains((x, y).into())) {
                        if let Some(a) = run.take() {
                            chips.push((Rect::new(a, y, x - a, 1), start));
                        }
                    } else {
                        run.get_or_insert(x);
                    }
                }
                if let Some(a) = run {
                    chips.push((
                        Rect::new(a, y, input.x + to.min(input.width as usize) as u16 - a, 1),
                        start,
                    ));
                }
            }
        }
        *self.painted_prompt.borrow_mut() = Some(PaintedPrompt {
            frame,
            main: crate::shell::prompt_main(self, frame),
            home: self.home,
            session: self.session.clone(),
            revision: self.input_revision,
            generation: self.generation,
            cursor: self.editor.cursor,
            anchor: self.editor.anchor,
            chips,
        });
    }

    fn prompt_vertical(&mut self, down: bool, select: bool) -> bool {
        // Keyboard input uses the last painted width even after draft edits;
        // after a resize, the next paint establishes the new width.
        let width = self.prompt_width.get().unwrap_or(usize::MAX);
        let edge = if down { self.input.len() } else { 0 };
        if !select && self.editor.cursor == edge {
            return false;
        }
        if self
            .editor
            .vertical_wrapped(&self.input, width, down, select)
        {
            return true;
        }
        // At a visual edge, the donor first moves to the absolute raw edge;
        // only the next unselected arrow is owned by prompt history.
        if !select && self.editor.cursor != edge {
            self.editor.move_to(edge, false);
            return true;
        }
        false
    }

    /// Only the focused prompt, not a dialog or a dismissed revision, owns the overlay.
    pub(crate) fn slash_options(&self) -> Option<Vec<crate::autocomplete::SlashOption>> {
        if self.panel != TuiPanel::None || self.slash_dismissed == Some(self.input_revision) {
            return None;
        }
        let filter = crate::autocomplete::query(&self.input, self.editor.cursor)?;
        // Home does not register the session-only rename action. Inventory
        // padding must be measured after that route exclusion, before search.
        Some(crate::autocomplete::options(
            filter,
            &self.commands,
            &self.command_descriptions,
            self.home,
        ))
    }

    /// Keep selection and activation aligned when caret movement or a catalog
    /// refresh shrinks the filtered list without an intervening text edit.
    pub(crate) fn slash_selected(&self, count: usize) -> usize {
        self.slash_selected.min(count.saturating_sub(1))
    }

    fn clear_mentions(&mut self) {
        self.mention_result = None;
        self.mention_dismissed = None;
        self.mention_owner_epoch = None;
        self.mention_selected = 0;
    }

    /// A parked view retains its draft, but its filesystem snapshot is no
    /// longer current when the route becomes active again.
    pub fn invalidate_file_suggestions(&mut self) {
        self.generation += 1;
        self.clear_mentions();
    }

    /// A successful owner reload invalidates Location-generation UI snapshots
    /// even when the canonical path and prompt text did not change.
    pub fn refresh_configuration(&mut self, catalog: CatalogSnapshot) {
        self.close_panel();
        self.invalidate_file_suggestions();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.sessions.clear();
        self.sessions_loaded = false;
        self.skills.clear();
        self.skills_loaded = false;
        self.dcp = DcpPanelState::default();
        self.apply_catalog(catalog);
    }

    /// No storage or filesystem access: the binary asks the owner after the
    /// input burst, then delivers the bounded snapshot using this exact key.
    pub fn mention_request(&self) -> Option<MentionRequest> {
        if self.panel != TuiPanel::None || self.editor.selected().is_some() {
            return None;
        }
        let location = self.chrome.location.as_ref()?.clone();
        let (start, query) = crate::autocomplete::mention(&self.input, self.editor.cursor)?;
        let request = MentionRequest {
            query: query.into(),
            location,
            view_id: self.view_id,
            generation: self.generation,
            revision: self.input_revision,
            caret: self.editor.cursor,
            start,
        };
        (self.mention_dismissed.as_ref() != Some(&request)).then_some(request)
    }

    /// Reject late results from another edit, caret, route, or owner epoch.
    pub fn apply_file_suggestions(
        &mut self,
        request: MentionRequest,
        result: FileSuggestionsSnapshot,
    ) -> bool {
        if self.mention_request().as_ref() != Some(&request)
            || self.chrome.location.as_deref() != Some(result.location.as_str())
            || self
                .mention_owner_epoch
                .is_some_and(|epoch| epoch != result.generation)
        {
            return false;
        }
        self.mention_owner_epoch = Some(result.generation);
        self.mention_selected = 0;
        self.mention_result = Some((request, result));
        true
    }

    /// A zero-match snapshot is loaded too; do not query again each frame.
    pub fn mention_loaded(&self, request: &MentionRequest) -> bool {
        self.mention_result
            .as_ref()
            .is_some_and(|(key, _)| key == request)
    }

    pub(crate) fn mention_options(&self) -> Option<&FileSuggestionsSnapshot> {
        let request = self.mention_request()?;
        self.mention_result
            .as_ref()
            .filter(|(key, snapshot)| {
                *key == request
                    && snapshot.location == request.location
                    && self.mention_owner_epoch == Some(snapshot.generation)
            })
            .map(|(_, snapshot)| snapshot)
    }

    pub(crate) fn mention_selected(&self, count: usize) -> usize {
        self.mention_selected.min(count.saturating_sub(1))
    }

    fn select_mention(&mut self) {
        let Some((start, caret, path)) = self.mention_options().and_then(|options| {
            let path = options
                .paths
                .get(self.mention_selected(options.paths.len()))?;
            Some((
                self.mention_request()?.start,
                self.editor.cursor,
                path.clone(),
            ))
        }) else {
            return;
        };
        // An owner path is Location-relative. It is inserted as ordinary text;
        // no structured part or implicit read is created.
        if path.starts_with('/') || path.split('/').any(|part| part == "..") {
            return;
        }
        // Pinned autocomplete.tsx:164-175 inserts a separator at the caret,
        // except when the following text already supplies whitespace.
        let separator = self
            .input
            .get(caret..)
            .and_then(|after| after.chars().next())
            .is_some_and(char::is_whitespace);
        let replacement = format!("@{path}{}", if separator { "" } else { " " });
        if self.input.len() - (caret - start) + replacement.len() > MAX_INPUT_BYTES {
            return;
        }
        self.editor.move_to(start, false);
        if self.editor.cursor != start {
            // A pasted chip is an atomic editor range; never expand a mention
            // selection across hidden pasted text.
            self.editor.move_to(caret, false);
            return;
        }
        self.editor.move_to(caret, true);
        if self.editor.selected() != Some((start, caret)) {
            self.editor.move_to(caret, false);
            return;
        }
        if self
            .editor
            .replace(&mut self.input, &replacement, MAX_INPUT_BYTES)
            > 0
        {
            self.editor
                .mark_file_mention(start, start + 1 + path.len(), &self.input);
            self.input_revision += 1;
            self.mention_selected = 0;
            self.mention_result = None;
            self.mention_dismissed = self.mention_request();
        }
    }

    fn replace_slash(&mut self, name: &str, trailing_space: bool) {
        let cursor = self.editor.cursor;
        self.editor.move_to(0, false);
        self.editor.move_to(cursor, true);
        let replacement = format!("/{name}{}", if trailing_space { " " } else { "" });
        if self
            .editor
            .replace(&mut self.input, &replacement, MAX_INPUT_BYTES)
            > 0
        {
            self.input_revision += 1;
        }
        self.slash_selected = 0;
        self.slash_dismissed = Some(self.input_revision);
    }

    async fn select_slash(&mut self, enter: bool) -> KeyOutcome {
        let Some(options) = self.slash_options() else {
            return KeyOutcome::default();
        };
        let selected = self.slash_selected(options.len());
        let Some(option) = options.into_iter().nth(selected) else {
            return KeyOutcome::default();
        };
        if (!enter && option.action != Some(CommandAction::ReloadConfiguration))
            || option.arguments
            || option.action.is_none()
        {
            self.replace_slash(&option.name, true);
            return KeyOutcome::default();
        }
        let action = option.action.expect("argument-free built-in");
        if matches!(
            action,
            CommandAction::NewSession
                | CommandAction::CloseTab
                | CommandAction::CompactSession
                | CommandAction::ReloadConfiguration
                | CommandAction::UndoConversation
                | CommandAction::RedoConversation
        ) {
            // The binary clears these drafts only after its owner accepts the
            // intent. An optimistic removal would lose `/new` on refusal.
            if self.input != format!("/{}", option.name) {
                self.replace_slash(&option.name, false);
            }
            return self.handle_enter().await;
        }
        // The existing owner path checks availability and returns actual intents;
        // a refused command leaves the editable slash text untouched.
        let result = self.run_command(action);
        if result.note.is_none() {
            let cursor = self.editor.cursor;
            self.editor.move_to(0, false);
            self.editor.move_to(cursor, true);
            if self.editor.delete(&mut self.input, true, false) {
                self.input_revision += 1;
            }
            self.slash_selected = 0;
        }
        result
    }

    /// Active turn, if any.
    pub fn active_turn(&self) -> Option<&WorkerTurnId> {
        self.active_turn.as_ref()
    }

    /// Transcript scroll offset in rows (0 = pinned to the newest row).
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Effective offset in the last drawn viewport; requested offset survives resize.
    pub fn display_scroll(&self) -> usize {
        self.viewport
            .get()
            .filter(|view| view.requested_scroll == self.scroll)
            .map_or_else(
                || self.scroll.min(self.max_scroll()),
                |view| view.displayed_scroll,
            )
    }

    /// True while a submission awaits acceptance or a turn streams.
    pub fn is_busy(&self) -> bool {
        self.active_turn.is_some()
            || self.pending.is_some()
            || self.compactions.iter().any(crate::compaction::active)
    }

    /// Replace from the owner's conversation-filtered replay projection.
    pub fn apply_compaction_history(
        &mut self,
        snapshots: Vec<oc_core::compaction::CompactionSnapshot>,
    ) {
        self.invalidate_transcript();
        self.compactions = snapshots
            .into_iter()
            // The owner's bounded journal query returns newest-first.
            .rev()
            .filter(|s| self.session.as_ref().is_some_and(|id| id.0 == s.session))
            .collect();
        self.sync_compaction_clock();
    }

    pub fn apply_compaction(&mut self, snapshot: oc_core::compaction::CompactionSnapshot) {
        if self
            .session
            .as_ref()
            .is_none_or(|id| id.0 != snapshot.session)
        {
            return;
        }
        if let Some(current) = self.compactions.iter_mut().find(|s| s.id == snapshot.id) {
            // Admission may arrive after a running/completed broadcast.
            if !crate::compaction::active(current) && crate::compaction::active(&snapshot)
                || current.state == oc_core::compaction::CompactionState::Running
                    && snapshot.state == oc_core::compaction::CompactionState::Queued
            {
                return;
            }
            *current = snapshot;
        } else {
            self.compactions.push(snapshot);
        }
        if self.compactions.len() > 100 {
            self.compactions.remove(0);
        }
        self.invalidate_transcript();
        self.sync_compaction_clock();
    }

    /// Palette admission preserves the composer, including a literal `/compact` draft.
    pub fn compaction_request_revision(&self) -> Option<u64> {
        (self.panel != TuiPanel::Commands).then_some(self.input_revision)
    }

    pub fn compaction_admitted(
        &mut self,
        snapshot: oc_core::compaction::CompactionSnapshot,
        revision: Option<u64>,
    ) {
        if self
            .session
            .as_ref()
            .is_none_or(|id| id.0 != snapshot.session)
        {
            return;
        }
        if Some(self.input_revision) == revision
            && dispatch(self.input.trim()) == Some(CommandAction::CompactSession)
        {
            self.input.clear();
            self.editor.clear();
            self.input_revision += 1;
        }
        if self.panel == TuiPanel::Commands {
            self.close_panel();
        }
        self.apply_compaction(snapshot);
    }

    fn sync_compaction_clock(&mut self) {
        if self
            .compactions
            .iter()
            .any(|s| s.state == oc_core::compaction::CompactionState::Running)
            || self.live_parts.iter().any(|part| matches!(part,
                LivePart::Tool { card, .. } if card.name == "compress" && matches!(card.state.as_str(), "started" | "running")))
        {
            self.compaction_at.get_or_insert_with(Instant::now);
        } else {
            self.compaction_at = None;
            self.compaction_frame = 0;
        }
    }

    fn tick_compaction(&mut self, now: Instant) -> bool {
        if self.chrome.animations == Some(false) {
            return false;
        }
        let Some(at) = self.compaction_at else {
            return false;
        };
        let elapsed = now.saturating_duration_since(at).as_millis();
        let steps = elapsed / 80;
        if steps == 0 {
            return false;
        }
        self.compaction_frame = (self.compaction_frame + (steps % 10) as usize) % 10;
        self.compaction_at = now.checked_sub(Duration::from_millis((elapsed % 80) as u64));
        true
    }

    /// Status note, if any (intent errors and hints; never chat history).
    pub fn note(&self) -> Option<&str> {
        self.note.as_ref().map(|(message, _)| message.as_str())
    }

    /// Semantic color of the currently displayed note.
    pub fn note_variant(&self) -> Option<NoteVariant> {
        self.note.as_ref().map(|(_, variant)| *variant)
    }

    /// Newest page becomes the whole window; scroll pins to the newest row.
    pub fn attach_page(&mut self, page: &HistoryPage) {
        self.invalidate_transcript();
        self.remember_compaction_turns(page);
        self.completion_anchor.get_mut().take();
        self.reverted = page.reverted.clone();
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.exploration_expanded.clear();
        self.reasoning_down = None;
        self.reasoning_expanded.clear();
        self.viewport.set(None);
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.window.reset(page);
        self.scroll = 0;
        self.wheel_motion = None;
    }

    /// Same-session TurnFinished receipt only. Explicit routing/conversation
    /// resets use attach_page. Preserve a painted part, not a total-row delta:
    /// durable projection may replace synthetic parts and the paging window.
    pub fn refresh_completed_page(&mut self, page: &HistoryPage) {
        self.remember_compaction_turns(page);
        let view = self.viewport.get().filter(|_| self.scroll > 0);
        let old_rows = self.transcript_rows();
        let mut anchor = None;
        if let Some(view) = view {
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            self.transcript_part_positions(
                &old_rows,
                (view.width, view.terminal_width),
                |index, start, end| {
                    if start <= top && top < end {
                        anchor = Some((index, top - start));
                    }
                },
            );
        }
        self.reverted = page.reverted.clone();
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.clear_transcript_selection();
        self.window.refresh_completed(page, self.scroll > 0);
        self.invalidate_transcript();
        self.completion_anchor.get_mut().take();
        let rows = self.transcript_rows();
        self.reasoning_expanded = old_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let identity = row.reasoning.as_ref()?.identity?;
                if !self.reasoning_expanded.contains(&identity) {
                    return None;
                }
                let index = Self::refreshed_part(&old_rows, &rows, index)?;
                rows[index].reasoning.as_ref()?.identity
            })
            .collect();
        if let Some((index, row_offset)) = anchor {
            let matched = Self::refreshed_part(&old_rows, &rows, index);
            if let Some(index) = matched
                && let Some(message) = rows[index].message_id.clone()
            {
                let part = rows[..index]
                    .iter()
                    .filter(|row| row.message_id.as_ref() == Some(&message))
                    .count();
                *self.completion_anchor.get_mut() = Some(CompletionAnchor {
                    message,
                    part,
                    row: row_offset,
                    requested_scroll: self.scroll,
                    pending: true,
                });
            }
        }
        // Keep the painted geometry for sticky-bottom and fallback clamping.
        // The semantic anchor resolves against the new cached part positions.
    }

    fn refreshed_part(old_rows: &[HistoryRow], rows: &[HistoryRow], index: usize) -> Option<usize> {
        let old = &old_rows[index];
        if let Some(id) = &old.message_id {
            let ordinal = old_rows[..index]
                .iter()
                .filter(|row| row.message_id.as_ref() == Some(id))
                .count();
            rows.iter()
                .enumerate()
                .filter(|(_, row)| row.message_id.as_ref() == Some(id))
                .nth(ordinal)
                .map(|(index, _)| index)
        } else {
            // Live parts have no durable ID yet; operation ID or the exact
            // final text binds them to the newly committed part once.
            let same = |row: &HistoryRow| {
                row.role == old.role
                    && match (&old.tool, &row.tool) {
                        (Some(old), Some(new)) => old.op == new.op,
                        (None, None) => {
                            old.text == row.text
                                && old.reasoning.as_ref().map(|r| &r.text)
                                    == row.reasoning.as_ref().map(|r| &r.text)
                        }
                        _ => false,
                    }
            };
            let ordinal = old_rows[index + 1..].iter().filter(|row| same(row)).count();
            rows.iter()
                .enumerate()
                .rev()
                .filter(|(_, row)| same(row))
                .nth(ordinal)
                .map(|(index, _)| index)
        }
    }

    fn transcript_part_positions(
        &self,
        rows: &[HistoryRow],
        widths: (u16, u16),
        part: impl FnMut(usize, usize, usize),
    ) -> usize {
        crate::messages::transcript_part_positions(
            rows,
            Theme::dark(),
            (widths.0, widths.1, None),
            &|agent| self.agent_color(agent),
            &self.markdown_cache,
            &|op| self.exploration_expanded.contains(op),
            part,
        )
    }

    /// Add an older page at the front of the window.
    pub fn prepend_page(&mut self, page: &HistoryPage) {
        self.invalidate_transcript();
        self.remember_compaction_turns(page);
        self.completion_anchor.get_mut().take();
        self.reverted = page.reverted.clone();
        self.scroll = self.display_scroll();
        self.clear_transcript_selection();
        self.viewport.set(None);
        self.window.prepend_older(page);
        self.prune_reasoning();
    }

    /// Add a newer page at the back of the window.
    pub fn append_page(&mut self, page: &HistoryPage) {
        self.invalidate_transcript();
        self.remember_compaction_turns(page);
        self.reverted = page.reverted.clone();
        self.clear_transcript_selection();
        self.window.append_newer(page);
        self.prune_reasoning();
    }

    /// Older committed rows exist before the loaded window.
    pub fn needs_older(&self) -> bool {
        self.window.has_older()
    }

    /// Newer committed rows exist after the loaded window.
    pub fn needs_newer(&self) -> bool {
        self.window.has_newer()
    }

    /// Bounded history window (rows, caps and paging flags).
    pub fn history(&self) -> &HistoryWindow {
        &self.window
    }

    /// Close any open panel (chat view).
    pub fn close_panel(&mut self) {
        let was_open = self.panel != TuiPanel::None;
        self.wheel_motion = None;
        self.clear_transcript_selection();
        self.panel = TuiPanel::None;
        self.rename_input.clear();
        self.rename_editor.clear();
        self.rename_pending = None;
        self.rename_selected = None;
        self.session_delete_confirm = None;
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.mouse_down = None;
        self.tab_down = None;
        if was_open {
            self.tab_view.get_mut().reset_hover();
            self.last_mouse = None;
        }
        self.close_hold = None;
        self.exploration_down = None;
        self.reasoning_down = None;
        self.select.reset();
    }

    /// The active dialog exclusively owns search/cursor input; when it is
    /// replaced (Model → Variant) the former owner is destroyed, and closing
    /// the replacement restores the original prompt draft, selection and caret.
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        if self.panel == TuiPanel::None {
            self.prepare_tabs(area, Instant::now());
            if matches!(
                event.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) && self.tab_wheel_hit(area, event.column, event.row)
            {
                self.tab_scroll
                    .set(if event.kind == MouseEventKind::ScrollUp {
                        self.tab_scroll.get().saturating_sub(1)
                    } else {
                        self.tab_scroll
                            .get()
                            .saturating_add(1)
                            .min(self.tabs.len().saturating_sub(1))
                    });
                self.last_mouse = Some((event.column, event.row, area));
                self.tab_down = None;
                self.prepare_tabs(area, Instant::now());
                return KeyOutcome::default();
            }
            if crate::shell::tab_strip(self, area)
                .and_then(|strip| strip.hit_test(event.column, event.row))
                .is_none()
            {
                let view = self.tab_view.get_mut();
                view.hovered = None;
                view.leave = Some(Instant::now());
            }
        }
        if self.approvals.active().is_some() {
            if !crate::shell::tab_region(self, area).contains((event.column, event.row).into()) {
                self.tab_down = None;
                return self.approvals.mouse(event, area);
            }
            self.approvals.cancel_pointer();
        }
        use crate::dialog::DialogHit;
        if matches!(
            event.kind,
            MouseEventKind::Down(_) | MouseEventKind::Drag(_)
        ) {
            self.message_down = None;
        }
        let pointer_down = self.reasoning_pointer_down;
        match event.kind {
            MouseEventKind::Down(_) | MouseEventKind::Drag(_) => {
                self.reasoning_pointer_down = true;
            }
            MouseEventKind::Up(_) => self.reasoning_pointer_down = false,
            _ => {}
        }
        self.last_mouse = (self.panel == TuiPanel::None).then_some((event.column, event.row, area));
        if self.close_hold.as_ref().is_some_and(|hold| {
            hold.area != area
                || hold.until <= Instant::now()
                || event.row != hold.strip.tabs.first().map_or(u16::MAX, |tab| tab.rect.y)
                || !area.contains((event.column, event.row).into())
        }) || matches!(event.kind, MouseEventKind::Down(_))
        {
            self.close_hold = None;
        }
        if self.panel == TuiPanel::None {
            let toast = crate::shell::toast_rect(self, area);
            let toast_hit =
                toast.is_some_and(|rect| rect.contains((event.column, event.row).into()));
            self.set_toast_hover(toast_hit, Instant::now());
            // Only the painted close cell is an activation target. The terminal
            // does not expose selection state, so text drags must remain inert.
            let close_hit = toast
                .is_some_and(|rect| event.column == rect.right() - 4 && event.row == rect.y + 1);
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.toast_down = close_hit && event.modifiers.is_empty();
                    if self.toast_down {
                        self.selection_gesture = false;
                        self.click = None;
                        self.tab_down = None;
                        self.exploration_down = None;
                        self.reasoning_down = None;
                        return KeyOutcome::default();
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if std::mem::take(&mut self.toast_down)
                        && close_hit
                        && event.modifiers.is_empty()
                    {
                        self.note = None;
                        self.toast_expiry = None;
                        return KeyOutcome::default();
                    }
                }
                MouseEventKind::Drag(_) => self.toast_down = false,
                _ => {}
            }
        } else {
            self.toast_down = false;
            self.set_toast_hover(false, Instant::now());
        }
        // These surfaces are drawn after the transcript. Their entire painted
        // rectangles own the press and release, including blank fill cells.
        if self.panel == TuiPanel::None
            && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && self.transcript_overpainted(area, event.column, event.row)
        {
            self.selection_gesture = false;
            self.click = None;
            self.reasoning_down = None;
            self.exploration_down = None;
            self.tab_down = None;
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::None {
            if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
                && !self.transcript_overpainted(area, event.column, event.row)
            {
                let start = self
                    .painted_prompt
                    .borrow()
                    .as_ref()
                    .filter(|p| {
                        p.frame == area
                            && p.main == crate::shell::prompt_main(self, area)
                            && p.home == self.home
                            && p.session == self.session
                            && p.revision == self.input_revision
                            && p.generation == self.generation
                            && p.cursor == self.editor.cursor
                            && p.anchor == self.editor.anchor
                    })
                    .and_then(|p| {
                        p.chips
                            .iter()
                            .find(|(rect, _)| rect.contains((event.column, event.row).into()))
                            .map(|&(_, start)| start)
                    });
                if let Some(start) = start
                    && self.editor.expand_chip(&self.input, start)
                {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.mention_selected = 0;
                    self.selection_gesture = false;
                    self.click = None;
                    self.tab_down = None;
                    self.exploration_down = None;
                    self.reasoning_down = None;
                    self.clear_prompt_paint();
                    return KeyOutcome::default();
                }
            }
            self.handle_transcript_selection(event, area);
            if matches!(event.kind, MouseEventKind::Up(MouseButton::Left))
                && self.transcript_overpainted(area, event.column, event.row)
            {
                self.reasoning_down = None;
                self.exploration_down = None;
                self.tab_down = None;
                return KeyOutcome::default();
            }
            // A drag started on a header belongs to text selection; it must
            // never activate the header on release, even if it returns there.
            if matches!(event.kind, MouseEventKind::Drag(_)) {
                self.reverted_down = None;
                self.message_down = None;
                self.exploration_down = None;
                self.reasoning_down = None;
                self.tab_down = None;
            }
            match event.kind {
                MouseEventKind::Moved => {
                    self.enter_tab_at(area, event.column, event.row, Instant::now());
                    self.exploration_down = None;
                    self.reasoning_down = None;
                }
                MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                    self.reverted_down = self
                        .painted_user_message_target_at(area, event.column, event.row)
                        .filter(|target| target.reverted)
                        .and_then(|_| self.reverted.clone())
                        .map(|boundary| (boundary, self.paint_generation.get(), area));
                    self.message_down = self
                        .painted_user_message_target_at(area, event.column, event.row)
                        .and_then(|target| target.message_id)
                        .map(|id| ((*id).clone(), self.paint_generation.get(), area));
                    self.tab_down = self.tab_hit(area, event.column, event.row);
                    self.exploration_down = self
                        .exploration_hit(area, event.column, event.row)
                        .map(|op| (op, event.column, event.row));
                    self.reasoning_down =
                        self.reasoning_hit(area, event.column, event.row).map(|id| {
                            let rect = crate::shell::transcript_area(self, area);
                            let (_, total, scroll) = self.visible_transcript_at_viewport(
                                rect.width,
                                area.width,
                                rect.height,
                            );
                            (
                                id,
                                event.column,
                                event.row,
                                area,
                                total,
                                scroll,
                                self.scroll,
                            )
                        });
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let reverted_pressed = self.reverted_down.take();
                    let message_pressed = self.message_down.take();
                    let pressed_tab = self.tab_down.take();
                    let pressed = self.exploration_down.take();
                    let exploration_pressed = pressed.is_some();
                    let reasoning_pressed = self.reasoning_down.take();
                    // Resolve a user click only against the last painted,
                    // still-current block. A selection/drag owns its release.
                    if event.modifiers.is_empty()
                        && self
                            .user_message_target_at(area, event.column, event.row)
                            .is_some_and(|target| target.reverted)
                        && self.reverted.as_ref().is_some_and(|boundary| {
                            reverted_pressed.as_ref()
                                == Some(&(boundary.clone(), self.paint_generation.get(), area))
                        })
                    {
                        return self.run_command(CommandAction::RedoConversation);
                    }
                    if event.modifiers.is_empty()
                        && let Some(target) =
                            self.user_message_target_at(area, event.column, event.row)
                        && !target.reverted
                        && let Some(message) = target.message_id
                        && message_pressed
                            == Some(((*message).clone(), self.paint_generation.get(), area))
                    {
                        self.select.reset();
                        self.panel = TuiPanel::MessageActions {
                            message: (*message).clone(),
                            seq: target.seq,
                        };
                        return KeyOutcome::default();
                    }
                    if event.modifiers.is_empty()
                        && let Some(tab) = pressed_tab
                        && self.tab_hit(area, event.column, event.row) == Some(tab)
                    {
                        return KeyOutcome {
                            intent: Some(match tab {
                                TabPress::Add => PanelIntent::NewSession,
                                TabPress::Tab(index) => PanelIntent::ActivateTab { index },
                                TabPress::Close(index) => PanelIntent::CloseTab { index },
                            }),
                            ..KeyOutcome::default()
                        };
                    }
                    if event.modifiers.is_empty()
                        && self.click.is_none_or(|click| click.count == 1)
                        && let Some((op, x, y)) = pressed
                        && (x, y) == (event.column, event.row)
                        && self
                            .exploration_hit(area, event.column, event.row)
                            .as_deref()
                            == Some(&op)
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let height = rect.height as usize;
                        let (_, before, displayed) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        // Keep the clicked header at its painted row by anchoring
                        // the first visible row, rather than the bottom offset.
                        let first = before.saturating_sub(height).saturating_sub(displayed);
                        let rows = self.transcript_rows();
                        self.exploration_expanded.retain(|id| {
                            rows.iter()
                                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
                        });
                        if !self.exploration_expanded.insert(op.clone()) {
                            self.exploration_expanded.remove(&op);
                        }
                        let (_, after) =
                            self.visible_transcript(rect.width, area.width, rect.height);
                        self.scroll = after.saturating_sub(height).saturating_sub(first);
                        self.observe_transcript_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                            after,
                            self.scroll,
                        );
                    }
                    // The pinned original toggles onMouseUp without a down.
                    // Admit that path only for a currently painted header; a
                    // consumed/stale press or selection gesture stays inert.
                    let release_only = if reasoning_pressed.is_none()
                        && pressed_tab.is_none()
                        && !exploration_pressed
                        && !pointer_down
                        && event.modifiers.is_empty()
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let (rows, total, scroll) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        self.painted_transcript
                            .borrow()
                            .as_ref()
                            .filter(|painted| {
                                painted.area == rect
                                    && painted.total == total
                                    && painted.scroll == scroll
                                    && painted.rows == rows
                            })
                            .and_then(|_| self.reasoning_hit(area, event.column, event.row))
                            .map(|id| {
                                (
                                    id,
                                    event.column,
                                    event.row,
                                    area,
                                    total,
                                    scroll,
                                    self.scroll,
                                )
                            })
                    } else {
                        None
                    };
                    if event.modifiers.is_empty()
                        && self.click.is_none_or(|click| click.count == 1)
                        && matches!(self.selection_text(), Ok(None))
                        && let Some((id, x, y, painted, total, scroll, requested)) =
                            reasoning_pressed.or(release_only)
                        && painted == area
                        && requested == self.scroll
                        && (x, y) == (event.column, event.row)
                        && self.reasoning_hit(area, x, y) == Some(id)
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let (_, now, displayed) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        if (now, displayed) == (total, scroll) {
                            let first = now
                                .saturating_sub(rect.height as usize)
                                .saturating_sub(displayed);
                            self.prune_reasoning();
                            if !self.reasoning_expanded.insert(id) {
                                self.reasoning_expanded.remove(&id);
                            }
                            let (_, after) =
                                self.visible_transcript(rect.width, area.width, rect.height);
                            self.scroll = after
                                .saturating_sub(rect.height as usize)
                                .saturating_sub(first);
                            self.observe_transcript_viewport(
                                rect.width,
                                area.width,
                                rect.height,
                                after,
                                self.scroll,
                            );
                        }
                    }
                }
                MouseEventKind::Drag(_)
                | MouseEventKind::Down(_)
                | MouseEventKind::Up(_)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown => {
                    self.exploration_down = None;
                    self.reasoning_down = None;
                    self.tab_down = None;
                    if matches!(event.kind, MouseEventKind::Drag(_)) {
                        self.tab_view.get_mut().reset_hover();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.reasoning_down = None;
        self.tab_down = None;
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        if self.panel == TuiPanel::Rename {
            let rect = crate::dialog::rename_geometry(area);
            let hit = if !rect.contains((event.column, event.row).into()) {
                DialogHit::Backdrop
            } else if event.row == rect.y + 1
                && event.column >= rect.right().saturating_sub(7)
                && event.column < rect.right().saturating_sub(4)
            {
                DialogHit::Close
            } else {
                DialogHit::Surface
            };
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => self.mouse_down = Some(hit),
                MouseEventKind::Up(MouseButton::Left) => {
                    let pressed = self.mouse_down.take();
                    if self.rename_pending.is_none()
                        && ((pressed == Some(DialogHit::Backdrop) && hit == DialogHit::Backdrop)
                            || (pressed == Some(DialogHit::Close) && hit == DialogHit::Close))
                    {
                        self.close_panel();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::Cards && self.card_output.is_some() {
            let (rect, _, _) = crate::dialog::card_geometry(area);
            let inside = rect.contains((event.column, event.row).into());
            match event.kind {
                MouseEventKind::ScrollUp if inside => {
                    for _ in 0..3 {
                        self.handle_panel_key(KeyAction::Up);
                    }
                }
                MouseEventKind::ScrollDown if inside => {
                    for _ in 0..3 {
                        self.handle_panel_key(KeyAction::Down);
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    self.mouse_down = Some(if inside {
                        DialogHit::Surface
                    } else {
                        DialogHit::Backdrop
                    });
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let hit = if inside {
                        DialogHit::Surface
                    } else {
                        DialogHit::Backdrop
                    };
                    if self.mouse_down.take() == Some(DialogHit::Backdrop)
                        && hit == DialogHit::Backdrop
                    {
                        self.close_panel();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        let options = self.modal_options();
        let size = crate::dialog::size_for(&self.panel);
        let hit = self
            .select
            .hit(area, size, &options, event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.mouse_down = Some(hit);
                if let DialogHit::Option(index) = hit {
                    if self.select.cursor != index {
                        self.session_delete_confirm = None;
                    }
                    self.select.cursor = index;
                    self.sync_modal_cursor();
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                if let DialogHit::Option(index) = hit
                    && self.select.cursor != index
                {
                    self.session_delete_confirm = None;
                    self.select.cursor = index;
                    self.sync_modal_cursor();
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if hit != DialogHit::Backdrop {
                    self.select.scroll_rows(
                        if event.kind == MouseEventKind::ScrollUp {
                            -3
                        } else {
                            3
                        },
                        area,
                        size,
                        &options,
                    );
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self.mouse_down.take();
                if pressed == Some(hit) {
                    match hit {
                        DialogHit::Backdrop | DialogHit::Close => self.close_panel(),
                        DialogHit::Option(index)
                            if !matches!(
                                self.panel,
                                TuiPanel::Dcp | TuiPanel::Cards | TuiPanel::Help(_)
                            ) =>
                        {
                            self.select.cursor = index;
                            self.sync_modal_cursor();
                            return self.panel_enter();
                        }
                        DialogHit::Option(_)
                        | DialogHit::Search
                        | DialogHit::Surface
                        | DialogHit::List => {}
                    }
                }
            }
            _ => {}
        }
        KeyOutcome::default()
    }

    /// Snapshot only the rows actually painted. This same bounded set supplies
    /// the rendered highlight and the clipboard; a new frame or resize makes
    /// stale selections unusable until the next valid mouse selection.
    #[cfg(test)]
    pub(crate) fn paint_transcript(
        &self,
        area: Rect,
        rows: &[Line],
        total: usize,
        scroll: usize,
    ) -> Vec<Line> {
        self.paint_transcript_at(area, rows, total, scroll, None, &[])
    }

    /// The terminal frame is the pointer's owner. The visible rows already
    /// have the clipped header and styles; drawing must not re-index history.
    pub(crate) fn paint_transcript_at(
        &self,
        area: Rect,
        rows: &[Line],
        total: usize,
        scroll: usize,
        frame: Option<Rect>,
        user_targets: &[Option<crate::messages::UserMessageTarget>],
    ) -> Vec<Line> {
        let theme = Theme::dark();
        let hovered_user = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || area != crate::shell::transcript_area(self, frame)
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
                || x <= area.x
            {
                return None;
            }
            user_targets.get((y - area.y) as usize).cloned().flatten()
        });
        let hover_row = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || self.thinking_expanded
                || area != crate::shell::transcript_area(self, frame)
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
            {
                return None;
            }
            let row = (y - area.y) as usize;
            crate::messages::collapsed_thought_header(rows.get(row)?, theme, (x - area.x) as usize)
                .then_some(row)
        });
        let hover_tool = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || area != crate::shell::transcript_area(self, frame)
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
            {
                return None;
            }
            let row = (y - area.y) as usize;
            // A short transcript does not paint every row in its viewport.
            // A pointer retained from the composer can land in that blank
            // space after submission; it cannot authorize a tool hover.
            let painted = rows.get(row)?;
            let range = crate::messages::tool_hover_range(rows, theme, row);
            let block = painted.style().bg == Some(theme.background_raised());
            let header = rows.get(range.start)?.plain_text();
            // Reject other painted surfaces before another indexed hit lookup.
            // Text is only a cheap candidate filter: the owner hit still decides.
            if !block
                && !header.trim_start().starts_with("→ Explored")
                && !header.trim_start().starts_with("⋯ Exploring")
            {
                return None;
            }
            self.exploration_hit(frame, x, y).map(|_| range)
        });
        let selected = self.selection.as_ref().filter(|selected| {
            selected.painted.area == area
                && selected.painted.total == total
                && selected.painted.scroll == scroll
                && selected.painted.rows == rows
                && self.panel == TuiPanel::None
        });
        let result = rows
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let hovered;
                let line = if hovered_user.is_some()
                    && user_targets.get(row).cloned().flatten() == hovered_user
                {
                    hovered = if hovered_user.as_ref().is_some_and(|target| target.reverted) {
                        crate::messages::hover_reverted_content(line, theme)
                    } else {
                        crate::messages::hover_user_content(line, theme)
                    };
                    &hovered
                } else if hover_row == Some(row) {
                    hovered = crate::messages::hover_collapsed_thought(line, theme);
                    &hovered
                } else if hover_tool
                    .as_ref()
                    .is_some_and(|range| range.contains(&row))
                {
                    hovered = crate::messages::hover_tool_content(line, theme);
                    &hovered
                } else {
                    line
                };
                if let Some(selected) = selected {
                    let (start, end) = selected.bounds();
                    let begin = if row == start.row {
                        start.byte
                    } else if row > start.row {
                        0
                    } else {
                        line.plain_text().len()
                    };
                    let finish = if row == end.row {
                        end.byte
                    } else if row < end.row {
                        line.plain_text().len()
                    } else {
                        0
                    };
                    if begin < finish {
                        return line.highlight(begin, finish, theme.text(), theme.background());
                    }
                }
                line.clone()
            })
            .collect();
        let mut painted = self.painted_transcript.borrow_mut();
        if !painted.as_ref().is_some_and(|previous| {
            previous.area == area
                && previous.total == total
                && previous.scroll == scroll
                && previous.rows == rows
                && previous.user_targets == user_targets
        }) {
            self.paint_generation
                .set(self.paint_generation.get().wrapping_add(1));
            *painted = Some(PaintedTranscript {
                area,
                rows: rows.to_vec(),
                user_targets: user_targets.to_vec(),
                total,
                scroll,
            });
        }
        result
    }

    /// Configuration/test override of the platform's native clipboard mode.
    pub fn set_clipboard_mode(&mut self, mode: ClipboardMode) {
        self.clipboard_mode = mode;
    }

    pub fn clipboard_mode(&self) -> ClipboardMode {
        self.clipboard_mode
    }

    /// A parked route may carry an older catalog. Route activation adopts the
    /// current view's last successful owner projection, not the parked value.
    pub fn sync_clipboard_mode_from(&mut self, current: &Self) {
        self.chrome.session_tps = current.chrome.session_tps;
        self.chrome.diffs = current.chrome.diffs;
        self.set_conversation_shortcuts(
            Some(current.conversation_shortcut(true)),
            Some(current.conversation_shortcut(false)),
        );
        self.chrome.conversation_shortcuts = current.chrome.conversation_shortcuts.clone();
        if current.clipboard_mode == ClipboardMode::Disabled {
            self.disable_clipboard_until_catalog();
        } else {
            self.apply_owner_clipboard_mode(current.owner_clipboard_mode);
        }
    }

    /// A reload receipt is already published even if a later route-specific
    /// catalog query fails. Apply this safety-sensitive owner setting first so
    /// a stale view cannot keep auto-copy enabled after switching to manual.
    pub fn refresh_clipboard_mode(&mut self, mode: Option<oc_core::queries::TerminalCopyMode>) {
        self.clear_transcript_selection();
        self.apply_owner_clipboard_mode(mode);
    }

    pub fn disable_clipboard_until_catalog(&mut self) {
        self.clear_transcript_selection();
        self.clipboard_mode = ClipboardMode::Disabled;
    }

    fn apply_owner_clipboard_mode(&mut self, mode: Option<oc_core::queries::TerminalCopyMode>) {
        self.owner_clipboard_mode = mode;
        self.chrome.terminal_copy = mode;
        self.clipboard_mode = match mode {
            Some(oc_core::queries::TerminalCopyMode::Select) => ClipboardMode::Select,
            Some(oc_core::queries::TerminalCopyMode::Manual) => ClipboardMode::Manual,
            None => ClipboardMode::default(),
        };
    }

    /// Drain the mouse copy request once. The caller performs the actual
    /// clipboard write and reports its asynchronous result separately.
    pub fn take_copy_request(&mut self) -> Option<String> {
        self.pending_copy.take()
    }

    /// Exact owner text, never a wrapped or locally shortened preview.
    pub fn copy_message_text(&mut self, text: String) -> Result<(), String> {
        if self.clipboard_mode == ClipboardMode::Disabled {
            return Err("clipboard configuration unavailable".into());
        }
        if text.len() > MAX_SELECTION_BYTES {
            return Err("Message exceeds clipboard size limit".into());
        }
        self.pending_copy = Some(text);
        Ok(())
    }

    pub fn restore_prompt(&mut self, text: String) {
        self.input = text;
        self.editor.clear();
        self.editor.cursor = self.input.len();
        self.input_revision += 1;
    }

    pub fn conversation_applied(
        &mut self,
        snapshot: &oc_core::queries::ConversationSnapshot,
        page: &HistoryPage,
    ) {
        let draft = snapshot.draft.clone().unwrap_or_else(|| {
            if matches!(
                dispatch(self.input.trim()),
                Some(CommandAction::UndoConversation | CommandAction::RedoConversation)
            ) {
                String::new()
            } else {
                self.input.clone()
            }
        });
        self.set_session(snapshot.session.clone());
        self.attach_page(page);
        self.restore_prompt(draft);
        self.conversation_available = Some((snapshot.can_undo, snapshot.can_redo));
        self.reverted = snapshot.reverted.clone();
        self.dcp = DcpPanelState::default();
    }

    /// The binary reports the actual asynchronous clipboard write outcome.
    pub fn report_clipboard_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                if matches!(self.panel, TuiPanel::MessageActions { .. }) {
                    self.close_panel();
                }
                self.push_transient_note("Copied to clipboard", NoteVariant::Info);
            }
            Err(error) => self.push_transient_note(&error, NoteVariant::Error),
        }
    }

    fn clear_transcript_selection(&mut self) {
        self.reverted_down = None;
        self.message_down = None;
        self.paint_generation
            .set(self.paint_generation.get().wrapping_add(1));
        self.selection = None;
        self.selection_gesture = false;
        self.click = None;
        self.pending_copy = None;
        *self.painted_transcript.borrow_mut() = None;
    }

    fn selection_text(&self) -> Result<Option<String>, &'static str> {
        const TOO_LARGE: &str = "Selection exceeds clipboard size limit";
        let Some(selected) = self.selection.as_ref() else {
            return Ok(None);
        };
        let painted = self.painted_transcript.borrow();
        let Some(current) = painted.as_ref() else {
            return Ok(None);
        };
        if selected.painted.area != current.area
            || selected.painted.total != current.total
            || selected.painted.scroll != current.scroll
            || selected.painted.rows != current.rows
        {
            return Ok(None);
        }
        let (start, end) = selected.bounds();
        if start == end {
            return Ok(None);
        }
        let mut result = String::new();
        for row in start.row..=end.row {
            let Some(line) = selected.painted.rows.get(row) else {
                return Ok(None);
            };
            let text = line.plain_text();
            let begin = if row == start.row { start.byte } else { 0 };
            let finish = if row == end.row { end.byte } else { text.len() };
            if begin > finish || !text.is_char_boundary(begin) || !text.is_char_boundary(finish) {
                return Ok(None);
            }
            let addition = finish - begin + usize::from(row > start.row);
            if result.len().saturating_add(addition) > MAX_SELECTION_BYTES {
                return Err(TOO_LARGE);
            }
            if row > start.row {
                result.push('\n');
            }
            result.push_str(&text[begin..finish]);
        }
        Ok((!result.is_empty()).then_some(result))
    }

    fn request_selection_copy(&mut self) {
        self.pending_copy = None;
        match self.selection_text() {
            Ok(Some(text)) => self.pending_copy = Some(text),
            Ok(None) => {}
            Err(error) => self.push_transient_note(error, NoteVariant::Error),
        }
    }

    fn handle_transcript_selection(&mut self, event: MouseEvent, area: Rect) {
        let relevant = matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && event.modifiers.is_empty()
            || matches!(event.kind, MouseEventKind::Down(MouseButton::Right))
                && self.clipboard_mode == ClipboardMode::Manual
            || self.selection_gesture
                && matches!(event.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        if !relevant {
            // A new press on any other surface breaks the multi-click chain,
            // without destroying the completed selection for manual copy.
            if matches!(event.kind, MouseEventKind::Down(_)) {
                self.selection_gesture = false;
                self.click = None;
            }
            return;
        }
        let current = crate::shell::transcript_area(self, area);
        let painted_valid = self
            .painted_transcript
            .borrow()
            .as_ref()
            .is_some_and(|painted| painted.area == current);
        if !painted_valid {
            self.clear_transcript_selection();
            return;
        }
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && !current.contains((event.column, event.row).into())
        {
            self.selection_gesture = false;
            self.click = None;
            return;
        }
        let (rows, total, scroll) =
            self.visible_transcript_at_viewport(current.width, area.width, current.height);
        if !self
            .painted_transcript
            .borrow()
            .as_ref()
            .is_some_and(|painted| {
                painted.total == total && painted.scroll == scroll && painted.rows == rows
            })
        {
            self.clear_transcript_selection();
            return;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Right)
                if self.clipboard_mode == ClipboardMode::Manual =>
            {
                self.selection_gesture = false;
                self.click = None;
                self.request_selection_copy();
            }
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                let painted = self
                    .painted_transcript
                    .borrow()
                    .clone()
                    .expect("validated paint");
                let Some(at) = self.transcript_point(&painted, area, event.column, event.row)
                else {
                    self.selection_gesture = false;
                    self.click = None;
                    return;
                };
                let now = Instant::now();
                let count = self
                    .click
                    .filter(|click| {
                        click.x == event.column
                            && click.y == event.row
                            && now.duration_since(click.at) <= Duration::from_millis(500)
                    })
                    .map_or(1, |click| (click.count % 3) + 1);
                self.click = Some(TranscriptClick {
                    x: event.column,
                    y: event.row,
                    at: now,
                    count,
                });
                let mut selected = TranscriptSelection {
                    anchor: at,
                    focus: at,
                    painted,
                    dragging: count > 1,
                };
                self.selection_gesture = true;
                if count == 2 || count == 3 {
                    let text = selected.painted.rows[at.row].plain_text();
                    if count == 3 {
                        // OpenTUI paints the content of the display row, not
                        // the indentation before its first visible glyph.
                        selected.anchor.byte = text.len() - text.trim_start().len();
                        selected.focus.byte = text.len();
                    } else {
                        // OpenTUI's painted word includes internal hyphens
                        // (GEOMETRY-SHORT), but not the adjacent colon. Group
                        // Unicode words on this already-wrapped display row.
                        let words: Vec<_> = text.unicode_word_indices().collect();
                        if let Some((index, (offset, word))) =
                            words.iter().enumerate().find(|(_, (offset, word))| {
                                *offset <= at.byte && at.byte < *offset + word.len()
                            })
                        {
                            let mut start = *offset;
                            let mut end = start + word.len();
                            for (next_offset, next_word) in words[index + 1..].iter() {
                                if text.get(end..*next_offset) != Some("-") {
                                    break;
                                }
                                end = next_offset + next_word.len();
                            }
                            for (previous_offset, previous_word) in words[..index].iter().rev() {
                                if text.get(previous_offset + previous_word.len()..start)
                                    != Some("-")
                                {
                                    break;
                                }
                                start = *previous_offset;
                            }
                            selected.anchor.byte = start;
                            selected.focus.byte = end;
                        }
                    }
                }
                self.selection = Some(selected);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let at = self
                    .painted_transcript
                    .borrow()
                    .as_ref()
                    .and_then(|painted| {
                        self.transcript_point(painted, area, event.column, event.row)
                    });
                if let Some(at) = at
                    && let Some(selected) = &mut self.selection
                {
                    selected.dragging = true;
                    selected.focus = at;
                }
                self.click = None;
            }
            MouseEventKind::Drag(_) => {
                // Only a left press owns this selection. An unrelated button
                // must never turn an old highlight into a new drag/copy.
                self.selection_gesture = false;
                self.click = None;
            }
            MouseEventKind::Up(_) => {
                self.selection_gesture = false;
                let at = self
                    .painted_transcript
                    .borrow()
                    .as_ref()
                    .and_then(|painted| {
                        self.transcript_point(painted, area, event.column, event.row)
                    });
                if let Some(selected) = &mut self.selection
                    && selected.dragging
                {
                    if let Some(at) = at {
                        // Repeated-click word/line selection keeps its
                        // expanded endpoints on a no-motion release.
                        if self.click.is_none_or(|click| {
                            click.count == 1 || (click.x, click.y) != (event.column, event.row)
                        }) {
                            selected.focus = at;
                        }
                    }
                    selected.dragging = false;
                    if self.clipboard_mode == ClipboardMode::Select {
                        self.request_selection_copy();
                    }
                }
            }
            MouseEventKind::Down(_) => {
                self.selection_gesture = false;
                self.click = None;
            }
            _ => {}
        }
    }

    fn user_message_target_at(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::UserMessageTarget> {
        let area = crate::shell::transcript_area(self, frame);
        if self.panel != TuiPanel::None
            || !area.contains((x, y).into())
            || x <= area.x
            || self.transcript_overpainted(frame, x, y)
            || self.click.is_none_or(|click| click.count != 1)
            || self.selection_gesture
            || !matches!(self.selection_text(), Ok(None))
        {
            return None;
        }
        self.painted_user_message_target_at(frame, x, y)
    }

    fn painted_user_message_target_at(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::UserMessageTarget> {
        let area = crate::shell::transcript_area(self, frame);
        if self.panel != TuiPanel::None
            || !area.contains((x, y).into())
            || x <= area.x
            || self.transcript_overpainted(frame, x, y)
        {
            return None;
        }
        let (rows, total, scroll, targets) =
            self.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        let painted = self.painted_transcript.borrow();
        let painted = painted.as_ref()?;
        if painted.area != area
            || painted.total != total
            || painted.scroll != scroll
            || painted.rows != rows
            || painted.user_targets != targets
        {
            return None;
        }
        targets.get((y - area.y) as usize).cloned().flatten()
    }

    fn transcript_point(
        &self,
        painted: &PaintedTranscript,
        area: Rect,
        x: u16,
        y: u16,
    ) -> Option<TextPoint> {
        let rect = painted.area;
        if !rect.contains((x, y).into()) || self.transcript_overpainted(area, x, y) {
            return None;
        }
        let row = (y - rect.y) as usize;
        Some(TextPoint {
            row,
            byte: painted.rows.get(row)?.byte_at_cell((x - rect.x) as usize),
        })
    }

    fn tab_hit(&self, area: Rect, x: u16, y: u16) -> Option<TabPress> {
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
                        .then_some(self.session_title.as_deref())
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

    fn enter_tab_at(&mut self, area: Rect, x: u16, y: u16, now: Instant) {
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

    fn next_tab_deadline(&self) -> Option<Instant> {
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

    fn tick_tabs(&mut self, now: Instant) -> bool {
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

    fn exploration_hit(&self, area: Rect, x: u16, y: u16) -> Option<String> {
        let rect = crate::shell::transcript_area(self, area);
        if rect.width == 0 || rect.height == 0 || !rect.contains((x, y).into()) {
            return None;
        }
        let rows = self.transcript_rows();
        let live_row = (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| {
            rows.len()
                - 1
                - usize::from(rows.last().is_some_and(|row| row.role == "reverted"))
                - usize::from(self.active_turn.is_some() && self.live_preview_truncated)
        });
        crate::messages::exploration_header_at(
            &rows,
            Theme::dark(),
            (rect.width, area.width),
            (
                rect.height as usize,
                self.scroll_for_current_view(),
                live_row,
            ),
            |agent| self.agent_color(agent),
            &self.markdown_cache,
            (
                &|op| self.exploration_expanded.contains(op),
                ((x - rect.x) as usize, (y - rect.y) as usize),
            ),
        )
    }

    fn reasoning_hit(
        &self,
        area: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::ReasoningIdentity> {
        if self.thinking_expanded || self.transcript_overpainted(area, x, y) {
            return None;
        }
        let rect = crate::shell::transcript_area(self, area);
        if rect.width == 0 || rect.height == 0 || !rect.contains((x, y).into()) {
            return None;
        }
        let rows = self.transcript_rows();
        let live_row = (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| {
            rows.len() - 1 - usize::from(self.active_turn.is_some() && self.live_preview_truncated)
        });
        crate::messages::reasoning_header_at(
            &rows,
            Theme::dark(),
            (rect.width, area.width),
            (
                rect.height as usize,
                self.scroll_for_current_view(),
                live_row,
            ),
            |agent| self.agent_color(agent),
            &self.markdown_cache,
            (
                &|op| self.exploration_expanded.contains(op),
                ((x - rect.x) as usize, (y - rect.y) as usize),
            ),
        )
    }

    fn transcript_overpainted(&self, area: Rect, x: u16, y: u16) -> bool {
        if crate::shell::toast_rect(self, area).is_some_and(|rect| rect.contains((x, y).into())) {
            return true;
        }
        if self.slash_options().is_none() && self.mention_options().is_none() {
            return false;
        }
        // Recover the session main column from the exact painted transcript
        // rectangle, then use the same prompt allocation as shell::render_session.
        let transcript = crate::shell::transcript_area(self, area);
        if transcript.width == 0 {
            return false;
        }
        let shell = crate::layout::configured_shell_regions(
            area,
            self.chrome.devtools_visible(),
            self.chrome.vertical_tabs_width,
        );
        let pad = transcript.x.saturating_sub(shell.session.x);
        let main = Rect::new(
            shell.session.x,
            shell.session.y,
            transcript.width.saturating_add(2 * pad),
            shell.session.height,
        );
        let text_width = main.width.saturating_sub(4 * pad + 1).max(1);
        let input_height = (self.prompt_layout(text_width as usize).0.len() as u16)
            .min((area.height / 3).max(6))
            .max(1);
        let body = crate::layout::dynamic_session_regions(main, 0, input_height + 3).prompt;
        let covers = |count: usize| {
            let height = (count.clamp(1, 10) as u16).min(body.y.saturating_sub(area.y));
            let rect = Rect::new(body.x, body.y.saturating_sub(height), body.width, height);
            rect.width >= 3 && rect.height > 0 && rect.contains((x, y).into())
        };
        self.slash_options()
            .is_some_and(|options| covers(options.len()))
            || self.mention_options().is_some_and(|options| {
                let height = (options.paths.len().clamp(1, MENTION_LIMIT) as u16)
                    .min(body.y.saturating_sub(area.y));
                let rect = Rect::new(body.x, body.y.saturating_sub(height), body.width, height);
                rect.width >= 3 && rect.height > 0 && rect.contains((x, y).into())
            })
    }

    fn prune_reasoning(&mut self) {
        let retained: BTreeSet<_> = self
            .transcript_rows()
            .iter()
            .filter_map(|row| row.reasoning.as_ref()?.identity)
            .collect();
        self.reasoning_expanded.retain(|id| retained.contains(id));
        self.reasoning_down = self
            .reasoning_down
            .take()
            .filter(|(id, ..)| retained.contains(id));
    }

    /// Window bytes plus live text, live parts and input; bounded by the
    /// window caps.
    pub fn retained_bytes(&self) -> usize {
        self.window.retained_bytes()
            + self.markdown_cache.borrow().retained_bytes()
            + self.visible_projection.borrow().as_ref().map_or(0, |view| {
                view.lines
                    .iter()
                    .flat_map(Line::spans)
                    .map(|span| std::mem::size_of::<crate::styled::Span>() + span.content().len())
                    .sum::<usize>()
                    + view.targets.len()
                        * std::mem::size_of::<Option<crate::messages::UserMessageTarget>>()
            })
            + self.live_text.len()
            + self.live_reasoning.len()
            + self
                .pending_tool_seen
                .iter()
                .map(String::len)
                .sum::<usize>()
            + self
                .live_parts
                .iter()
                .map(LivePart::retained_bytes)
                .sum::<usize>()
            + self.input.len()
            + self.mention_result.as_ref().map_or(0, |(key, result)| {
                key.query.len()
                    + key.location.len()
                    + result.location.len()
                    + result.paths.iter().map(String::len).sum::<usize>()
            })
            + self
                .mention_dismissed
                .as_ref()
                .map_or(0, |key| key.query.len() + key.location.len())
            + self.editor.retained_bytes()
            + self
                .pending
                .as_ref()
                .map_or(0, |pending| pending.draft.len())
    }

    /// No transcript projection, Markdown parse, or history walk. The cache is
    /// persistent on each view, including parked tabs; counts are payload
    /// bytes, not String/Vec capacities or temporary frame allocations.
    pub fn live_view_metrics(&self) -> LiveViewMetrics {
        let mut result = LiveViewMetrics {
            text_bytes: self.live_text.len(),
            reasoning_bytes: self.live_reasoning.len(),
            part_count: self.live_parts.len(),
            markdown_cache_retained_bytes: self.markdown_cache.borrow().retained_bytes(),
        };
        for part in &self.live_parts {
            match part {
                LivePart::Text(text) => result.text_bytes += text.len(),
                LivePart::Reasoning { text, .. } => result.reasoning_bytes += text.len(),
                LivePart::Tool { .. } | LivePart::Vacant => {}
            }
        }
        result
    }

    /// Visible viewport lines (bounded, scroll-aware, live answer last).
    ///
    /// Unstyled and unwrapped; [`TuiState::transcript_lines`] is the styled
    /// renderer the shell uses.
    pub fn viewport(&self) -> Vec<String> {
        let lines = self.transcript_lines(0, u16::MAX);
        let texts: Vec<String> = lines
            .iter()
            .map(|line| line.plain_text().trim_end().to_string())
            .collect();
        let total = texts.len();
        let max_scroll = total.saturating_sub(VIEWPORT_LINES);
        let scroll = self.scroll.min(max_scroll);
        let end = total - scroll;
        let start = end.saturating_sub(VIEWPORT_LINES);
        texts[start..end].to_vec()
    }

    /// Render rows of the transcript: the bounded window plus the live parts
    /// (frozen text/reasoning segments and tool cards) and the open live
    /// answer (reasoning block and streaming text) while a turn is active.
    pub fn transcript_rows(&self) -> Vec<HistoryRow> {
        #[cfg(test)]
        self.transcript_row_copies
            .set(self.transcript_row_copies.get() + self.window.len());
        let mut rows = self.window.rows().to_vec();
        for (ordinal, part) in self.live_parts.iter().enumerate() {
            if matches!(part, LivePart::Vacant) {
                continue;
            }
            rows.push(part.to_row(
                self.active_agent.clone(),
                Some(crate::messages::ReasoningIdentity::Live(
                    self.reasoning_epoch,
                    self.live_part_offset + ordinal,
                )),
            ));
        }
        let live = !self.live_text.is_empty() || !self.live_reasoning.is_empty();
        if live {
            rows.push(HistoryRow {
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: self.live_text.clone(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning: (!self.live_reasoning.is_empty()).then(|| ReasoningBlock {
                    text: self.live_reasoning.clone(),
                    duration_ms: None,
                    running: true,
                    expanded: false,
                    toggleable: true,
                    identity: Some(crate::messages::ReasoningIdentity::Live(
                        self.reasoning_epoch,
                        self.live_part_offset + self.live_parts.len(),
                    )),
                }),
                meta: None,
                tool: None,
            });
        }
        if self.active_turn.is_some() && self.live_preview_truncated {
            rows.push(HistoryRow {
                seq:i64::MAX,role:"assistant".into(),text:"[Live preview truncated; durable parts remain available through history and /cards]".into(),
                message_id: None,
                agent:None,agent_color_index:None,chips:Vec::new(),reasoning:None,meta:None,tool:None,
            });
        }
        let mut group_first = None;
        for row in &mut rows {
            let adjacent = crate::messages::reasoning_group_member(row);
            if !adjacent {
                group_first = None;
            }
            let visible = crate::messages::visible_reasoning(row);
            if let Some(reasoning) = &mut row.reasoning {
                let id = reasoning.identity;
                let first = if adjacent && visible {
                    *group_first.get_or_insert(id)
                } else {
                    id
                };
                reasoning.expanded = self.thinking_expanded
                    || first.is_some_and(|id| self.reasoning_expanded.contains(&id));
                reasoning.toggleable = !self.thinking_expanded;
            }
        }
        if let Some(reverted) = &self.reverted
            && !self.window.has_newer()
        {
            rows.push(HistoryRow {
                message_id: Some(std::sync::Arc::new(reverted.message.clone())),
                seq: i64::MAX,
                role: "reverted".into(),
                text: reverted.user_messages.to_string(),
                agent: Some(self.conversation_shortcut(false)),
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: None,
            });
        }
        self.interleave_compactions(&mut rows);
        // Apply current owner configuration to the render copy, including
        // replayed/parked footer metadata. Retain every measured statistic in
        // the history/live state. Indexed footers are generated outside the
        // Markdown body cache, so config changes cannot reuse a stale footer.
        for row in &mut rows {
            if let Some(card) = &mut row.tool {
                card.diff_settings = self.chrome.diffs;
                if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
                    view.config = self.chrome.dcp.clone();
                    view.color_index = row.agent_color_index.or(view.color_index).or_else(|| {
                        row.agent.as_ref().and_then(|id| {
                            self.agents
                                .iter()
                                .find(|entry| &entry.id == id)
                                .map(|entry| entry.color_index)
                        })
                    });
                    view.spinner = (self.chrome.animations != Some(false))
                        .then(|| crate::compaction::FRAMES[self.compaction_frame].to_string());
                }
            }
            if let Some(meta) = &mut row.meta {
                meta.session_tps = self.chrome.session_tps;
            }
        }
        rows
    }

    fn interleave_compactions(&self, rows: &mut Vec<HistoryRow>) {
        // Traverse newest first so multiple checkpoints sharing one anchor stay ordered.
        for snapshot in self.compactions.iter().rev() {
            let anchor = &snapshot.anchor;
            let at = [
                anchor.tool.as_ref().and_then(|tool| {
                    rows.iter()
                        .rposition(|r| r.tool.as_ref().is_some_and(|card| &card.op == tool))
                }),
                anchor.message.as_ref().and_then(|message| {
                    rows.iter()
                        .rposition(|r| r.message_id.as_ref().is_some_and(|id| &id.0 == message))
                }),
            ]
            .into_iter()
            .flatten()
            .max()
            // A turn can acquire continuation text after this checkpoint. Its
            // replay message is a fallback, never a replacement for an exact
            // tool/message boundary (OC2 compaction20260927-10 completed frame).
            .or_else(|| {
                anchor
                    .turn
                    .as_ref()
                    .and_then(|turn| self.compaction_turn_messages.get(turn))
                    .and_then(|message| {
                        rows.iter()
                            .rposition(|r| r.message_id.as_ref().is_some_and(|id| &id.0 == message))
                    })
            });
            let position = if let Some(at) = at {
                at + 1
            } else if anchor.message.is_none()
                && anchor.tool.is_none()
                && anchor.turn.is_none()
                && !self.window.has_older()
            {
                0
            } else if crate::compaction::active(snapshot) && !self.window.has_newer() {
                rows.len()
            } else {
                continue;
            };
            rows.insert(
                position,
                crate::compaction::row(
                    snapshot,
                    self.compaction_frame,
                    self.chrome.animations != Some(false),
                ),
            );
        }
    }

    fn remember_compaction_turns(&mut self, page: &HistoryPage) {
        for message in &page.rows {
            if let Some(turn) = &message.turn {
                self.compaction_turn_messages
                    .insert(turn.id.clone(), message.id.0.clone());
            }
        }
        while self.compaction_turn_messages.len() > crate::history::WINDOW_ROWS {
            self.compaction_turn_messages.pop_first();
        }
    }

    /// Styled transcript lines wrapped to the content-box `width`
    /// (upstream row model: user block, assistant markdown, collapsed
    /// reasoning, assistant footer). `width == 0` is the unbounded text
    /// projection used for scroll metrics and plain-text assertions.
    pub fn transcript_lines(&self, width: u16, terminal_width: u16) -> Vec<Line> {
        let theme = Theme::dark();
        crate::messages::transcript_with_expansion(
            &self.transcript_rows(),
            theme,
            width,
            terminal_width,
            |agent| self.agent_color(agent),
            Some(&self.markdown_cache),
            &|op| self.exploration_expanded.contains(op),
        )
    }

    /// Bounded wrapped content, including the scrollbox's top padding. Padding
    /// scrolls away with long history; short history starts below that one row.
    pub fn rendered_transcript(&self, width: u16, terminal_width: u16) -> Vec<Line> {
        let mut lines = vec![Line::plain("")];
        lines.extend(crate::styled::wrap_lines(
            &self.transcript_lines(width, terminal_width),
            width as usize,
        ));
        lines
    }

    /// Materialize no more than the visible viewport, counting bounded parts
    /// through the per-session Markdown cache instead of building all rows.
    pub fn visible_transcript(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (Vec<Line>, usize) {
        let (lines, total, _) = self.visible_transcript_at_viewport(width, terminal_width, height);
        (lines, total)
    }

    /// Resolve a resize against the last painted top row without changing
    /// the input-owned scroll request or materializing the whole transcript.
    pub(crate) fn visible_transcript_at_viewport(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (Vec<Line>, usize, usize) {
        let (lines, total, scroll, _) =
            self.visible_transcript_at_viewport_with_targets(width, terminal_width, height);
        (lines, total, scroll)
    }

    pub(crate) fn visible_transcript_at_viewport_with_targets(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (
        Vec<Line>,
        usize,
        usize,
        Vec<Option<crate::messages::UserMessageTarget>>,
    ) {
        let theme = Theme::dark();
        if let Some(view) = self.visible_projection.borrow().as_ref().filter(|view| {
            view.revision == self.transcript_revision
                && view.window_revision == self.window.revision()
                && view.viewport == self.viewport.get()
                && view.live_lengths == (self.live_text.len(), self.live_reasoning.len())
                && view.dimensions == (width, terminal_width, height)
                && view.requested_scroll == self.scroll
                && view.chrome == self.chrome
                && view.theme == theme.mode()
                && view.thinking == self.thinking_expanded
                && view.reasoning == self.reasoning_expanded
                && view.exploration == self.exploration_expanded
                && view.compaction_frame == self.compaction_frame
        }) {
            return (
                view.lines.clone(),
                view.total,
                view.scroll,
                view.targets.clone(),
            );
        }
        #[cfg(test)]
        self.visible_projection_builds
            .set(self.visible_projection_builds.get() + 1);
        let rows = self.transcript_rows();
        let live_row = (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| {
            rows.len() - 1 - usize::from(self.active_turn.is_some() && self.live_preview_truncated)
        });
        let render = |height, scroll| {
            crate::messages::visible_transcript_user_targets(
                &rows,
                Theme::dark(),
                (width, terminal_width),
                (height, scroll, live_row),
                |agent| self.agent_color(agent),
                &self.markdown_cache,
                &|op| self.exploration_expanded.contains(op),
            )
        };
        let previous = self.viewport.get();
        let resized = previous.is_some_and(|view| {
            (view.width, view.terminal_width, view.height) != (width, terminal_width, height)
        });
        let semantic_scroll = self
            .completion_anchor
            .borrow()
            .as_ref()
            .filter(|anchor| {
                anchor.requested_scroll == self.scroll
                    && self.scroll > 0
                    && (anchor.pending || resized)
            })
            .and_then(|anchor| {
                let index = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.message_id.as_ref() == Some(&anchor.message))
                    .nth(anchor.part)
                    .map(|(index, _)| index)?;
                let mut top = None;
                let total = self.transcript_part_positions(
                    &rows,
                    (width, terminal_width),
                    |part, start, end| {
                        if part == index && end > start {
                            top = Some(start + anchor.row.min(end - start - 1));
                        }
                    },
                );
                top.map(|top| total.saturating_sub(height as usize).saturating_sub(top))
            });
        let scroll = if self.scroll == 0 {
            0
        } else if let Some(scroll) = semantic_scroll {
            scroll
        } else if resized && previous.is_some_and(|view| view.requested_scroll == self.scroll) {
            let view = previous.expect("resized viewport");
            // Count-only indexing is needed on resize, not on every draw.
            let (_, total, _) = render(0, 0);
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            total.saturating_sub(height as usize).saturating_sub(top)
        } else if previous.is_some_and(|view| view.requested_scroll == self.scroll) {
            self.display_scroll()
        } else {
            self.scroll
        };
        let (mut lines, total, mut targets) = render(height as usize, scroll);
        // A bottom-relative offset alone follows appended rows even when the
        // reader detached. Preserve the last painted top row during live tail
        // growth. Prepending deliberately clears the viewport: its new rows
        // precede that anchor, so the bottom-relative offset already holds it.
        let scroll = if self.scroll > 0
            && semantic_scroll.is_none()
            && !resized
            && let Some(view) = previous.filter(|view| view.requested_scroll == self.scroll)
            && view.total != total
        {
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            let anchored = total.saturating_sub(height as usize).saturating_sub(top);
            (lines, _, targets) = render(height as usize, anchored);
            anchored
        } else {
            scroll
        };
        let scroll = scroll.min(total.saturating_sub(height as usize));
        *self.visible_projection.borrow_mut() = Some(VisibleTranscriptProjection {
            revision: self.transcript_revision,
            window_revision: self.window.revision(),
            viewport: Some(TranscriptViewport {
                width,
                terminal_width,
                height,
                total,
                requested_scroll: self.scroll,
                displayed_scroll: scroll,
            }),
            live_lengths: (self.live_text.len(), self.live_reasoning.len()),
            dimensions: (width, terminal_width, height),
            requested_scroll: self.scroll,
            chrome: self.chrome.clone(),
            theme: theme.mode(),
            thinking: self.thinking_expanded,
            reasoning: self.reasoning_expanded.clone(),
            exploration: self.exploration_expanded.clone(),
            compaction_frame: self.compaction_frame,
            lines: lines.clone(),
            total,
            scroll,
            targets: targets.clone(),
        });
        (lines, total, scroll, targets)
    }

    fn invalidate_transcript(&mut self) {
        self.transcript_revision = self.transcript_revision.wrapping_add(1);
        self.visible_projection.get_mut().take();
    }

    /// Categorical agent color (`context/local.tsx:75-133`): the agent's index
    /// in the generation's full admitted agent list (the catalog pins the slot),
    /// and the first categorical color for an
    /// unknown/missing agent.
    pub fn agent_color(&self, agent: Option<&str>) -> ratatui::style::Color {
        let colors = Theme::dark().categorical_agents();
        let index = agent.and_then(|id| {
            self.agents
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| entry.color_index)
        });
        match index {
            Some(index) => colors[index % colors.len()],
            None => colors[0],
        }
    }

    // ---- snapshots from the binary -------------------------------------

    /// Apply a catalog snapshot: picker, agents and the effective selection.
    pub fn apply_catalog(&mut self, snapshot: CatalogSnapshot) {
        self.invalidate_transcript();
        if self.chrome.conversation_shortcuts.leader
            != snapshot.chrome.conversation_shortcuts.leader
            || self.chrome.leader_timeout_ms() != snapshot.chrome.leader_timeout_ms()
            || self.chrome.command_palette_shortcut != snapshot.chrome.command_palette_shortcut
        {
            self.leader = None;
        }
        self.set_conversation_shortcuts(
            Some(snapshot.chrome.conversation_shortcuts.undo.clone()),
            Some(snapshot.chrome.conversation_shortcuts.redo.clone()),
        );
        if self.chrome.location != snapshot.chrome.location {
            self.generation += 1;
            self.clear_mentions();
        }
        self.apply_owner_clipboard_mode(snapshot.chrome.terminal_copy);
        self.chrome = snapshot.chrome.clone();
        self.auto_accept = snapshot.auto_accept;
        let mut picker = ModelPicker::new(catalog_from_snapshot(&snapshot));
        if !snapshot.model_id.is_empty() {
            let record = serde_json::json!({
                "provider": snapshot.provider,
                "id": snapshot.model_id,
                "variant": snapshot.variant,
            });
            picker.load_persisted_raw(Some(&record.to_string()));
            picker.focus_id(&snapshot.model_id);
        }
        if let Some(error) = picker.last_error() {
            self.push_note(error);
        }
        self.picker = Some(picker);
        self.commands = snapshot.commands;
        self.command_descriptions = snapshot.command_descriptions;
        self.active_agent = snapshot.agent_id.clone();
        self.agents = snapshot.agents;
        self.agents_cursor = snapshot
            .agent_id
            .as_ref()
            .and_then(|id| self.agents.iter().position(|agent| &agent.id == id))
            .unwrap_or(0);
        self.catalog_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply the session list snapshot.
    pub fn apply_sessions(&mut self, sessions: Vec<String>) {
        self.session_entries.clear();
        self.sessions = sessions;
        self.sessions_cursor = 0;
        self.sessions_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply the bounded application-owned root metadata page.
    pub fn apply_session_entries(&mut self, entries: Vec<oc_core::queries::SessionListEntry>) {
        self.sessions = entries.iter().map(|entry| entry.id.0.clone()).collect();
        self.session_entries = entries;
        self.sessions_loaded = true;
        if self.select.query.is_empty() {
            self.select.cursor = self
                .sessions
                .iter()
                .position(|id| {
                    self.attached_session()
                        .is_some_and(|current| &current.0 == id)
                })
                .unwrap_or(0);
            self.select.follow_selection();
        }
        self.sync_modal_cursor();
    }

    pub fn session_search(&self) -> String {
        self.select.query.clone()
    }

    pub fn sessions_all_projects(&self) -> bool {
        self.sessions_all_projects
    }

    pub fn session_entries(&self) -> &[oc_core::queries::SessionListEntry] {
        &self.session_entries
    }

    pub fn sessions_title(&self) -> String {
        if !self.sessions_all_projects
            && let Some(name) = &self.session_project_name
        {
            return format!("Sessions for {name}");
        }
        "Sessions".into()
    }

    pub fn session_scope_update(&mut self) -> Option<bool> {
        self.session_scope_pending.take()
    }

    pub fn apply_session_picker_context(
        &mut self,
        context: oc_core::queries::SessionPickerContext,
    ) {
        self.sessions_all_projects = context.all_projects;
        self.session_project_name = context.project_name;
    }

    pub fn selected_session_rename(&self) -> Option<&str> {
        self.rename_selected.as_deref()
    }

    pub fn selected_session_renamed(&mut self, id: &str, title: String) {
        if self
            .attached_session()
            .is_some_and(|session| session.0 == id)
        {
            self.session_title = Some(title.clone());
        }
        if self.rename_selected.as_deref() == Some(id) {
            self.close_panel();
        }
    }

    pub fn session_delete_rejected(&mut self, message: String) {
        self.session_delete_confirm = None;
        self.apply_intent_error(message);
    }

    /// Apply the skill card snapshot (bodies never reach the view).
    pub fn apply_skills(&mut self, cards: Vec<SkillCard>) {
        self.skills = cards;
        self.skills_cursor = 0;
        self.skills_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply a DCP context/stats snapshot.
    pub fn apply_dcp_snapshot(&mut self, snapshot: DcpSnapshot) {
        self.dcp.set_snapshot(snapshot);
    }

    /// Apply a newest-first tool-card page; rendered as bounded rows.
    pub fn apply_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.card_ops = cards.iter().map(|card| card.op.clone()).collect();
        self.cards = cards.iter().map(card_row).collect();
        self.cards_cursor = 0;
        self.cards_loaded = true;
        self.cards_has_older = has_older;
    }

    /// Prepend an older tool-card page (paging up in the Cards panel).
    pub fn prepend_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        let mut ops: Vec<String> = cards.iter().map(|card| card.op.clone()).collect();
        ops.append(&mut self.card_ops);
        ops.truncate(CARDS_MAX);
        self.card_ops = ops;
        let mut rows: Vec<HistoryRow> = cards.iter().map(card_row).collect();
        rows.append(&mut self.cards);
        rows.truncate(CARDS_MAX);
        self.cards = rows;
        self.cards_has_older = has_older;
    }

    /// True when older tool cards exist before the loaded page.
    pub fn cards_need_older(&self) -> bool {
        self.cards_has_older
    }

    /// Replace a single bounded output page. The text remains application-owned;
    /// this preview disappears when the panel/session is closed.
    pub fn apply_card_output(
        &mut self,
        op: String,
        offset: usize,
        page: oc_core::queries::ToolOutputPage,
    ) {
        if self.panel == TuiPanel::Cards && self.card_ops.contains(&op) {
            self.card_output = Some(CardOutput { op, offset, page });
            self.card_scroll = 0;
            self.card_seen.set(0);
            self.select.reset();
        }
    }

    /// Handle a bracketed paste as one bounded event (never per-char).
    pub fn handle_paste(&mut self, text: &str) -> KeyOutcome {
        if self.approvals.active().is_some() {
            self.approvals.paste(text);
            return KeyOutcome::default();
        }
        use unicode_segmentation::UnicodeSegmentation as _;
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        if self.panel != TuiPanel::None {
            if self.panel == TuiPanel::Rename {
                return self.paste_rename(text);
            }
            let room = 512_usize.saturating_sub(self.select.query.len());
            let mut kept = 0;
            for grapheme in text.graphemes(true) {
                if kept + grapheme.len() > room {
                    break;
                }
                if !grapheme.chars().any(char::is_control) {
                    self.select.query.push_str(grapheme);
                    kept += grapheme.len();
                }
            }
            self.changed_modal_query();
            return KeyOutcome {
                note: (text.len() > kept).then(|| "modal search truncated at 512 bytes".into()),
                intent: (self.panel == TuiPanel::Sessions).then_some(PanelIntent::LoadSessions),
                ..KeyOutcome::default()
            };
        }
        let mut clean = String::with_capacity(text.len().min(MAX_INPUT_BYTES));
        let mut exceeded = false;
        for grapheme in text.graphemes(true) {
            let safe = match grapheme {
                "\r\n" | "\r" => "\n",
                "\t" => " ",
                _ if grapheme.chars().any(char::is_control) && grapheme != "\n" => continue,
                _ => grapheme,
            };
            if clean.len() + safe.len() > MAX_INPUT_BYTES {
                exceeded = true;
                break;
            }
            clean.push_str(safe);
        }
        let chip_count = self.editor.chip_count();
        let paste = if exceeded {
            self.editor
                .paste_clipped(&mut self.input, &clean, MAX_INPUT_BYTES)
        } else {
            self.editor.paste(&mut self.input, &clean, MAX_INPUT_BYTES)
        };
        if paste.expanded {
            self.input_revision += 1;
            self.slash_selected = 0;
            self.mention_selected = 0;
            return KeyOutcome::default();
        }
        let dropped = text.len().saturating_sub(paste.inserted);
        if paste.inserted > 0 {
            self.input_revision += 1;
            self.slash_selected = 0;
            self.mention_selected = 0;
        }
        let mut notes = Vec::new();
        if exceeded || clean.len() > paste.inserted + paste.trimmed {
            notes.push(format!(
                "paste truncated: {dropped} bytes dropped at the {MAX_INPUT_BYTES} byte input limit"
            ));
        } else if text.len() > clean.len() {
            notes.push(format!(
                "paste filtered: {} control/newline-normalization bytes",
                text.len() - clean.len()
            ));
        }
        if paste.trimmed > 0 {
            notes.push(format!(
                "paste chip trimmed: {} surrounding whitespace bytes removed",
                paste.trimmed
            ));
        }
        if paste.inserted > 0
            && chip_count == crate::editor::MAX_PASTE_CHIPS
            && crate::editor::chip_worthy(&clean[..paste.inserted + paste.trimmed]).is_some()
        {
            notes
                .push("paste display chip limit reached; full pasted text remains in draft".into());
        }
        KeyOutcome {
            note: (!notes.is_empty()).then(|| notes.join("; ")),
            ..KeyOutcome::default()
        }
    }

    /// Report a runtime DCP outcome: transient notice, never chat history.
    pub fn notify_dcp(&mut self, outcome: DcpOutcome) {
        let failure = matches!(outcome, DcpOutcome::Failed { .. });
        self.dcp.set_outcome(outcome);
        if (failure || self.chrome.dcp.notification != oc_core::dcp_view::DcpNotificationMode::Off)
            && let Some(notice) = self.dcp.notice().map(str::to_string)
        {
            self.push_transient_note(
                &notice,
                if failure {
                    NoteVariant::Error
                } else {
                    NoteVariant::Info
                },
            );
        }
        self.dcp.clear_notice();
    }

    /// Query only committed block IDs retained by this view, with a hard batch
    /// bound. Disabled/minimal/toast views never request summary contents.
    pub fn dcp_summary_requests(&self) -> Vec<(String, usize)> {
        use oc_core::dcp_view::{DcpNotificationChannel, DcpNotificationMode};
        if !self.chrome.dcp.show_compression
            || self.chrome.dcp.notification != DcpNotificationMode::Detailed
            || self.chrome.dcp.channel != DcpNotificationChannel::Chat
        {
            return Vec::new();
        }
        self.transcript_rows()
            .into_iter()
            .rev()
            .filter_map(|row| row.tool)
            .flat_map(|card| {
                let crate::tools::ToolRender::Dcp(view) = card.render else {
                    return Vec::new();
                };
                let Some(run) = view.snapshot else {
                    return Vec::new();
                };
                run.block_ids
                    .into_iter()
                    .enumerate()
                    .filter(|(_, id)| !view.summaries.iter().any(|page| &page.block_id == id))
                    .map(|(index, _)| (card.op.clone(), index))
                    .collect::<Vec<_>>()
            })
            .take(32)
            .collect()
    }

    pub fn apply_dcp_summary(
        &mut self,
        session: &SessionId,
        op: &str,
        page: oc_core::dcp_view::DcpSummaryPage,
    ) {
        if self.session.as_ref() != Some(session) {
            return;
        }
        self.invalidate_transcript();
        if self.window.apply_dcp_summary(op, page.clone()) {
            return;
        }
        for part in &mut self.live_parts {
            if let LivePart::Tool { card, .. } = part
                && card.op == op
                && let crate::tools::ToolRender::Dcp(view) = &mut card.render
            {
                view.apply_summary(page);
                self.enforce_parts();
                return;
            }
        }
    }

    /// Report that an intent could not be applied; the input is kept so the
    /// user can retry or edit it.
    pub fn apply_intent_error(&mut self, message: String) {
        self.push_note(&message);
    }

    /// Current modal value, distinct from the prompt draft and its caret.
    pub fn rename_title(&self) -> Option<&str> {
        (self.panel == TuiPanel::Rename).then_some(self.rename_input.as_str())
    }

    pub(crate) fn rename_cursor(&self) -> usize {
        self.rename_editor.cursor
    }

    /// Call only after the application owner has accepted the title update.
    /// This also updates the active tab until the next owner deck snapshot.
    pub fn rename_session_applied(&mut self, title: String) {
        if self.panel != TuiPanel::Rename || self.rename_pending.as_deref() != Some(&title) {
            return;
        }
        self.session_title = Some(title.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.title = Some(title);
        }
        self.close_panel();
        self.note = None;
    }

    /// Owner refusal retains the focused title and prompt draft for retry.
    pub fn rename_session_rejected(&mut self, message: String) {
        if self.panel == TuiPanel::Rename {
            self.rename_pending = None;
            self.apply_intent_error(message);
        }
    }

    /// ACK a direct `/rename <title>` only when its matching owner request completed.
    /// Preserve edits made to the composer while the owner was working.
    pub fn rename_session_direct_applied(&mut self, title: String) {
        let Some((pending, revision)) = self.rename_direct_pending.take() else {
            return;
        };
        if pending != title {
            self.rename_direct_pending = Some((pending, revision));
            return;
        }
        self.session_title = Some(title.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.title = Some(title);
        }
        if self.input_revision == revision {
            self.accept_intent();
            self.input_revision += 1;
        }
        self.note = None;
    }

    /// A failed direct request leaves the original slash draft available for correction.
    pub fn rename_session_direct_rejected(&mut self, message: String) {
        if self.rename_direct_pending.take().is_some() {
            self.apply_intent_error(message);
        }
    }

    /// Resolve only the matching slash request, preserving edits typed while
    /// the provider was running. A completion on another tab is not applied.
    pub fn regenerated_title(&mut self, result: Result<String, String>) {
        let Some(revision) = self.regenerate_pending.take() else {
            return;
        };
        match result {
            Ok(title) => {
                self.session_title = Some(title.clone());
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.title = Some(title);
                }
                if self.input_revision == revision {
                    self.accept_intent();
                    self.input_revision += 1;
                }
                self.note = None;
            }
            Err(message) => self.apply_intent_error(message),
        }
    }

    fn paste_rename(&mut self, text: &str) -> KeyOutcome {
        use unicode_segmentation::UnicodeSegmentation as _;
        if self.rename_pending.is_some() {
            return KeyOutcome::default();
        }
        let mut clean = String::new();
        let mut clipped = false;
        for grapheme in text.graphemes(true) {
            let safe = match grapheme {
                "\r\n" | "\n" | "\r" | "\t" => " ",
                _ if grapheme.chars().any(char::is_control) => continue,
                _ => grapheme,
            };
            if clean.len() + safe.len() > MAX_SESSION_TITLE_BYTES {
                clipped = true;
                break;
            }
            clean.push_str(safe);
        }
        let inserted =
            self.rename_editor
                .replace(&mut self.rename_input, &clean, MAX_SESSION_TITLE_BYTES);
        KeyOutcome {
            note: (clipped || inserted < clean.len()).then(|| self.rename_limit_note()),
            ..KeyOutcome::default()
        }
    }

    fn rename_limit_note(&self) -> String {
        if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
            format!(
                "session title exceeds {MAX_SESSION_TITLE_BYTES} bytes; shorten it or select all to replace"
            )
        } else {
            format!("session title truncated at {MAX_SESSION_TITLE_BYTES} bytes")
        }
    }

    /// Report that an intent was accepted and applied; clears the input.
    pub fn accept_intent(&mut self) {
        self.input.clear();
        self.editor.clear();
    }

    /// The accepted compress turn starts streaming: status, turn, DCP panel.
    pub fn begin_compress_turn(&mut self, turn: WorkerTurnId) {
        self.invalidate_transcript();
        self.reset_scanner();
        self.live_preview_truncated = false;
        self.live_part_states.clear();
        self.live_terminal_status = None;
        self.live_model_label = None;
        self.live_agent_color_index = None;
        self.compress_turn = Some(turn.clone());
        self.active_turn = Some(turn);
        self.reasoning_epoch = self.reasoning_epoch.wrapping_add(1);
        self.status = TuiStatus::Streaming;
        self.panel = TuiPanel::Dcp;
        self.clear_mouse_position();
        self.input.clear();
        self.editor.clear();
        self.live_text.clear();
        self.live_reasoning.clear();
        self.live_parts.clear();
        self.live_part_offset = 0;
        self.reasoning_down = None;
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.scroll = 0;
        self.wheel_motion = None;
    }

    /// Whether the active turn was accepted from a manual compression request.
    pub fn is_compress_turn(&self, turn: &WorkerTurnId) -> bool {
        self.compress_turn.as_ref() == Some(turn) && self.active_turn.as_ref() == Some(turn)
    }

    /// Enqueue manual compression using the same draft/receipt lifecycle as text.
    pub fn request_compress(&mut self, focus: String) -> Result<(), CoreError> {
        if self.is_busy() {
            return Err(CoreError::TurnBusy);
        }
        let session = self.session.clone().ok_or(CoreError::SessionNotFound)?;
        let receipt = self.app.request_compress(session.clone(), focus)?;
        self.begin_submission(receipt, session, false, true);
        Ok(())
    }

    fn begin_submission(
        &mut self,
        receipt: SubmissionReceipt,
        session: SessionId,
        fresh: bool,
        compress: bool,
    ) {
        self.request_id += 1;
        let agent = self.active_agent.clone();
        let agent_color_index = agent.as_deref().and_then(|agent| {
            self.agents
                .iter()
                .find(|entry| entry.id == agent)
                .map(|entry| entry.color_index)
        });
        self.pending = Some(PendingSubmission {
            request_id: self.request_id,
            generation: self.generation,
            session,
            fresh,
            draft: self.input.clone(),
            mentions: self.editor.submitted_mentions(&self.input),
            revision: self.input_revision,
            receipt,
            agent,
            agent_color_index,
            cancelling: false,
            compress,
        });
        self.status = TuiStatus::PendingSubmission;
        self.push_note("submission pending; Esc to cancel");
    }

    /// Set a persistent legacy status note until replaced or cleared.
    pub fn push_note(&mut self, note: &str) {
        self.push_note_variant(note, NoteVariant::Warning);
    }

    /// Set a typed feedback note without interpreting its free-form text.
    pub fn push_note_variant(&mut self, note: &str, variant: NoteVariant) {
        self.note = Some((note.to_string(), variant));
        self.toast_expiry = None;
        self.toast_down = false;
    }

    /// Timed feedback uses the upstream default five-second toast lifetime;
    /// legacy warnings continue to persist until replaced or explicitly cleared.
    pub fn push_transient_note(&mut self, note: &str, variant: NoteVariant) {
        self.push_transient_note_for(note, variant, Duration::from_secs(5));
    }

    /// Explicit duration for long-running owner operations such as reload.
    pub fn push_transient_note_for(
        &mut self,
        note: &str,
        variant: NoteVariant,
        duration: Duration,
    ) {
        self.push_transient_note_at(note, variant, duration, Instant::now());
    }

    fn push_transient_note_at(
        &mut self,
        note: &str,
        variant: NoteVariant,
        duration: Duration,
        now: Instant,
    ) {
        self.push_note_variant(note, variant);
        self.toast_expiry = Some(ToastExpiry {
            remaining: duration,
            started: Some(now),
        });
    }

    pub fn tick_toast(&mut self, now: Instant) {
        if self.interrupt_armed_until.is_some_and(|until| now >= until) {
            self.interrupt_armed_until = None;
        }
        if self.toast_expiry.as_ref().is_some_and(|expiry| {
            expiry
                .started
                .is_some_and(|started| now.saturating_duration_since(started) >= expiry.remaining)
        }) {
            self.note = None;
            self.toast_expiry = None;
            self.toast_down = false;
        }
    }

    fn set_toast_hover(&mut self, hovered: bool, now: Instant) {
        let Some(expiry) = &mut self.toast_expiry else {
            return;
        };
        if hovered {
            if let Some(started) = expiry.started.take() {
                expiry.remaining = expiry
                    .remaining
                    .saturating_sub(now.saturating_duration_since(started));
            }
        } else if expiry.started.is_none() {
            expiry.started = Some(now);
        }
    }

    /// Push one synthetic transcript row for a non-fatal warning, so a
    /// degraded capability stays visible after the status note is replaced.
    pub fn push_warning(&mut self, warning: &str) {
        self.window
            .push_synthetic("", &format!("(warning: {warning})"), None, None);
    }

    /// Model under the picker cursor; the active variant is preserved when
    /// the cursor still points at the selected model.
    pub fn picker_selection(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        let id = picker.cursor_id()?;
        let variant = picker
            .selection()
            .filter(|selection| selection.id == id)
            .and_then(|selection| selection.variant.as_ref())
            .map(|variant| variant.name.clone());
        Some((id, variant))
    }

    /// Effective model id and variant name from the catalog snapshot (the
    /// resolved selection, never the picker cursor).
    pub fn active_model_label(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        if let crate::picker::PickerState::Retired { wanted, .. } = picker.state() {
            return Some((format!("{wanted} (unavailable)"), None));
        }
        let selection = picker.selection()?;
        Some((
            selection
                .entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&selection.id)
                .to_string(),
            selection
                .variant
                .as_ref()
                .map(|variant| variant.name.clone())
                .or_else(|| {
                    picker
                        .retired_variant()
                        .map(|name| format!("{name} (unavailable)"))
                }),
        ))
    }

    /// Provider id of the loaded catalog, if any.
    pub fn active_provider(&self) -> Option<&str> {
        let picker = self.picker.as_ref()?;
        Some(
            picker
                .selection()
                .and_then(|s| s.entry.get("provider_name"))
                .and_then(|v| v.as_str())
                .unwrap_or(picker.provider()),
        )
    }

    /// Effective agent id from the catalog snapshot, if any.
    pub fn active_agent(&self) -> Option<&str> {
        self.active_agent.as_deref()
    }

    /// True when the input names a workspace command (template expanded by
    /// the application, never by the view).
    pub fn is_workspace_command(&self, text: &str) -> bool {
        let Some(rest) = text.strip_prefix('/') else {
            return false;
        };
        let name = rest.split_whitespace().next().unwrap_or_default();
        self.commands.iter().any(|id| id == name)
    }

    /// Agent id under the Agents cursor, if any.
    pub fn selected_agent(&self) -> Option<String> {
        self.agents
            .get(self.agents_cursor)
            .map(|agent| agent.id.clone())
    }

    /// Sessions cursor position.
    pub fn sessions_cursor(&self) -> usize {
        self.sessions_cursor
    }

    // ---- key handling ---------------------------------------------------

    /// Handle one key action. The returned [`KeyOutcome`] tells the binary
    /// whether to display a note, apply an intent, or treat the input as
    /// consumed.
    pub async fn handle_key(&mut self, action: KeyAction) -> KeyOutcome {
        let mut action = action;
        if self.approvals.active().is_some() {
            return self.approvals.key(action);
        }
        self.poll_submission();
        if self
            .leader_deadline()
            .is_some_and(|until| Instant::now() >= until)
        {
            self.leader = None;
        }
        if action == KeyAction::Quit {
            self.leader = None;
        }
        if self.leader.take().is_some() {
            let key = match &action {
                KeyAction::SequenceKey(key, _) => key.clone(),
                KeyAction::Char(key) => key.to_string(),
                KeyAction::Cancel => "esc".into(),
                KeyAction::Backspace => "backspace".into(),
                _ => String::new(),
            };
            let binding = format!("{} {key}", self.leader_key);
            let command = if self.panel == TuiPanel::None {
                [
                    CommandAction::UndoConversation,
                    CommandAction::RedoConversation,
                ]
                .into_iter()
                .find(|action| {
                    self.conversation_shortcut(*action == CommandAction::UndoConversation)
                        .split(',')
                        .any(|value| value.trim().eq_ignore_ascii_case(&binding))
                })
                .or_else(|| {
                    self.chrome
                        .command_palette_shortcut
                        .as_ref()
                        .filter(|shortcut| {
                            shortcut
                                .split(',')
                                .any(|value| value.trim().eq_ignore_ascii_case(&binding))
                        })
                        .map(|_| CommandAction::OpenCommands)
                })
                .or_else(|| {
                    crate::commands::REGISTRY
                        .iter()
                        .filter(|c| {
                            !matches!(
                                c.action,
                                CommandAction::UndoConversation | CommandAction::RedoConversation
                            )
                        })
                        .find(|c| {
                            self.command_shortcuts(c)
                                .iter()
                                .any(|shortcut| shortcut.eq_ignore_ascii_case(&binding))
                        })
                        .map(|c| c.action.clone())
                })
            } else {
                None
            };
            if let Some(command) = command {
                return self.run_command(command);
            }
            // Actual pinned original: sequence miss clears pending, then an
            // unmatched printable reaches the focused composer (wzord oracle).
            action = match action {
                KeyAction::SequenceKey(_, Some(value)) | KeyAction::Char(value) => {
                    KeyAction::Char(value)
                }
                KeyAction::SequenceKey(ref key, _) if key == "enter" => KeyAction::Enter,
                KeyAction::Enter => KeyAction::Enter,
                _ => return KeyOutcome::default(),
            };
        }
        if self.panel != TuiPanel::None {
            if action == KeyAction::Leader {
                self.leader = Some(Instant::now());
                return KeyOutcome::default();
            }
            return self.handle_panel_key(action);
        }
        let action = if action == KeyAction::CtrlA {
            KeyAction::Home
        } else {
            action
        };
        if let Some(options) = self.slash_options() {
            match action {
                KeyAction::Up | KeyAction::Commands => {
                    if !options.is_empty() {
                        self.slash_selected = (self.slash_selected(options.len()) + options.len()
                            - 1)
                            % options.len();
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Down => {
                    if !options.is_empty() {
                        self.slash_selected =
                            (self.slash_selected(options.len()) + 1) % options.len();
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Tab => return self.select_slash(false).await,
                KeyAction::Enter => return self.select_slash(true).await,
                KeyAction::Cancel => {
                    self.slash_dismissed = Some(self.input_revision);
                    return KeyOutcome::default();
                }
                KeyAction::Interrupt => {
                    // autocomplete.tsx:744-759: prompt.clear hides the command
                    // menu and removes only the trigger-to-caret token.
                    let caret = self.editor.cursor;
                    self.editor.move_to(0, false);
                    self.editor.move_to(caret, true);
                    if self.editor.delete(&mut self.input, true, false) {
                        self.input_revision += 1;
                    }
                    self.slash_selected = 0;
                    self.slash_dismissed = Some(self.input_revision);
                    self.leader = None;
                    return KeyOutcome::default();
                }
                _ => {}
            }
        }
        if let Some(options) = self.mention_options() {
            let count = options.paths.len();
            match action {
                KeyAction::Up | KeyAction::Commands => {
                    if count > 0 {
                        self.mention_selected = (self.mention_selected(count) + count - 1) % count;
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Down => {
                    if count > 0 {
                        self.mention_selected = (self.mention_selected(count) + 1) % count;
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Tab => {
                    self.select_mention();
                    return KeyOutcome::default();
                }
                KeyAction::Cancel => {
                    self.mention_dismissed = self.mention_request();
                    return KeyOutcome::default();
                }
                KeyAction::Interrupt => {
                    // Reference autocomplete hides without deleting @query.
                    self.mention_dismissed = self.mention_request();
                    self.leader = None;
                    return KeyOutcome::default();
                }
                _ => {}
            }
        } else if let Some(request) = self.mention_request() {
            match action {
                KeyAction::Cancel | KeyAction::Interrupt => {
                    self.mention_dismissed = Some(request);
                    self.leader = None;
                    return KeyOutcome::default();
                }
                KeyAction::Up | KeyAction::Down | KeyAction::Commands | KeyAction::Tab => {
                    return KeyOutcome::default();
                }
                _ => {}
            }
        }
        if matches!(
            action,
            KeyAction::Cancel
                | KeyAction::Interrupt
                | KeyAction::Quit
                | KeyAction::Commands
                | KeyAction::Agents
                | KeyAction::CycleVariant
                | KeyAction::Rename
                | KeyAction::UndoConversation
                | KeyAction::RedoConversation
        ) {
            self.leader = None;
        }
        match action {
            KeyAction::Commands => self.run_command(CommandAction::OpenCommands),
            KeyAction::UndoConversation => self.run_command(CommandAction::UndoConversation),
            KeyAction::RedoConversation => self.run_command(CommandAction::RedoConversation),
            KeyAction::Agents => self.run_command(CommandAction::OpenAgents),
            KeyAction::CycleVariant if self.is_busy() => KeyOutcome {
                note: Some("turn active; action unavailable".into()),
                ..Default::default()
            },
            KeyAction::CycleVariant => KeyOutcome {
                intent: Some(PanelIntent::CycleVariant),
                ..Default::default()
            },
            KeyAction::Rename => self.run_command(CommandAction::RenameSession { title: None }),
            KeyAction::Leader => {
                self.leader = Some(Instant::now());
                KeyOutcome::default()
            }
            KeyAction::SequenceKey(_, _) => KeyOutcome::default(),
            KeyAction::Left
            | KeyAction::Right
            | KeyAction::WordLeft
            | KeyAction::WordRight
            | KeyAction::SelectLeft
            | KeyAction::SelectRight
            | KeyAction::SelectWordLeft
            | KeyAction::SelectWordRight => {
                let right = matches!(
                    action,
                    KeyAction::Right
                        | KeyAction::WordRight
                        | KeyAction::SelectRight
                        | KeyAction::SelectWordRight
                );
                let word = matches!(
                    action,
                    KeyAction::WordLeft
                        | KeyAction::WordRight
                        | KeyAction::SelectWordLeft
                        | KeyAction::SelectWordRight
                );
                let select = matches!(
                    action,
                    KeyAction::SelectLeft
                        | KeyAction::SelectRight
                        | KeyAction::SelectWordLeft
                        | KeyAction::SelectWordRight
                );
                self.editor.horizontal(&self.input, right, word, select);
                KeyOutcome::default()
            }
            KeyAction::Home | KeyAction::CtrlA | KeyAction::End => {
                self.editor
                    .line_edge(&self.input, action == KeyAction::End, false);
                KeyOutcome::default()
            }
            KeyAction::SelectHome | KeyAction::SelectEnd => {
                self.editor.move_to(
                    if action == KeyAction::SelectHome {
                        0
                    } else {
                        self.input.len()
                    },
                    true,
                );
                KeyOutcome::default()
            }
            KeyAction::SelectUp | KeyAction::SelectDown => {
                self.prompt_vertical(action == KeyAction::SelectDown, true);
                KeyOutcome::default()
            }
            KeyAction::PageUp | KeyAction::PageDown => KeyOutcome::default(),
            KeyAction::Char(c) => {
                if self
                    .editor
                    .replace(&mut self.input, &c.to_string(), MAX_INPUT_BYTES)
                    > 0
                {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.mention_selected = 0;
                    KeyOutcome::default()
                } else {
                    KeyOutcome {
                        note: Some(format!(
                            "input limit {MAX_INPUT_BYTES} bytes reached; the key was not added"
                        )),
                        ..KeyOutcome::default()
                    }
                }
            }
            KeyAction::Backspace => {
                if self.editor.delete(&mut self.input, true, false) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.mention_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::DeleteOrQuit if self.input.is_empty() && !self.is_busy() => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Delete
            | KeyAction::DeleteOrQuit
            | KeyAction::WordBackspace
            | KeyAction::WordDelete => {
                if self.editor.delete(
                    &mut self.input,
                    action == KeyAction::WordBackspace,
                    action != KeyAction::Delete,
                ) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Newline => {
                if self.editor.replace(&mut self.input, "\n", MAX_INPUT_BYTES) > 0 {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Undo | KeyAction::Redo => {
                if self.editor.undo(&mut self.input, action == KeyAction::Redo) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Up => {
                if self.prompt_vertical(false, false) {
                    return KeyOutcome::default();
                }
                if self.recall_history(true) {
                    return KeyOutcome::default();
                }
                self.scroll_transcript(true)
            }
            KeyAction::Down => {
                if self.prompt_vertical(true, false) {
                    return KeyOutcome::default();
                }
                if self.recall_history(false) {
                    return KeyOutcome::default();
                }
                self.scroll_transcript(false)
            }
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Interrupt => {
                if self.input.is_empty() {
                    self.status = TuiStatus::Quit;
                } else {
                    self.input.clear();
                    self.editor.clear();
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.slash_dismissed = None;
                    self.clear_mentions();
                }
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.is_busy() {
                    // Pending submission cancellation retains its existing
                    // safety semantics. Once a turn is accepted, the focused
                    // prompt follows the original's two-Esc interrupt guard.
                    if self.active_turn.is_some()
                        || self.compactions.iter().any(crate::compaction::active)
                    {
                        let now = Instant::now();
                        if self.interrupt_armed_until.is_none_or(|until| now >= until) {
                            self.interrupt_armed_until = now.checked_add(Duration::from_secs(5));
                            return KeyOutcome::default();
                        }
                        self.interrupt_armed_until = None;
                    }
                    let session = self
                        .pending
                        .as_ref()
                        .map(|p| &p.session)
                        .or(self.session.as_ref())
                        .cloned();
                    if let Some(pending) = &mut self.pending {
                        pending.cancelling = true;
                    }
                    let Some(session) = session else {
                        return KeyOutcome {
                            note: Some("no session yet".into()),
                            ..KeyOutcome::default()
                        };
                    };
                    let compaction = self.compactions.iter().any(crate::compaction::active);
                    if compaction {
                        if let Err(error) = self.app.cancel_compaction(session.clone()).await {
                            return KeyOutcome {
                                note: Some(format!("cancel: {error}")),
                                ..KeyOutcome::default()
                            };
                        }
                        if self.active_turn.is_none() && self.pending.is_none() {
                            return KeyOutcome::default();
                        }
                    }
                    match self.app.cancel(session).await {
                        Ok(()) => KeyOutcome::default(),
                        Err(error) => KeyOutcome {
                            note: Some(format!("cancel: {error}")),
                            ..KeyOutcome::default()
                        },
                    }
                } else {
                    self.status = TuiStatus::Quit;
                    KeyOutcome::default()
                }
            }
            KeyAction::Enter => self.handle_enter().await,
            KeyAction::Tab => KeyOutcome::default(),
        }
    }

    fn recall_history(&mut self, previous: bool) -> bool {
        let entries: Vec<String> = self
            .window
            .rows()
            .iter()
            .filter(|row| row.role == "user")
            .map(|row| row.text.clone())
            .collect();
        let changed = self.editor.recall(&mut self.input, previous, entries);
        if changed {
            self.editor
                .move_to(if previous { 0 } else { self.input.len() }, false);
            self.input_revision += 1;
        }
        changed
    }

    /// Wheel/scrollbox navigation never changes the focused editor, even
    /// when keyboard Up/Down would move its caret or recall prompt history.
    pub fn scroll_transcript(&mut self, up: bool) -> KeyOutcome {
        self.wheel_motion = None;
        self.poll_submission();
        if self.panel != TuiPanel::None {
            return KeyOutcome::default();
        }
        self.clear_transcript_selection();
        if up {
            let max_scroll = self.max_scroll();
            self.scroll = self.display_scroll();
            if self.scroll < max_scroll {
                self.scroll += 1;
            }
            KeyOutcome {
                intent: (self.window.has_older() && self.scroll >= self.max_scroll())
                    .then_some(PanelIntent::LoadOlder),
                ..KeyOutcome::default()
            }
        } else if self.scroll > 0 {
            // Resize can clamp the displayed position below the retained
            // request. The first Down must move from that visible row.
            self.scroll = self.display_scroll().saturating_sub(1);
            KeyOutcome::default()
        } else {
            KeyOutcome {
                intent: self.window.has_newer().then_some(PanelIntent::LoadNewer),
                ..KeyOutcome::default()
            }
        }
    }

    /// A compatible directional wheel burst, in original arrival order. Only
    /// adjacent events with the same owner/direction/modifiers may be grouped.
    /// OC2 CustomSpeedScroll(3) times the terminal's unit delta supplies the
    /// target; keyboard navigation deliberately retains its separate step.
    pub fn wheel_transcript_at(&mut self, up: bool, ticks: usize, now: Instant) -> KeyOutcome {
        if self.panel != TuiPanel::None || ticks == 0 {
            return KeyOutcome::default();
        }
        self.tick_scroll_animation(now);
        let visible = self.display_scroll();
        let continuing = self.wheel_motion.filter(|motion| motion.up == up);
        let pending = continuing.map_or(0, |motion| motion.distance - motion.applied);
        // A reversal discards the old presentation debt and starts at the
        // visible position. It must not wait for an obsolete target to settle.
        let distance = ticks.saturating_mul(3).saturating_add(pending);
        let distance = distance.min(if up {
            self.max_scroll().saturating_sub(visible)
        } else {
            visible
        });
        self.scroll = visible;
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.reasoning_down = None;
        self.wheel_motion = (distance > 0).then_some(WheelMotion {
            started: continuing.map_or(now, |motion| motion.started),
            distance: distance.saturating_add(continuing.map_or(0, |motion| motion.applied)),
            applied: continuing.map_or(0, |motion| motion.applied),
            up,
        });
        KeyOutcome {
            intent: if up && visible.saturating_add(distance) >= self.max_scroll() {
                self.window.has_older().then_some(PanelIntent::LoadOlder)
            } else if !up && distance == visible {
                self.window.has_newer().then_some(PanelIntent::LoadNewer)
            } else {
                None
            },
            ..KeyOutcome::default()
        }
    }

    /// Next changed-row deadline, absent at rest. The binary folds this into
    /// its existing active render deadline; there is no independent timer.
    pub fn next_scroll_animation_deadline(&self) -> Option<Instant> {
        let motion = self.wheel_motion?;
        if self.panel != TuiPanel::None {
            return None;
        }
        let nanos = (WHEEL_PRESENTATION.as_nanos() * (motion.applied + 1) as u128)
            .div_ceil(motion.distance as u128);
        Some(motion.started + Duration::from_nanos(nanos as u64))
    }

    /// Elapsed progress is bounded by one presentation budget, independent of
    /// scheduler frequency. Returns dirty only when a painted row changes.
    pub fn tick_scroll_animation(&mut self, now: Instant) -> bool {
        let Some(mut motion) = self.wheel_motion else {
            return false;
        };
        if self.panel != TuiPanel::None {
            self.wheel_motion = None;
            return false;
        }
        let elapsed = now
            .saturating_duration_since(motion.started)
            .as_nanos()
            .min(WHEEL_PRESENTATION.as_nanos());
        let applied = (motion.distance as u128 * elapsed / WHEEL_PRESENTATION.as_nanos()) as usize;
        let step = applied.saturating_sub(motion.applied);
        if step == 0 {
            return false;
        }
        let visible = self.display_scroll();
        self.scroll = if motion.up {
            visible.saturating_add(step).min(self.max_scroll())
        } else {
            visible.saturating_sub(step)
        };
        motion.applied = applied;
        let edge = if motion.up {
            self.scroll == self.max_scroll()
        } else {
            self.scroll == 0
        };
        self.wheel_motion = (!edge && applied < motion.distance).then_some(motion);
        if visible == self.scroll {
            return false;
        }
        // Movement invalidates every press/release target from the old paint,
        // including selection-guarded user/reverted and expandable tool rows.
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.reasoning_down = None;
        true
    }

    async fn handle_enter(&mut self) -> KeyOutcome {
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        if self.pending.is_some() {
            if let Some(action) = dispatch(self.input.trim())
                && let Some(reason) = self.command_unavailable(&action)
            {
                return KeyOutcome {
                    note: Some(reason.into()),
                    ..KeyOutcome::default()
                };
            }
            return KeyOutcome {
                note: Some("submission pending; Esc to cancel".into()),
                ..KeyOutcome::default()
            };
        }
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return KeyOutcome::default();
        }
        if let Some(action) = dispatch(&text) {
            // Workspace commands reach the application, which owns their
            // templates; the built-in table only routes known commands.
            if !matches!(action, CommandAction::Help(None)) || !self.is_workspace_command(&text) {
                if matches!(action, CommandAction::RenameSession { title: None }) {
                    if let Some(reason) = self.command_unavailable(&action) {
                        return KeyOutcome {
                            note: Some(reason.into()),
                            ..KeyOutcome::default()
                        };
                    }
                    if self.regenerate_pending.is_some() {
                        return KeyOutcome {
                            note: Some("title generation pending".into()),
                            ..KeyOutcome::default()
                        };
                    }
                    self.regenerate_pending = Some(self.input_revision);
                    return KeyOutcome {
                        intent: Some(PanelIntent::RegenerateTitle),
                        ..KeyOutcome::default()
                    };
                }
                let outcome = self.run_command(action);
                if outcome.consumed_input
                    || matches!(
                        self.panel,
                        TuiPanel::Model
                            | TuiPanel::Variant
                            | TuiPanel::Agents
                            | TuiPanel::Sessions
                            | TuiPanel::Skills
                            | TuiPanel::Commands
                            | TuiPanel::Cards
                            | TuiPanel::Help(_)
                    )
                {
                    self.input.clear();
                    self.editor.clear();
                    self.input_revision += 1;
                }
                return outcome;
            }
        }
        if self.active_turn.is_some() {
            return KeyOutcome {
                note: Some("turn busy".into()),
                ..KeyOutcome::default()
            };
        }
        // The application owns Home selection and validates it on acceptance;
        // `None` resolves the current Home choice without session preferences.
        let fresh = self.session.is_none();
        let session = self.session.clone().unwrap_or_else(fresh_session_id);
        let result = if fresh {
            self.app.request_submit_fresh(session.clone(), text, None)
        } else {
            self.app.request_submit(session.clone(), text)
        };
        match result {
            Ok(receipt) => {
                self.begin_submission(receipt, session, fresh, false);
                KeyOutcome::default()
            }
            Err(error) => KeyOutcome {
                note: Some(format!("submit: {error}")),
                ..KeyOutcome::default()
            },
        }
    }

    /// Reconcile the unique acceptance receipt before applying queued turn
    /// events. Failure leaves the editable draft intact. No worker is spawned.
    pub fn poll_submission(&mut self) {
        let Some(result) = self.pending.as_mut().and_then(|p| p.receipt.try_result()) else {
            return;
        };
        let pending = self.pending.take().expect("polled receipt");
        self.reconcile_submission(pending, result, false);
    }

    /// Quit must not discard an in-flight Home root after the owner commits
    /// it. Cancel through the owner first (so tool work is stopped safely),
    /// then resolve the *same* receipt before the application shuts down.
    /// Existing-session turns already have a durable tab and need no wait.
    pub async fn reconcile_fresh_quit(&mut self) -> Result<(), CoreError> {
        let Some(pending) = self.pending.as_mut().filter(|p| p.fresh) else {
            return Ok(());
        };
        pending.cancelling = true;
        let session = pending.session.clone();
        // Cancel follows SubmitFresh in the owner's inbox. A rejected or
        // already-finished turn has nothing left to cancel.
        match self.app.cancel(session).await {
            Ok(()) | Err(CoreError::TurnNotActive) => {}
            Err(error) => return Err(error),
        }
        let result = self
            .pending
            .as_mut()
            .expect("fresh receipt still pending")
            .receipt
            .wait()
            .await;
        let pending = self.pending.take().expect("fresh receipt still pending");
        if result == Err(CoreError::Shutdown) {
            return Err(CoreError::Shutdown);
        }
        self.reconcile_submission(pending, result, true);
        Ok(())
    }

    fn reconcile_submission(
        &mut self,
        pending: PendingSubmission,
        result: Result<WorkerTurnId, CoreError>,
        exiting: bool,
    ) {
        if (self.status == TuiStatus::Quit && !exiting)
            || pending.request_id != self.request_id
            || pending.generation != self.generation
            || (pending.fresh != self.session.is_none())
            || (!pending.fresh && self.session.as_ref() != Some(&pending.session))
        {
            return;
        }
        match result {
            Ok(turn) => {
                self.invalidate_transcript();
                if pending.fresh {
                    self.session = Some(pending.session);
                }
                self.live_preview_truncated = false;
                self.live_part_states.clear();
                self.live_terminal_status = None;
                self.live_model_label = None;
                self.live_agent_color_index = None;
                if !pending.compress {
                    self.home = false;
                    self.editor
                        .accepted_mentions(pending.draft.trim(), pending.mentions);
                    self.window.push_synthetic(
                        "user",
                        pending.draft.trim(),
                        pending.agent,
                        pending.agent_color_index,
                    );
                    self.prune_reasoning();
                }
                self.compress_turn = pending.compress.then(|| turn.clone());
                if !pending.compress {
                    self.conversation_available = Some((true, false));
                }
                self.live_text.clear();
                self.live_reasoning.clear();
                self.live_parts.clear();
                self.pending_tool_seen.clear();
                self.pending_tool_round = 0;
                self.live_part_offset = 0;
                self.reasoning_down = None;
                self.reasoning_started = None;
                self.reasoning_finished = None;
                self.turn_usage = None;
                self.active_turn = Some(turn);
                self.reasoning_epoch = self.reasoning_epoch.wrapping_add(1);
                if !exiting {
                    self.reset_scanner();
                    self.status = TuiStatus::Streaming;
                }
                self.scroll = 0;
                self.wheel_motion = None;
                if self.input_revision == pending.revision && !pending.cancelling {
                    self.input.clear();
                    self.editor.clear_submitted_draft();
                }
                self.dcp.clear_notice();
                self.note = None;
            }
            Err(error) => {
                if !exiting {
                    self.status = TuiStatus::Idle;
                }
                self.push_note(&format!("submit: {error}"));
            }
        }
    }

    fn run_command(&mut self, action: CommandAction) -> KeyOutcome {
        if let Some(reason) = self.command_unavailable(&action) {
            return KeyOutcome {
                note: Some(reason.into()),
                ..KeyOutcome::default()
            };
        }
        if matches!(
            action,
            CommandAction::UndoConversation | CommandAction::RedoConversation
        ) {
            return KeyOutcome {
                intent: Some(PanelIntent::ChangeConversation {
                    action: if action == CommandAction::UndoConversation {
                        oc_core::queries::ConversationAction::Undo
                    } else {
                        oc_core::queries::ConversationAction::Redo
                    },
                }),
                ..KeyOutcome::default()
            };
        }
        if action == CommandAction::CloseTab {
            let (_, index, _) = self.tab_presentation();
            // The binary applies the close and replaces the view on success.
            // A refused owner action must leave the dialog and draft intact.
            return KeyOutcome {
                intent: Some(PanelIntent::CloseTab { index }),
                ..KeyOutcome::default()
            };
        }
        if action == CommandAction::CompactSession {
            // Retain palette search/cursor and composer until owner admission.
            return KeyOutcome {
                intent: Some(PanelIntent::CompactSession),
                ..KeyOutcome::default()
            };
        }
        if self.regenerate_pending.is_some()
            && matches!(action, CommandAction::RenameSession { title: None })
        {
            return KeyOutcome {
                note: Some("title generation pending".into()),
                ..KeyOutcome::default()
            };
        }
        if let CommandAction::RenameSession { title: Some(title) } = &action {
            if self.rename_direct_pending.is_some() {
                return KeyOutcome {
                    note: Some("session rename pending".into()),
                    ..KeyOutcome::default()
                };
            }
            let Some(title) = oc_core::core_app::normalized_session_title(title) else {
                return KeyOutcome {
                    note: Some(format!(
                        "session title must be 1–{MAX_SESSION_TITLE_BYTES} bytes of visible text"
                    )),
                    ..KeyOutcome::default()
                };
            };
            let title = title.to_string();
            self.rename_direct_pending = Some((title.clone(), self.input_revision));
            return KeyOutcome {
                intent: Some(PanelIntent::RenameSessionDirect { title }),
                ..KeyOutcome::default()
            };
        }
        self.select.reset();
        self.mouse_down = None;
        self.tab_down = None;
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        self.last_mouse = None;
        self.leader = None;
        let mut outcome = KeyOutcome::default();
        match action {
            CommandAction::OpenSettings => self.panel = TuiPanel::Settings,
            CommandAction::OpenPermissions => self.panel = TuiPanel::Settings,
            CommandAction::UndoConversation | CommandAction::RedoConversation => {
                unreachable!("returned before modal reset")
            }
            CommandAction::OpenCommands => {
                self.panel = TuiPanel::Commands;
            }
            CommandAction::ToggleSidebar => {
                self.chrome.sidebar_hidden = !self.chrome.sidebar_hidden;
                self.panel = TuiPanel::None;
                outcome.consumed_input = true;
            }
            CommandAction::ToggleThinking => {
                self.thinking_expanded = !self.thinking_expanded;
                self.panel = TuiPanel::None;
                outcome.consumed_input = true;
            }
            CommandAction::Quit => {
                self.status = TuiStatus::Quit;
                outcome.consumed_input = true;
            }
            CommandAction::OpenModelPicker => {
                self.panel = TuiPanel::Model;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenVariants => self.open_variants(),
            CommandAction::NewSession => {
                outcome.intent = Some(PanelIntent::NewSession);
            }
            CommandAction::ReloadConfiguration => {
                outcome.intent = Some(PanelIntent::ReloadConfiguration);
            }
            CommandAction::RenameSession { title: None } => {
                self.panel = TuiPanel::Rename;
                // Generated titles contain at most 100 Unicode scalar values (<=400
                // UTF-8 bytes). Keep the whole title, even when the owner would
                // reject it as a replacement, so Enter cannot submit a prefix.
                // An unexpectedly larger title stays only in session_title, not
                // in the editor's undo history or an unbounded modal copy.
                const MAX_RENAME_PREFILL_BYTES: usize = 100 * 4;
                self.rename_input = self
                    .session_title
                    .as_ref()
                    .filter(|title| title.len() <= MAX_RENAME_PREFILL_BYTES)
                    .cloned()
                    .unwrap_or_default();
                if self
                    .session_title
                    .as_ref()
                    .is_some_and(|title| title.len() > MAX_RENAME_PREFILL_BYTES)
                {
                    outcome.note =
                        Some("existing title too long to prefill; type a replacement".into());
                } else if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
                    outcome.note = Some(self.rename_limit_note());
                }
                self.rename_editor.clear();
                self.rename_editor.cursor = self.rename_input.len();
                self.rename_pending = None;
            }
            CommandAction::RenameSession { title: Some(_) } => {
                unreachable!("direct rename is returned before modal reset")
            }
            CommandAction::CloseTab => unreachable!("close is returned before modal reset"),
            CommandAction::OpenAgents => {
                self.panel = TuiPanel::Agents;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenSessions => {
                self.panel = TuiPanel::Sessions;
                // Sessions can be created by the application since the last opening.
                outcome.intent = Some(PanelIntent::LoadSessions);
                outcome.consumed_input = self.sessions_loaded;
            }
            CommandAction::OpenSkills => {
                self.panel = TuiPanel::Skills;
                open_snapshot(&mut outcome, self.skills_loaded, PanelIntent::LoadSkills);
            }
            CommandAction::OpenCards => {
                self.panel = TuiPanel::Cards;
                self.card_output = None;
                self.card_scroll = 0;
                self.card_seen.set(0);
                open_snapshot(&mut outcome, self.cards_loaded, PanelIntent::LoadCards);
            }
            CommandAction::SwitchLocation { path } => {
                if path.is_empty() {
                    outcome.note = Some("usage: /location <project-path>".to_string());
                    outcome.consumed_input = true;
                } else {
                    outcome.intent = Some(PanelIntent::SwitchLocation { path });
                }
            }
            CommandAction::Help(topic) => {
                self.panel = TuiPanel::Help(topic);
                outcome.consumed_input = true;
            }
            CommandAction::DcpCompress { focus } => {
                self.panel = TuiPanel::Dcp;
                outcome.intent = Some(PanelIntent::Compress { focus });
            }
            CommandAction::CompactSession => {
                unreachable!("compaction is returned before modal reset");
            }
        }
        self.sync_modal_cursor();
        outcome
    }

    /// Panel navigation: Up/Down move the panel cursor, Enter chooses,
    /// Esc closes; text and paste belong to the focused modal search.
    pub fn handle_panel_key(&mut self, action: KeyAction) -> KeyOutcome {
        if self.approvals.active().is_some() {
            return self.approvals.key(action);
        }
        if self.panel == TuiPanel::Settings
            && action == KeyAction::Cancel
            && !self.select.query.is_empty()
        {
            self.select.reset();
            self.changed_modal_query();
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::Settings && matches!(action, KeyAction::Left | KeyAction::Right)
        {
            return KeyOutcome {
                intent: Some(PanelIntent::SetPermissionMode {
                    auto_once: !self.chrome.permissions_auto,
                }),
                ..Default::default()
            };
        }
        if self.panel == TuiPanel::Sessions {
            if matches!(action, KeyAction::Rename | KeyAction::DeleteOrQuit) {
                let Some(id) = self.sessions.get(self.sessions_cursor).cloned() else {
                    return KeyOutcome::default();
                };
                if action == KeyAction::DeleteOrQuit {
                    if self.session_delete_confirm.as_ref() == Some(&id) {
                        self.session_delete_confirm = None;
                        return KeyOutcome {
                            intent: Some(PanelIntent::DeleteSelectedSession { id }),
                            ..KeyOutcome::default()
                        };
                    }
                    self.session_delete_confirm = Some(id);
                } else {
                    self.rename_input = self
                        .session_entries
                        .iter()
                        .find(|entry| entry.id.0 == id)
                        .map(|entry| entry.title.clone())
                        .unwrap_or_default();
                    self.rename_editor.clear();
                    self.rename_editor.cursor = self.rename_input.len();
                    self.rename_pending = None;
                    self.rename_selected = Some(id);
                    self.session_delete_confirm = None;
                    self.panel = TuiPanel::Rename;
                }
                return KeyOutcome::default();
            }
            // Matches donor onMove; search/scope changes also retire intent.
            self.session_delete_confirm = None;
        }
        if self.panel == TuiPanel::Sessions && action == KeyAction::CtrlA {
            self.sessions_all_projects = !self.sessions_all_projects;
            self.session_scope_pending = Some(self.sessions_all_projects);
            self.select.changed_query();
            return KeyOutcome {
                intent: Some(PanelIntent::LoadSessions),
                ..KeyOutcome::default()
            };
        }
        let action = if action == KeyAction::CtrlA {
            KeyAction::Home
        } else {
            action
        };
        if self.panel == TuiPanel::Rename {
            return self.handle_rename_key(action);
        }
        if self.panel == TuiPanel::Cards && self.card_output.is_some() {
            let (start, height, count) = crate::views::card_window(self);
            self.card_scroll = start;
            match action {
                KeyAction::Up => self.card_scroll = start.saturating_sub(1),
                KeyAction::Down => self.card_scroll = (start + 1).min(count.saturating_sub(height)),
                KeyAction::PageUp => self.card_scroll = start.saturating_sub(height),
                KeyAction::PageDown => {
                    self.card_scroll = (start + height).min(count.saturating_sub(height))
                }
                KeyAction::Home => self.card_scroll = 0,
                KeyAction::End => self.card_scroll = count.saturating_sub(height),
                KeyAction::Enter
                    if height > 0 && start + height >= count && self.card_seen.get() >= count =>
                {
                    return self.panel_enter();
                }
                KeyAction::Cancel => {
                    self.card_output = None;
                    self.card_scroll = 0;
                    self.select.reset();
                }
                KeyAction::Quit | KeyAction::Interrupt => self.status = TuiStatus::Quit,
                _ => {}
            }
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Char(c) => {
                if self.select.query.len() + c.len_utf8() <= 512 {
                    self.select.query.push(c);
                }
                self.changed_modal_query();
                return KeyOutcome {
                    intent: (self.panel == TuiPanel::Sessions).then_some(PanelIntent::LoadSessions),
                    ..KeyOutcome::default()
                };
            }
            KeyAction::Backspace => {
                self.select.query.pop();
                self.changed_modal_query();
                return KeyOutcome {
                    intent: (self.panel == TuiPanel::Sessions).then_some(PanelIntent::LoadSessions),
                    ..KeyOutcome::default()
                };
            }
            KeyAction::Interrupt
                if matches!(
                    self.panel,
                    TuiPanel::Dcp | TuiPanel::Cards | TuiPanel::Help(_)
                ) =>
            {
                // Existing informational-panel shutdown binding (including
                // the post-compression DCP panel); Select dialogs retain their
                // own clear-filter/dismiss behavior.
                self.status = TuiStatus::Quit;
                return KeyOutcome::default();
            }
            KeyAction::Interrupt => {
                if self.select.query.is_empty() {
                    self.close_panel();
                } else {
                    self.select.reset();
                    self.changed_modal_query();
                    if self.panel == TuiPanel::Sessions {
                        return KeyOutcome {
                            intent: Some(PanelIntent::LoadSessions),
                            ..KeyOutcome::default()
                        };
                    }
                }
                return KeyOutcome::default();
            }
            KeyAction::Commands
            | KeyAction::Up
            | KeyAction::Down
            | KeyAction::PageUp
            | KeyAction::PageDown
            | KeyAction::Home
            | KeyAction::End
                if matches!(
                    self.panel,
                    TuiPanel::Commands
                        | TuiPanel::Model
                        | TuiPanel::Variant
                        | TuiPanel::Agents
                        | TuiPanel::Sessions
                        | TuiPanel::Skills
                        | TuiPanel::MessageActions { .. }
                ) =>
            {
                let count = self.modal_options().len();
                match action {
                    KeyAction::Home => self.select.cursor = 0,
                    KeyAction::End => self.select.cursor = count.saturating_sub(1),
                    _ => self.select.move_by(
                        match action {
                            KeyAction::Commands | KeyAction::Up => -1,
                            KeyAction::PageUp => -10,
                            KeyAction::PageDown => 10,
                            _ => 1,
                        },
                        count,
                    ),
                }
                self.sync_modal_cursor();
                self.select.follow_selection();
                return KeyOutcome::default();
            }
            KeyAction::Enter if self.modal_options().is_empty() => return KeyOutcome::default(),
            _ => {}
        }
        match action {
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.panel == TuiPanel::Cards && self.card_output.is_some() {
                    self.card_output = None;
                    self.select.reset();
                } else {
                    self.close_panel();
                }
                KeyOutcome::default()
            }
            KeyAction::Left | KeyAction::Right => KeyOutcome::default(),
            KeyAction::Up => {
                self.move_panel_cursor(-1);
                let intent = (self.panel == TuiPanel::Cards
                    && self.cards_has_older
                    && self.cards_cursor == 0)
                    .then_some(PanelIntent::LoadCards);
                KeyOutcome {
                    intent,
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Down => {
                self.move_panel_cursor(1);
                KeyOutcome::default()
            }
            KeyAction::Enter => self.panel_enter(),
            _ => KeyOutcome::default(),
        }
    }

    fn handle_rename_key(&mut self, action: KeyAction) -> KeyOutcome {
        if action == KeyAction::Cancel {
            if self.rename_pending.is_none() {
                self.close_panel();
            }
            return KeyOutcome::default();
        }
        if self.rename_pending.is_some() {
            return KeyOutcome::default();
        }
        if action == KeyAction::Interrupt {
            if self.rename_input.is_empty() {
                self.close_panel();
            } else {
                self.rename_input.clear();
                self.rename_editor.clear();
            }
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Enter => {
                if self.rename_selected.is_none()
                    && let Some(reason) =
                        self.command_unavailable(&CommandAction::RenameSession { title: None })
                {
                    return KeyOutcome {
                        note: Some(reason.into()),
                        ..KeyOutcome::default()
                    };
                }
                if self.rename_input.trim().is_empty() {
                    return KeyOutcome::default();
                }
                if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
                    if self.session_title.as_deref() == Some(self.rename_input.as_str()) {
                        self.close_panel();
                        return KeyOutcome::default();
                    }
                    return KeyOutcome {
                        note: Some(self.rename_limit_note()),
                        ..KeyOutcome::default()
                    };
                }
                let title = self.rename_input.trim().to_string();
                self.rename_pending = Some(title.clone());
                KeyOutcome {
                    intent: Some(match &self.rename_selected {
                        Some(id) => PanelIntent::RenameSelectedSession {
                            id: id.clone(),
                            title,
                        },
                        None => PanelIntent::RenameSession { title },
                    }),
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Char(c) if !c.is_control() => {
                let inserted = self.rename_editor.replace(
                    &mut self.rename_input,
                    &c.to_string(),
                    MAX_SESSION_TITLE_BYTES,
                );
                KeyOutcome {
                    note: (inserted == 0).then(|| self.rename_limit_note()),
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Backspace | KeyAction::WordBackspace => {
                self.rename_editor.delete(
                    &mut self.rename_input,
                    true,
                    action == KeyAction::WordBackspace,
                );
                KeyOutcome::default()
            }
            KeyAction::Delete | KeyAction::DeleteOrQuit | KeyAction::WordDelete => {
                self.rename_editor.delete(
                    &mut self.rename_input,
                    false,
                    action == KeyAction::WordDelete,
                );
                KeyOutcome::default()
            }
            KeyAction::Left
            | KeyAction::Right
            | KeyAction::WordLeft
            | KeyAction::WordRight
            | KeyAction::SelectLeft
            | KeyAction::SelectRight
            | KeyAction::SelectWordLeft
            | KeyAction::SelectWordRight => {
                self.rename_editor.horizontal(
                    &self.rename_input,
                    matches!(
                        action,
                        KeyAction::Right
                            | KeyAction::WordRight
                            | KeyAction::SelectRight
                            | KeyAction::SelectWordRight
                    ),
                    matches!(
                        action,
                        KeyAction::WordLeft
                            | KeyAction::WordRight
                            | KeyAction::SelectWordLeft
                            | KeyAction::SelectWordRight
                    ),
                    matches!(
                        action,
                        KeyAction::SelectLeft
                            | KeyAction::SelectRight
                            | KeyAction::SelectWordLeft
                            | KeyAction::SelectWordRight
                    ),
                );
                KeyOutcome::default()
            }
            KeyAction::Home | KeyAction::End | KeyAction::SelectHome | KeyAction::SelectEnd => {
                self.rename_editor.move_to(
                    if matches!(action, KeyAction::End | KeyAction::SelectEnd) {
                        self.rename_input.len()
                    } else {
                        0
                    },
                    matches!(action, KeyAction::SelectHome | KeyAction::SelectEnd),
                );
                KeyOutcome::default()
            }
            KeyAction::Undo | KeyAction::Redo => {
                self.rename_editor
                    .undo(&mut self.rename_input, action == KeyAction::Redo);
                KeyOutcome::default()
            }
            _ => KeyOutcome::default(),
        }
    }

    fn move_panel_cursor(&mut self, delta: isize) {
        match self.panel {
            TuiPanel::Model => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.move_cursor(delta);
                }
            }
            TuiPanel::Agents => {
                self.agents_cursor = clamp_cursor(self.agents_cursor, delta, self.agents.len());
            }
            TuiPanel::Sessions => {
                self.sessions_cursor =
                    clamp_cursor(self.sessions_cursor, delta, self.sessions.len());
            }
            TuiPanel::Skills => {
                self.skills_cursor = clamp_cursor(self.skills_cursor, delta, self.skills.len());
            }
            TuiPanel::Cards => {
                self.cards_cursor = clamp_cursor(self.cards_cursor, delta, self.cards.len());
            }
            _ => {}
        }
    }

    fn panel_enter(&mut self) -> KeyOutcome {
        let mut outcome = KeyOutcome::default();
        match self.panel.clone() {
            TuiPanel::MessageActions { message, seq } => {
                if !self
                    .window
                    .rows()
                    .iter()
                    .any(|row| row.role == "user" && row.message_id.as_deref() == Some(&message))
                {
                    outcome.note =
                        Some("message is no longer in the current history window".into());
                    return outcome;
                }
                let options = self.modal_options();
                match options
                    .get(self.select.cursor)
                    .map(|option| option.value.as_str())
                {
                    Some("jump") => self.close_panel(),
                    Some("revert") => {
                        outcome.intent = Some(PanelIntent::ChangeConversation {
                            action: oc_core::queries::ConversationAction::Revert { message },
                        })
                    }
                    Some("copy") => {
                        outcome.intent = Some(PanelIntent::CopyMessage { message, seq })
                    }
                    Some("fork") => outcome.intent = Some(PanelIntent::ForkMessage { message }),
                    _ => {}
                }
            }
            TuiPanel::Commands => {
                let options = self.modal_options();
                if let Some(option) = options.get(self.select.cursor)
                    && let Some(command) = crate::commands::REGISTRY
                        .iter()
                        .find(|c| c.id == option.value)
                {
                    return self.run_command(command.action.clone());
                }
            }
            TuiPanel::Rename => return self.handle_rename_key(KeyAction::Enter),
            TuiPanel::Model if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenModelPicker)
                    .map(str::to_string)
            }
            TuiPanel::Variant if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenVariants)
                    .map(str::to_string)
            }
            TuiPanel::Agents if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenAgents)
                    .map(str::to_string)
            }
            TuiPanel::Sessions if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenSessions)
                    .map(str::to_string)
            }
            TuiPanel::Model => match self.picker_selection() {
                Some((id, _)) => {
                    outcome.intent = Some(PanelIntent::SelectModel { id });
                }
                None => outcome.note = Some("no model selected".to_string()),
            },
            TuiPanel::Variant => {
                if let Some(option) = self.modal_options().get(self.select.cursor)
                    && let Some(selection) = self.picker.as_ref().and_then(|p| p.selection())
                {
                    outcome.intent = Some(PanelIntent::ChooseModel {
                        id: selection.id.clone(),
                        variant: (option.value != "default").then(|| option.value.clone()),
                    });
                }
            }
            TuiPanel::Settings => {
                outcome.intent = Some(PanelIntent::SetPermissionMode {
                    auto_once: !self.chrome.permissions_auto,
                });
            }
            TuiPanel::Agents => match self.selected_agent() {
                Some(id) => outcome.intent = Some(PanelIntent::SelectAgent { id }),
                None => outcome.note = Some("no agent selected".to_string()),
            },
            TuiPanel::Sessions => match self.sessions.get(self.sessions_cursor) {
                Some(id) if SessionId::new(id.clone()).is_some() => {
                    outcome.intent = Some(PanelIntent::SwitchSession { id: id.clone() });
                }
                Some(_) => outcome.note = Some("bad session id".to_string()),
                None => outcome.note = Some("empty session list".to_string()),
            },
            TuiPanel::Skills => {
                self.panel = TuiPanel::None;
            }
            TuiPanel::Cards => {
                if self.session.is_none() {
                    outcome.note = Some("no session yet".into());
                    return outcome;
                }
                if let Some(detail) = &self.card_output {
                    if let Some(offset) = detail.page.next_offset {
                        outcome.intent = Some(PanelIntent::LoadCardOutput {
                            op: detail.op.clone(),
                            offset: offset as usize,
                        });
                    } else {
                        self.card_output = None;
                    }
                } else if let Some(op) = self.card_ops.get(self.cards_cursor) {
                    outcome.intent = Some(PanelIntent::LoadCardOutput {
                        op: op.clone(),
                        offset: 0,
                    });
                }
            }
            TuiPanel::Dcp => {
                if self.session.is_some() {
                    outcome.intent = Some(PanelIntent::Compress {
                        focus: String::new(),
                    });
                } else {
                    outcome.note = Some("no session yet".into());
                }
            }
            TuiPanel::Help(_) | TuiPanel::None => {
                self.panel = TuiPanel::None;
            }
        }
        outcome
    }

    // ---- worker events --------------------------------------------------

    pub fn apply_model_switch(
        &mut self,
        turn: &WorkerTurnId,
        notice: &oc_core::queries::ModelSwitchNotice,
    ) {
        if self.active_turn.as_ref() != Some(turn) {
            return;
        }
        self.window
            .insert_before_live_user(crate::history::model_switch_text(notice));
        self.invalidate_transcript();
    }

    /// Apply a worker text delta to the live line (turn-scoped: deltas for
    /// a stale turn are ignored, so a late event can never corrupt the view).
    pub fn apply_delta(&mut self, turn: &WorkerTurnId, delta: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        if self.live_text.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_text.len();
            self.live_text.push_str(crate::truncate_utf8(delta, room));
            self.invalidate_transcript();
        }
    }

    /// Apply a worker reasoning delta to the live reasoning block
    /// (turn-scoped, bounded, never persisted).
    pub fn apply_reasoning_delta(&mut self, turn: &WorkerTurnId, delta: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if self.live_reasoning.is_empty() && !self.live_text.is_empty() {
            self.freeze_text();
            self.enforce_parts();
        }
        if self.reasoning_started.is_none() {
            self.reasoning_started = Some(Instant::now());
        }
        if self.live_reasoning.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_reasoning.len();
            self.live_reasoning
                .push_str(crate::truncate_utf8(delta, room));
            self.invalidate_transcript();
        }
    }

    /// Close one active public reasoning item before the next output item.
    /// Duplicates and boundaries without public text do not create rows.
    pub fn apply_reasoning_item_ended(&mut self, turn: &WorkerTurnId) {
        if Some(turn) != self.active_turn.as_ref() || self.live_reasoning.is_empty() {
            return;
        }
        self.reasoning_finished = Some(Instant::now());
        self.freeze_reasoning();
        self.enforce_parts();
    }

    /// Adopt explicit checkpoint identities and pinned presentation only for
    /// the current generation/turn. Deltas remain transient until checkpointed.
    pub fn apply_presentation(
        &mut self,
        turn: &WorkerTurnId,
        projection: &oc_core::queries::HistoryTurn,
    ) {
        if self.active_turn.as_ref() != Some(turn) || projection.id != turn.0 {
            return;
        }
        self.live_part_states = projection.part_states.clone();
        self.invalidate_transcript();
        self.live_preview_truncated |= projection.truncated;
        self.live_agent_color_index = projection.agent_color_index;
        self.live_terminal_status = Some(projection.status.clone());
        self.live_model_label =
            (!projection.model_label.is_empty()).then(|| projection.model_label.clone());
    }

    /// Apply provider-reported usage for the active turn; without it the
    /// footer omits `tok/s` instead of inventing a rate.
    pub fn apply_usage(
        &mut self,
        turn: &WorkerTurnId,
        input_tokens: u64,
        output_tokens: u64,
        streamed_ms: u64,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.turn_usage = Some(TurnUsage {
            input_tokens,
            output_tokens,
            streamed_ms,
        });
    }

    /// Apply a worker turn-finished event: commit the live answer with its
    /// reasoning block and footer metadata, then release the turn. When tool
    /// cards or frozen segments exist, every part keeps its upstream position
    /// and the footer becomes its own row after them; the committed text is
    /// the concatenation of the frozen segments (the deltas already shown).
    pub fn apply_finished(&mut self, turn: &WorkerTurnId, text: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.invalidate_transcript();
        let meta = self.finish_meta(false, duration_ms);
        self.reset_scanner();
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if self.live_parts.is_empty() {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: text.to_string(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
            });
            self.prune_reasoning();
            return;
        }
        let parts = self.commit_live_parts(reasoning);
        self.push_committed_parts(parts);
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.prune_reasoning();
    }

    /// Apply a worker turn-interrupted event: keep the partial text (never
    /// committed to storage), mark the footer `interrupted`, and release the
    /// turn.
    pub fn apply_interrupted(&mut self, turn: &WorkerTurnId, partial: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.invalidate_transcript();
        let meta = self.finish_meta(true, duration_ms);
        self.reset_scanner();
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Cancelled;
        if self.live_parts.is_empty() {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: partial.to_string(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
            });
            self.prune_reasoning();
            return;
        }
        // The partial text is already the frozen trailing segment.
        let parts = self.commit_live_parts(reasoning);
        self.push_committed_parts(parts);
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.prune_reasoning();
    }

    /// Release a failed turn and show its error, never a successful answer.
    pub fn apply_failed(&mut self, turn: &WorkerTurnId, error: &CoreError) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.invalidate_transcript();
        let mut meta = self.finish_meta(false, 0);
        self.reset_scanner();
        if meta.status.is_none() {
            meta.status = Some("failed".into());
        }
        let reasoning = self.take_reasoning();
        let parts = self.commit_live_parts(reasoning);
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if !parts.is_empty() {
            // Cards already shown stay visible; the error follows them.
            self.push_committed_parts(parts);
        }
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.window
            .push_synthetic("", &format!("(error: {error})"), None, None);
        self.prune_reasoning();
    }

    /// Freeze the open live segments and return the whole part list in
    /// arrival order, including reasoning after an earlier tool round.
    fn commit_live_parts(&mut self, reasoning: Option<ReasoningBlock>) -> Vec<LivePart> {
        self.discard_pending_tools();
        self.pending_tool_seen.clear();
        self.pending_tool_round = 0;
        if let Some(reasoning) = reasoning {
            self.live_parts.push(LivePart::Reasoning {
                text: reasoning.text,
                duration_ms: reasoning.duration_ms,
            });
        }
        self.freeze_reasoning();
        self.freeze_text();
        self.live_reasoning.clear();
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        let parts = std::mem::take(&mut self.live_parts);
        self.sync_compaction_clock();
        parts
    }

    /// Push committed part rows into the window (bounded like any row).
    fn push_committed_parts(&mut self, parts: Vec<LivePart>) {
        for (ordinal, part) in parts.into_iter().enumerate() {
            if matches!(part, LivePart::Vacant) {
                continue;
            }
            self.window.push_row(part.to_row(
                self.active_agent.clone(),
                Some(crate::messages::ReasoningIdentity::Live(
                    self.reasoning_epoch,
                    self.live_part_offset + ordinal,
                )),
            ));
        }
    }

    /// Freeze the open text segment (a tool call follows it).
    fn freeze_text(&mut self) {
        if !self.live_text.is_empty() {
            self.live_parts
                .push(LivePart::Text(std::mem::take(&mut self.live_text)));
        }
    }

    /// Freeze the open reasoning segment with its measured window.
    fn freeze_reasoning(&mut self) {
        if self.live_reasoning.is_empty() {
            return;
        }
        let duration_ms = match (
            self.reasoning_started.take(),
            self.reasoning_finished.take(),
        ) {
            (Some(started), Some(finished)) => {
                Some(finished.saturating_duration_since(started).as_millis() as u64)
            }
            _ => None,
        };
        self.live_parts.push(LivePart::Reasoning {
            text: std::mem::take(&mut self.live_reasoning),
            duration_ms,
        });
        self.reasoning_started = None;
        self.reasoning_finished = None;
    }

    /// Apply a recorded tool-call intent: freeze the open segments, then
    /// append the running card in upstream part order.
    pub fn apply_tool_started(&mut self, turn: &WorkerTurnId, op: &str, name: &str, input: &str) {
        self.apply_tool_started_with_presentation(turn, op, name, input, None);
    }

    /// Owner-derived pending DCP topic; input remains an opaque recorded payload.
    pub fn apply_tool_started_with_presentation(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        input: &str,
        dcp_topic: Option<String>,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if name == "compress" && self.live_parts.iter().any(|part| matches!(part,
            LivePart::Tool { card, .. } if card.op == op && matches!(&card.render, crate::tools::ToolRender::Dcp(view) if view.snapshot.is_some()))) {
            return;
        }
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        self.freeze_reasoning();
        self.freeze_text();
        let mut card = card_from_row(&ToolOpView {
            rowid: 0,
            op: op.to_string(),
            name: name.to_string(),
            state: "started".to_string(),
            input: Some(input.to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic,
        });
        if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
            view.color_index = self.live_agent_color_index;
        }
        if let Some(part) = self
            .live_parts
            .iter_mut()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            *part = LivePart::Tool {
                card: Box::new(card),
                input: input.to_string(),
            };
            self.enforce_parts();
            self.sync_compaction_clock();
            return;
        }
        self.live_parts.push(LivePart::Tool {
            card: Box::new(card),
            input: input.to_string(),
        });
        self.enforce_parts();
        self.sync_compaction_clock();
    }

    /// Apply disposable provider snapshots. Raw fragments never enter the
    /// argument parser, diff renderer, durable projection or tool executor.
    pub fn project_pending_approvals(&mut self, requests: &[oc_core::approval::ApprovalRequest]) {
        if requests.is_empty()
            && !self.live_parts.iter().any(|part| {
                matches!(part,
            LivePart::Tool { card, .. } if card.state == "permission_pending")
            })
        {
            return;
        }
        use oc_core::approval::ApprovalPreview;
        let eligible: Vec<_> = requests
            .iter()
            .filter(|r| {
                self.session
                    .as_ref()
                    .is_some_and(|s| s.0 == r.binding.session)
                    && self
                        .active_turn
                        .as_ref()
                        .is_some_and(|t| t.0 == r.binding.turn)
            })
            .collect();
        for request in &eligible {
            let present = self.live_parts.iter().any(|part| matches!(part, LivePart::Tool { card, .. } if card.op == request.binding.operation || card.op.splitn(3, ':').nth(2).and_then(|json| serde_json::from_str::<[String; 2]>(json).ok()).is_some_and(|ids| ids[1] == request.binding.call)));
            if !present && self.live_parts.iter().filter(|p| matches!(p, LivePart::Tool { card, .. } if matches!(card.state.as_str(), "argument_stream" | "permission_pending"))).count() < oc_core::tool_stream::PENDING_TOOL_MAX {
                let card = card_from_row(&ToolOpView { rowid: 0, op: request.binding.operation.clone(), name: request.action.clone(), state: "argument_stream".into(), input: None, output: None, output_bytes: 0, output_truncated: false, patch_effects: None, dcp: None, dcp_topic: None });
                self.live_parts.push(LivePart::Tool { card: Box::new(card), input: String::new() });
            }
        }
        for part in &mut self.live_parts {
            let LivePart::Tool { card, .. } = part else {
                continue;
            };
            if !matches!(
                card.state.as_str(),
                "argument_stream" | "permission_pending"
            ) {
                continue;
            }
            let request = eligible.iter().find(|r| {
                card.op == r.binding.operation
                    || card
                        .op
                        .splitn(3, ':')
                        .nth(2)
                        .and_then(|json| serde_json::from_str::<[String; 2]>(json).ok())
                        .is_some_and(|ids| ids[1] == r.binding.call)
            });
            let Some(request) = request else {
                if card.state == "permission_pending" {
                    card.state = "argument_stream".into();
                }
                continue;
            };
            // This is a disposable view of owner-prepared bytes, never an execution
            // intent or a parser of unfinished provider argument fragments.
            // Bind the presentation to its prepared operation so a rejection can
            // reconcile without a Started/Linked event (neither may precede consent).
            card.op.clone_from(&request.binding.operation);
            card.state = "permission_pending".into();
            card.input_preview.clear();
            if let ApprovalPreview::Patch {
                files,
                total_files,
                truncated,
            } = &request.preview
            {
                card.files = files
                    .iter()
                    .take(crate::history::CARD_FILES)
                    .map(|file| file.destination.as_ref().unwrap_or(&file.path).clone())
                    .collect();
                card.files_truncated = *truncated || *total_files > card.files.len();
            }
            card.render = match &request.preview {
                ApprovalPreview::Shell { command, cwd } => {
                    crate::tools::ToolRender::Shell(crate::tools::ShellRender {
                        command: command.clone(),
                        cwd: Some(cwd.clone()),
                        ..Default::default()
                    })
                }
                ApprovalPreview::Resource { values } if card.name == "read" => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Read {
                        path: values.first().cloned().unwrap_or_default(),
                    })
                }
                ApprovalPreview::Resource { values } if card.name == "webfetch" => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::WebFetch {
                        url: values.first().cloned().unwrap_or_default(),
                    })
                }
                ApprovalPreview::Search { pattern, .. } if card.name == "glob" => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Glob {
                        pattern: pattern.clone(),
                        matches: None,
                    })
                }
                ApprovalPreview::Search { pattern, .. } => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Grep {
                        pattern: pattern.clone(),
                        matches: None,
                    })
                }
                _ => crate::tools::ToolRender::Inline(crate::tools::InlineRender::Generic {
                    args: request
                        .resources
                        .iter()
                        .take(4)
                        .map(|resource| ("resource".into(), resource.clone()))
                        .collect(),
                }),
            };
        }
        self.enforce_parts();
    }

    pub fn apply_tool_argument_stream(
        &mut self,
        turn: &WorkerTurnId,
        event: &oc_core::tool_stream::ToolStreamEvent,
    ) {
        use oc_core::tool_stream::{PENDING_TOOL_MAX, ToolStreamEvent};
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let key = |id: &oc_core::tool_stream::ToolStreamIdentity| {
            format!(
                "pending:{}:{}",
                id.round,
                serde_json::json!([id.item_id, id.call_id])
            )
        };
        match event {
            ToolStreamEvent::Pending {
                identity,
                name,
                preview,
                ..
            } => {
                if identity.item_id.is_empty()
                    || identity.call_id.is_empty()
                    || identity.item_id.len() > 512
                    || identity.call_id.len() > 512
                    || name.is_empty()
                    || name.len() > 512
                {
                    return;
                }
                if identity.round < self.pending_tool_round {
                    return;
                }
                if identity.round > self.pending_tool_round {
                    // Round-local provider drafts can expire. Owner-prepared
                    // waits recovered ahead of this broadcast remain authoritative.
                    for part in &mut self.live_parts {
                        if matches!(part, LivePart::Tool { card, .. } if card.state == "argument_stream")
                        {
                            *part = LivePart::Vacant;
                        }
                    }
                    self.pending_tool_seen.clear();
                    self.pending_tool_round = identity.round;
                }
                let op = key(identity);
                // A queue snapshot can recover the prepared card before older
                // broadcast argument snapshots are drained. Never replace it
                // with unfinished bytes or create a second card for that call.
                if self.session.as_ref().and_then(|session| self.approvals.request_for_call(&session.0, &turn.0, &identity.call_id)).is_some_and(|request| self.live_parts.iter().any(|part| matches!(part, LivePart::Tool { card, .. } if card.state == "permission_pending" && (card.op == request.binding.operation || card.op.splitn(3, ':').nth(2).and_then(|json| serde_json::from_str::<[String; 2]>(json).ok()).is_some_and(|ids| ids[1] == identity.call_id))))) { return; }
                let found = self.live_parts.iter().position(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op && card.state == "argument_stream"));
                if found.is_none()
                    && (self.pending_tool_seen.contains(&op)
                        || self.pending_tool_seen.len() >= PENDING_TOOL_MAX)
                {
                    return;
                }
                let mut card = card_from_row(&ToolOpView {
                    rowid: 0,
                    op: op.clone(),
                    name: name.clone(),
                    state: "argument_stream".into(),
                    input: None,
                    output: None,
                    output_bytes: 0,
                    output_truncated: false,
                    patch_effects: None,
                    dcp: None,
                    dcp_topic: None,
                });
                card.render =
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Generic {
                        args: Vec::new(),
                    });
                // Bounded raw prefix is presentation text only, not input JSON.
                let mut end = preview
                    .len()
                    .min(oc_core::tool_stream::ARGUMENT_PREVIEW_MAX);
                while !preview.is_char_boundary(end) {
                    end -= 1;
                }
                card.input_preview = preview[..end].to_string();
                let part = LivePart::Tool {
                    card: Box::new(card),
                    input: String::new(),
                };
                if let Some(index) = found {
                    self.live_parts[index] = part;
                } else {
                    self.freeze_reasoning();
                    self.freeze_text();
                    self.pending_tool_seen.push(op);
                    self.live_parts.push(part);
                }
                self.enforce_parts();
            }
            ToolStreamEvent::Linked { identity, op } => {
                let key = key(identity);
                let prefix = format!("pending:{}:", identity.round);
                for part in &mut self.live_parts {
                    let LivePart::Tool { card, .. } = part else {
                        continue;
                    };
                    if !matches!(
                        card.state.as_str(),
                        "argument_stream" | "permission_pending"
                    ) {
                        continue;
                    }
                    if card.op == key {
                        card.op.clone_from(op);
                        card.state = "started".into();
                        card.input_preview.clear();
                    } else if card
                        .op
                        .strip_prefix(&prefix)
                        .and_then(|json| serde_json::from_str::<[String; 2]>(json).ok())
                        .is_some_and(|ids| ids[1] == identity.call_id)
                    {
                        // A canonical call supersedes conflicting announcements;
                        // never link the wrong provider item or show both cards.
                        *part = LivePart::Vacant;
                    }
                }
            }
            ToolStreamEvent::Clear { round } => {
                let prefix = format!("pending:{round}:");
                for part in &mut self.live_parts {
                    if matches!(part, LivePart::Tool { card, .. } if matches!(card.state.as_str(), "argument_stream" | "permission_pending") && card.op.starts_with(&prefix))
                    {
                        *part = LivePart::Vacant;
                    }
                }
                self.prune_reasoning();
            }
        }
    }

    fn discard_pending_tools(&mut self) {
        for part in &mut self.live_parts {
            if matches!(part, LivePart::Tool { card, .. } if matches!(card.state.as_str(), "argument_stream" | "permission_pending"))
            {
                *part = LivePart::Vacant;
            }
        }
    }

    /// Apply a recorded tool-call outcome: rebuild the matching card from the
    /// stored input plus the outcome, then drop the transient input.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
    ) {
        self.apply_tool_finished_with_effects(
            turn,
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
            None,
        );
    }

    /// Apply the owner's bounded result-derived mutation projection.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished_with_effects(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
        patch_effects: Option<oc_core::patch::PatchEffects>,
    ) {
        self.apply_tool_finished_with_presentation(
            turn,
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
            patch_effects,
            None,
        );
    }

    /// The same frozen DCP metadata as durable history, attached to one operation.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished_with_presentation(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
        patch_effects: Option<oc_core::patch::PatchEffects>,
        dcp: Option<oc_core::dcp_view::DcpRunSnapshot>,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let dcp = dcp.filter(|run| {
            run.operation_id == op && self.session.as_ref().is_some_and(|id| id.0 == run.session)
        });
        let already_confirmed = self.live_parts.iter().any(|part| matches!(part,
            LivePart::Tool { card, .. } if card.op == op && matches!(&card.render, crate::tools::ToolRender::Dcp(view) if view.snapshot.is_some())));
        if already_confirmed {
            return;
        }
        if state == "completed"
            && let Some(notice) = dcp
                .as_ref()
                .and_then(|run| crate::dcp_view::toast_text(run, &self.chrome.dcp))
        {
            self.push_transient_note(&notice, NoteVariant::Info);
        }
        let outcome = ToolOpView {
            rowid: 0,
            op: op.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            input: None,
            output: Some(output.to_string()),
            output_bytes,
            output_truncated,
            patch_effects,
            dcp,
            dcp_topic: None,
        };
        if let Some(LivePart::Tool { card, input }) = self
            .live_parts
            .iter_mut()
            .rev()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            let mut row = outcome;
            row.name = card.name.clone();
            row.input = Some(std::mem::take(input));
            let mut finished = card_from_row(&row);
            if let crate::tools::ToolRender::Dcp(view) = &mut finished.render {
                view.color_index = self.live_agent_color_index;
            }
            // Reject never emits Started, so its transient card may have only
            // owner-prepared targets rather than canonical arguments. Retain
            // those labels, not a proposed diff or a claim of applied effects.
            if card.name == "apply_patch"
                && finished.files.is_empty()
                && matches!(state, "denied" | "cancelled")
                && crate::history::permission_output(output).is_some()
                && finished
                    .patch_effects
                    .as_ref()
                    .is_none_or(|effects| effects.files.is_empty())
            {
                finished.files = card.files.clone();
                finished.files_truncated = card.files_truncated;
            }
            **card = finished;
            self.enforce_parts();
            self.sync_compaction_clock();
            return;
        }
        // The intent event was not observed (e.g. a late subscription): the
        // card appears with the outcome only, never with an invented input.
        let mut card = card_from_row(&outcome);
        if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
            view.color_index = self.live_agent_color_index;
        }
        self.live_parts.push(LivePart::Tool {
            card: Box::new(card),
            input: String::new(),
        });
        self.enforce_parts();
        self.sync_compaction_clock();
    }

    /// Evict oldest live parts while the count or byte cap is exceeded; the
    /// parts are transient (a reload restores committed history).
    fn enforce_parts(&mut self) {
        self.invalidate_transcript();
        while self.live_parts.len() > LIVE_PARTS_MAX
            || self
                .live_parts
                .iter()
                .map(LivePart::retained_bytes)
                .sum::<usize>()
                > WINDOW_BYTES
        {
            self.live_parts.remove(0);
            self.live_part_offset += 1;
            self.live_preview_truncated = true;
        }
        self.prune_reasoning();
    }

    /// Footer metadata for the finished turn from real state: the effective
    /// model label, the measured turn duration, provider usage and the
    /// interrupt marker. Absent data stays `None` (the footer omits it).
    fn finish_meta(&mut self, interrupted: bool, duration_ms: u64) -> AssistantMeta {
        let model = self.live_model_label.take().or_else(|| {
            self.picker.as_ref().and_then(|picker| {
                let selection = picker.selection()?;
                Some(
                    selection
                        .entry
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&selection.id)
                        .to_string(),
                )
            })
        });
        let usage = self.turn_usage.take();
        AssistantMeta {
            model,
            duration_ms: (duration_ms > 0).then_some(duration_ms),
            input_tokens: usage.map(|usage| usage.input_tokens),
            output_tokens: usage.map(|usage| usage.output_tokens),
            context_usage: None,
            streamed_ms: usage.map(|usage| usage.streamed_ms),
            session_tps: None,
            interrupted,
            status: self.live_terminal_status.take(),
            agent_color_index: self.live_agent_color_index,
        }
    }

    /// Completed reasoning block for the turn, with its measured duration
    /// (`part.time.completed - part.time.created` when text followed, else the
    /// elapsed reasoning window).
    fn take_reasoning(&mut self) -> Option<ReasoningBlock> {
        if self.live_reasoning.is_empty() {
            return None;
        }
        let duration_ms = match (
            self.reasoning_started.take(),
            self.reasoning_finished.take(),
        ) {
            (Some(started), Some(finished)) => {
                Some(finished.saturating_duration_since(started).as_millis() as u64)
            }
            _ => None,
        };
        Some(ReasoningBlock {
            text: std::mem::take(&mut self.live_reasoning),
            duration_ms,
            running: false,
            expanded: false,
            toggleable: true,
            identity: Some(crate::messages::ReasoningIdentity::Live(
                self.reasoning_epoch,
                self.live_part_offset + self.live_parts.len(),
            )),
        })
    }

    fn line_count(&self) -> usize {
        self.transcript_lines(0, u16::MAX).len()
    }

    fn max_scroll(&self) -> usize {
        self.viewport.get().map_or_else(
            || self.line_count().saturating_sub(VIEWPORT_LINES),
            |view| view.total.saturating_sub(view.height as usize),
        )
    }

    fn scroll_for_current_view(&self) -> usize {
        if self
            .viewport
            .get()
            .is_some_and(|view| view.requested_scroll == self.scroll)
        {
            self.display_scroll()
        } else {
            self.scroll
        }
    }

    /// Actual rendered geometry for input-driven row scrolling.
    pub fn observe_viewport(&self, height: u16, rendered_rows: usize) {
        self.observe_transcript_viewport(0, 0, height, rendered_rows, self.scroll);
    }

    pub(crate) fn observe_transcript_viewport(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
        total: usize,
        displayed_scroll: usize,
    ) {
        let mut anchor = self.completion_anchor.borrow_mut();
        if anchor
            .as_ref()
            .is_some_and(|anchor| anchor.requested_scroll != self.scroll)
        {
            anchor.take();
        } else if let Some(anchor) = anchor.as_mut() {
            anchor.pending = false;
        }
        self.viewport.set(Some(TranscriptViewport {
            width,
            terminal_width,
            height,
            total,
            requested_scroll: self.scroll,
            displayed_scroll: displayed_scroll.min(total.saturating_sub(height as usize)),
        }));
    }

    /// Latest measured context, never DCP's estimate or a renderer constant.
    pub fn context_usage(&self) -> Option<(u64, Option<u64>)> {
        let usage = self
            .turn_usage
            .as_ref()
            .map(|u| (u.input_tokens, u.output_tokens))
            .or_else(|| {
                // Live usage is handled above; frozen/live/compaction rows
                // have no footer measurement. Borrow the retained durable tail.
                self.window.rows().iter().rev().find_map(|r| {
                    let m = r.meta.as_ref()?;
                    m.context_usage
                        .or_else(|| Some((m.input_tokens?, m.output_tokens?)))
                })
            })?;
        let limit = self
            .picker
            .as_ref()
            .and_then(|p| p.selection())
            .and_then(|s| s.entry.pointer("/limit/context"))
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0);
        Some((usage.0.saturating_add(usage.1), limit))
    }
}

/// A candidate identity exists only for an actual fresh submission, never for
/// an idle Home. The counter disambiguates submissions within a process even
/// when the clock resolution is coarse (including immediate rejected retries).
fn fresh_session_id() -> SessionId {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |now| now.as_nanos());
    SessionId(format!(
        "s-tui-{nanos:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Mark a command as fully handled, or request its snapshot when the view
/// has never loaded one.
fn open_snapshot(outcome: &mut KeyOutcome, loaded: bool, intent: PanelIntent) {
    if loaded {
        outcome.consumed_input = true;
    } else {
        outcome.intent = Some(intent);
    }
}

/// One bounded render row for a tool card.
fn card_row(card: &ToolCard) -> HistoryRow {
    // `apply_patch` shows a bounded diff (touched files with +/- counts and
    // hunk counts) instead of the raw patch bytes; other tools keep the
    // parsed path list. Never a second copy of a large payload.
    let files = if let Some(effects) = &card.patch_effects {
        let listed = effects
            .files
            .iter()
            .take(crate::history::CARD_FILES)
            .map(|file| {
                let mut text = format!(
                    "{} +{} -{}",
                    file.destination.as_ref().unwrap_or(&file.path),
                    file.additions,
                    file.deletions
                );
                if !file.hunks.is_empty() {
                    text.push_str(&format!(
                        " ({}{}h{})",
                        if file.truncated { "≥" } else { "" },
                        file.hunks.len(),
                        if file.truncated { " preview" } else { "" }
                    ));
                }
                text
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            " [{listed}{}] confirmed {}f +{} -{}",
            if effects.truncated { ", …" } else { "" },
            effects.total_files,
            effects.additions,
            effects.deletions
        )
    } else {
        match &card.diff {
            Some(diff) => {
                let listed = diff
                    .files
                    .iter()
                    .take(crate::history::CARD_FILES)
                    .map(|file| {
                        let marker = match file.change {
                            "Add" => "+",
                            "Delete" => "-",
                            _ => "~",
                        };
                        let mut text = format!(
                            "{marker}{} +{} -{}",
                            file.path, file.additions, file.removals
                        );
                        if file.hunks > 0 {
                            text.push_str(&format!(" ({}h)", file.hunks));
                        }
                        if let Some(target) = &file.move_to {
                            text.push_str(&format!(" -> {target}"));
                        }
                        text
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let suffix = if diff.truncated || diff.files.len() > crate::history::CARD_FILES {
                    ", …"
                } else {
                    ""
                };
                format!(
                    " [{listed}{suffix}] request preview (not confirmed) {}f +{} -{}",
                    diff.files.len(),
                    diff.additions,
                    diff.removals
                )
            }
            None if card.files.is_empty() => String::new(),
            None => {
                let suffix = if card.files_truncated { ", …" } else { "" };
                format!(" [{}{suffix}]", card.files.join(", "))
            }
        }
    };
    let output = if card.output_preview.is_empty() {
        String::new()
    } else if card.output_truncated {
        format!(
            " -> {}…[+{} bytes stored]",
            card.output_preview,
            card.output_bytes.max(0)
        )
    } else {
        format!(" -> {}", card.output_preview)
    };
    // Diff first: a long operation id must never push the diff off a narrow
    // panel row.
    HistoryRow {
        message_id: None,
        seq: i64::MAX,
        role: String::new(),
        text: format!(
            "{} {}{}{} ({})",
            card.name, card.state, files, output, card.op
        ),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        tool: None,
    }
}

/// Assistant footer row: no text, no reasoning, footer metadata only; the
/// upstream footer follows every part of the assistant message
/// (`routes/session/index.tsx:1934-1985`).
fn footer_row(agent: Option<String>, meta: AssistantMeta) -> HistoryRow {
    HistoryRow {
        message_id: None,
        seq: i64::MAX,
        role: "assistant".to_string(),
        text: String::new(),
        agent,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: Some(meta),
        tool: None,
    }
}

fn clamp_cursor(cursor: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = cursor as isize + delta;
    next.clamp(0, len as isize - 1) as usize
}

/// Rebuild the adapter catalog view from the application snapshot so the
/// picker keeps its exact-id/variant validation over fresh discovery data.
fn catalog_from_snapshot(snapshot: &CatalogSnapshot) -> ModelCatalog {
    let mut models = BTreeMap::new();
    for entry in &snapshot.models {
        let variants: serde_json::Map<String, serde_json::Value> = entry
            .variants
            .iter()
            .map(|variant| {
                let mut value = serde_json::Map::new();
                if variant.disabled {
                    value.insert("disabled".to_string(), serde_json::json!(true));
                }
                if let Some(effort) = &variant.reasoning_effort {
                    value.insert("reasoningEffort".to_string(), serde_json::json!(effort));
                }
                let value = serde_json::Value::Object(value);
                (variant.name.clone(), value)
            })
            .collect();
        models.insert(
            entry.id.clone(),
            serde_json::json!({
                "name": if entry.display_name.is_empty() { &entry.id } else { &entry.display_name },
                "provider_name": if entry.provider_name.is_empty() { &snapshot.provider } else { &entry.provider_name },
                "cost": entry.price.as_ref().map(|p|serde_json::json!({"input":p.input,"output":p.output})),
                "limit": { "context": entry.context_known.then_some(entry.context), "output": entry.output_known.then_some(entry.output) },
                "variants": variants,
            }),
        );
    }
    ModelCatalog {
        provider: snapshot.provider.clone(),
        models,
    }
}

/// Scripted driver used by tests: holds one broadcast subscription like the
/// real binary event loop and applies worker events in order.
pub struct ScriptDriver {
    rx: tokio::sync::broadcast::Receiver<CoreEvent>,
}

impl ScriptDriver {
    /// Attach to the same handle the `TuiState` uses.
    pub fn attach(app: &CoreApp) -> Self {
        Self {
            rx: app.subscribe(),
        }
    }

    /// Pump worker events into `state` until idle (no active turn) or
    /// timeout. Returns terminal text or interrupt marker.
    pub async fn pump_until_idle(
        &mut self,
        state: &mut TuiState,
        timeout: Duration,
    ) -> PumpOutcome {
        loop {
            if !state.is_busy() && state.status != TuiStatus::Streaming {
                return PumpOutcome::Idle;
            }
            let event = tokio::time::timeout(timeout, self.rx.recv()).await;
            state.poll_submission();
            match event {
                Ok(Ok(CoreEvent::PermissionAsked(_)))
                | Ok(Ok(CoreEvent::PermissionResolved { .. })) => {
                    if let Ok(pending) = state.app.pending_approvals().await {
                        let requests = pending
                            .into_iter()
                            .filter(|r| {
                                state
                                    .session
                                    .as_ref()
                                    .is_some_and(|s| s.0 == r.binding.session)
                            })
                            .map(|r| (r, state.parent_id.is_some()))
                            .collect();
                        state.approvals.reconcile(requests);
                    }
                }
                Ok(Ok(CoreEvent::Compaction(snapshot))) => state.apply_compaction(snapshot),
                Ok(Ok(CoreEvent::SessionTitleUpdated { session, title })) => {
                    if state.attached_session() == Some(&session) {
                        state.session_title = Some(title);
                    }
                }
                Err(_) => return PumpOutcome::Timeout,
                Ok(Err(_)) => return PumpOutcome::Closed,
                Ok(Ok(CoreEvent::TurnStarted {
                    turn, model_switch, ..
                })) => {
                    if let Some(notice) = model_switch {
                        state.apply_model_switch(&turn, &notice);
                    }
                }
                Ok(Ok(CoreEvent::TurnPresentation {
                    turn, projection, ..
                })) => state.apply_presentation(&turn, &projection),
                Ok(Ok(CoreEvent::TurnFailed { turn, error, .. })) => {
                    state.apply_failed(&turn, &error);
                    return PumpOutcome::Closed;
                }
                Ok(Ok(CoreEvent::TextDelta { turn, delta, .. })) => {
                    state.apply_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::ReasoningDelta { turn, delta, .. })) => {
                    state.apply_reasoning_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::ReasoningItemEnded { turn, .. })) => {
                    state.apply_reasoning_item_ended(&turn);
                }
                Ok(Ok(CoreEvent::ToolArgumentStream { turn, event, .. })) => {
                    state.apply_tool_argument_stream(&turn, &event);
                }
                Ok(Ok(CoreEvent::ToolCallStarted {
                    turn,
                    op,
                    name,
                    input,
                    dcp_topic,
                    ..
                })) => {
                    state
                        .apply_tool_started_with_presentation(&turn, &op, &name, &input, dcp_topic);
                }
                Ok(Ok(CoreEvent::ToolCallFinished {
                    turn,
                    op,
                    name,
                    state: tool_state,
                    output,
                    output_bytes,
                    output_truncated,
                    patch_effects,
                    dcp,
                    ..
                })) => {
                    state.apply_tool_finished_with_presentation(
                        &turn,
                        &op,
                        &name,
                        &tool_state,
                        &output,
                        output_bytes,
                        output_truncated,
                        patch_effects,
                        dcp,
                    );
                }
                Ok(Ok(CoreEvent::TurnUsage {
                    turn,
                    input_tokens,
                    output_tokens,
                    streamed_ms,
                    ..
                })) => {
                    state.apply_usage(&turn, input_tokens, output_tokens, streamed_ms);
                }
                Ok(Ok(CoreEvent::TurnFinished {
                    turn,
                    text,
                    duration_ms,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_finished(&turn, &text, duration_ms);
                        return PumpOutcome::Finished(text);
                    }
                }
                Ok(Ok(CoreEvent::TurnInterrupted {
                    turn,
                    partial,
                    duration_ms,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_interrupted(&turn, &partial, duration_ms);
                        return PumpOutcome::Interrupted(partial);
                    }
                }
            }
        }
    }
}

/// Terminal pump result.
#[derive(Debug, PartialEq, Eq)]
pub enum PumpOutcome {
    /// Turn finished with full text.
    Finished(String),
    /// Turn interrupted with partial text.
    Interrupted(String),
    /// Already idle (no active turn).
    Idle,
    /// Timed out waiting for events.
    Timeout,
    /// Channel closed.
    Closed,
}

#[cfg(test)]
mod tests;
