//! One visible-projection build owns the existing per-view Markdown cache.
//! Only keys coalesce behind it; neither TuiState nor another source snapshot
//! crosses the worker boundary. Finished stale work can still return its cache.
use super::*;
use ratatui::style::Color;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};

const SYNC_SOURCE_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    epoch: u64,
    owner: (u64, u64),
    revision: u64,
    window: u64,
    lengths: (usize, usize),
    pub(super) dimensions: (u16, u16, u16),
    scroll: usize,
    chrome: oc_core::queries::TuiChrome,
    thinking: bool,
    reasoning: BTreeSet<crate::messages::ReasoningIdentity>,
    exploration: BTreeSet<String>,
    errors: BTreeSet<String>,
    instructions: BTreeSet<String>,
    calls: BTreeMap<String, BTreeSet<usize>>,
}

#[derive(Clone)]
pub(super) struct PaintedTop {
    pub(super) part: transcript::CompletionPart,
    pub(super) row: usize,
    pub(super) requested_scroll: usize,
}

#[derive(Default)]
pub(super) struct Work {
    epoch: u64,
    flight: Option<Flight>,
    desired: Option<Key>,
    pub(super) painted_top: Option<PaintedTop>,
    cache_bytes: usize,
    suspended: bool,
    #[cfg(test)]
    hold: Option<Arc<TestHold>>,
    #[cfg(test)]
    started: usize,
}

struct Flight {
    key: Key,
    cancel: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<Output>,
}

struct Input {
    key: Key,
    rows: Vec<HistoryRow>,
    live: Option<usize>,
    runs: Vec<Vec<crate::messages::ReasoningIdentity>>,
    colors: BTreeMap<String, Color>,
    fallback: Color,
    delegations: BTreeMap<String, usize>,
    previous: Option<TranscriptViewport>,
    anchor: Option<CompletionAnchor>,
    painted_top: Option<PaintedTop>,
    attached: bool,
}

struct Output {
    cache: crate::messages::MarkdownCache,
    key: Key,
    view: Option<VisibleTranscriptProjection>,
    reasoning: bool,
}

pub(super) fn finish_reasoning(
    rows: &mut [HistoryRow],
    runs: &[Vec<crate::messages::ReasoningIdentity>],
    thinking: bool,
    expanded: &BTreeSet<crate::messages::ReasoningIdentity>,
) {
    for run in runs {
        if crate::styled::render_cancelled() {
            return;
        }
        let first = run.iter().copied().find(|id| {
            rows.iter().any(|row| {
                row.reasoning
                    .as_ref()
                    .is_some_and(|reasoning| reasoning.identity == Some(*id))
                    && crate::messages::visible_reasoning(row)
            })
        });
        for row in rows.iter_mut() {
            let Some(id) = row
                .reasoning
                .as_ref()
                .and_then(|part| part.identity)
                .filter(|id| run.contains(id))
            else {
                continue;
            };
            let selected = if crate::messages::visible_reasoning(row) {
                first.unwrap_or(id)
            } else {
                id
            };
            if let Some(reasoning) = &mut row.reasoning {
                reasoning.expanded = thinking || expanded.contains(&selected);
                reasoning.toggleable = !thinking;
            }
        }
    }
}

fn build(
    mut input: Input,
    cache: &std::cell::RefCell<crate::messages::MarkdownCache>,
) -> Option<(VisibleTranscriptProjection, bool)> {
    finish_reasoning(
        &mut input.rows,
        &input.runs,
        input.key.thinking,
        &input.key.reasoning,
    );
    if crate::styled::render_cancelled() {
        return None;
    }
    let rows = &input.rows;
    let key = &input.key;
    let (width, terminal_width, height) = key.dimensions;
    let color = |agent: Option<&str>| {
        agent
            .and_then(|id| input.colors.get(id).copied())
            .unwrap_or(input.fallback)
    };
    let expanded = |target: crate::messages::ToolExpansion<'_>| match target {
        crate::messages::ToolExpansion::Operation(op) => key.exploration.contains(op),
        crate::messages::ToolExpansion::InlineError(op) => key.errors.contains(op),
        crate::messages::ToolExpansion::Instructions(op) => key.instructions.contains(op),
        crate::messages::ToolExpansion::ExecuteCall(op, call) => {
            key.calls.get(op).is_some_and(|calls| calls.contains(&call))
        }
    };
    let positions = |part: usize, offset: usize| {
        let mut top = None;
        let total = crate::messages::transcript_part_positions(
            rows,
            Theme::dark(),
            (width, terminal_width, input.live),
            &color,
            cache,
            &expanded,
            |index, start, end| {
                if index == part && end > start {
                    top = Some(start + offset.min(end - start - 1));
                }
            },
        );
        top.map(|top| total.saturating_sub(height as usize).saturating_sub(top))
    };
    let resized = input
        .previous
        .is_some_and(|view| (view.width, view.terminal_width, view.height) != key.dimensions);
    let semantic = if key.scroll == 0 {
        None
    } else {
        input
            .anchor
            .as_ref()
            .filter(|anchor| anchor.requested_scroll == key.scroll && (anchor.pending || resized))
            .and_then(|anchor| {
                let index = if let Some(component) = &anchor.component {
                    rows.iter().position(|row| {
                        row.message_id.as_ref() == Some(&anchor.message)
                            && row
                                .position
                                .as_ref()
                                .is_some_and(|position| &position.component == component)
                    })
                } else {
                    rows.iter()
                        .enumerate()
                        .filter(|(_, row)| row.message_id.as_ref() == Some(&anchor.message))
                        .nth(anchor.part)
                        .map(|(index, _)| index)
                }?;
                positions(index, anchor.row)
            })
            .or_else(|| {
                input
                    .painted_top
                    .as_ref()
                    .filter(|top| top.requested_scroll == key.scroll)
                    .and_then(|top| positions(top.part.resolve(rows)?, top.row))
            })
    };
    if crate::styled::render_cancelled() {
        return None;
    }
    let render = |height, scroll| {
        crate::messages::visible_transcript_targets(
            rows,
            Theme::dark(),
            (width, terminal_width),
            (height, scroll, input.live),
            color,
            cache,
            &expanded,
        )
    };
    let scroll = if key.scroll == 0 {
        0
    } else if let Some(scroll) = semantic {
        scroll
    } else if let Some(previous) = input
        .previous
        .filter(|previous| previous.requested_scroll == key.scroll)
    {
        if resized {
            let (_, total, _, _, _, _) = render(0, 0);
            let top = previous
                .total
                .saturating_sub(previous.height as usize)
                .saturating_sub(previous.displayed_scroll);
            total.saturating_sub(height as usize).saturating_sub(top)
        } else {
            previous.displayed_scroll
        }
    } else {
        key.scroll
    };
    let (mut lines, total, mut targets, mut children, mut hits, mut top) =
        render(height as usize, scroll);
    let scroll = if key.scroll > 0
        && semantic.is_none()
        && !resized
        && let Some(previous) = input
            .previous
            .filter(|view| view.requested_scroll == key.scroll)
        && previous.total != total
    {
        let at = previous
            .total
            .saturating_sub(previous.height as usize)
            .saturating_sub(previous.displayed_scroll);
        let anchored = total.saturating_sub(height as usize).saturating_sub(at);
        (lines, _, targets, children, hits, top) = render(height as usize, anchored);
        anchored
    } else {
        scroll
    }
    .min(total.saturating_sub(height as usize));
    if crate::styled::render_cancelled() {
        return None;
    }
    let reasoning = input.attached && crate::messages::has_running_reasoning(rows);
    let top = top.map(|(index, row)| PaintedTop {
        part: transcript::CompletionPart::capture(rows, index, input.live == Some(index)),
        row,
        requested_scroll: key.scroll,
    });
    let animations = lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.animation_cells()
                .into_iter()
                .map(move |(byte, source)| (row, byte, source))
        })
        .map(|(row, byte, source)| {
            let source = match source {
                crate::styled::AnimationSource::Tool(ref op) => input
                    .delegations
                    .get(op)
                    .map(|slot| crate::styled::AnimationSource::Delegation(*slot))
                    .unwrap_or(source),
                _ => source,
            };
            (row, byte, source)
        })
        .collect();
    let viewport = Some(TranscriptViewport {
        width,
        terminal_width,
        height,
        total,
        requested_scroll: key.scroll,
        displayed_scroll: scroll,
    });
    Some((
        VisibleTranscriptProjection {
            key: input.key,
            dimensions: (width, terminal_width, height),
            viewport,
            lines: lines.into(),
            total,
            scroll,
            targets,
            child_targets: children,
            animations,
            hits,
            top,
        },
        reasoning,
    ))
}

fn run(input: Input, cache: crate::messages::MarkdownCache, cancel: Arc<AtomicBool>) -> Output {
    let key = input.key.clone();
    let cache = std::cell::RefCell::new(cache);
    let result = crate::styled::with_render_cancellation(cancel, || {
        let result = build(input, &cache);
        if crate::styled::render_cancelled() {
            None
        } else {
            result
        }
    });
    let (view, reasoning) =
        result.map_or((None, false), |(view, reasoning)| (Some(view), reasoning));
    Output {
        cache: cache.into_inner(),
        key,
        view,
        reasoning,
    }
}

impl TuiState {
    fn projection_key(&self, dimensions: (u16, u16, u16)) -> Key {
        Key {
            epoch: self.projection_work.borrow().epoch,
            owner: self.view_identity(),
            revision: self.transcript_revision,
            window: self.window.revision(),
            lengths: (self.live_text.len(), self.live_reasoning.len()),
            dimensions,
            scroll: self.scroll,
            chrome: self.chrome.clone(),
            thinking: self.thinking_expanded,
            reasoning: self.reasoning_expanded.clone(),
            exploration: self.exploration_expanded.clone(),
            errors: self.inline_errors_expanded.clone(),
            instructions: self.instructions_expanded.clone(),
            calls: self.execute_calls_expanded.clone(),
        }
    }

    pub(super) fn large_projection(&self) -> bool {
        let retained = self.window.rows().iter().map(|row| {
            row.text.len()
                + row.reasoning.as_ref().map_or(0, |part| part.text.len())
                + row.tool.as_ref().map_or(0, |card| card.retained_bytes())
        });
        let frozen = self.live_parts.iter().map(LivePart::retained_bytes);
        let pending = self
            .pending
            .as_ref()
            .map_or(0, |pending| pending.source.len());
        let inbox = self.inbox.iter().map(|item| item.text.len());
        retained
            .chain(frozen)
            .chain(inbox)
            .chain(self.compactions.iter().map(|snapshot| snapshot.body.len()))
            .chain(std::iter::once(pending))
            .try_fold(
                self.live_text
                    .len()
                    .min(crate::messages::LIVE_MARKDOWN_BYTES)
                    + self
                        .live_reasoning
                        .len()
                        .min(crate::messages::LIVE_MARKDOWN_BYTES),
                |sum, bytes| {
                    let total = sum.saturating_add(bytes);
                    (total <= SYNC_SOURCE_BYTES).then_some(total)
                },
            )
            .is_none()
    }

    fn projection_input(&self, key: Key) -> Input {
        let (rows, live, runs) = self.compose_structure(true);
        let mut colors = BTreeMap::new();
        for agent in &self.agents {
            colors.insert(agent.id.clone(), self.agent_color(Some(&agent.id)));
        }
        for id in self.chrome.agent_colors.keys() {
            colors.insert(id.clone(), self.agent_color(Some(id)));
        }
        let delegations = self
            .live_parts
            .iter()
            .enumerate()
            .filter_map(|(index, part)| match part {
                LivePart::Tool {
                    card,
                    delegation: Some(_),
                    ..
                } => Some((card.op.clone(), self.live_part_offset + index)),
                _ => None,
            })
            .collect();
        Input {
            key,
            rows,
            live,
            runs,
            colors,
            fallback: self.agent_color(None),
            delegations,
            previous: self.viewport.get(),
            anchor: self.completion_anchor.borrow().clone(),
            painted_top: self.projection_work.borrow().painted_top.clone(),
            attached: self.tool_spinner_clocks_attached,
        }
    }

    pub(super) fn projection_ready(&self, dimensions: (u16, u16, u16)) -> bool {
        let key = self.projection_key(dimensions);
        self.visible_projection
            .borrow()
            .as_ref()
            .is_some_and(|view| {
                view.key == key
                    && view.viewport.is_some_and(|view| {
                        (view.width, view.terminal_width, view.height) == dimensions
                    })
            })
    }

    pub(super) fn request_projection(&self, dimensions: (u16, u16, u16)) {
        let key = self.projection_key(dimensions);
        let warm = self
            .visible_projection
            .borrow()
            .as_ref()
            .is_some_and(|view| view.key == key);
        {
            let mut work = self.projection_work.borrow_mut();
            if work.desired.as_ref() != Some(&key) {
                work.desired = Some(key.clone());
            }
            if let Some(flight) = &work.flight {
                if flight.key.dimensions.0 != dimensions.0
                    || flight.key.dimensions.1 != dimensions.1
                {
                    flight.cancel.store(true, Ordering::Relaxed);
                }
                return;
            }
            if work.suspended || warm {
                return;
            }
        }
        let input = self.projection_input(key.clone());
        #[cfg(test)]
        self.visible_projection_builds
            .set(self.visible_projection_builds.get() + 1);
        let cache = std::mem::take(&mut *self.markdown_cache.borrow_mut());
        let cancel = Arc::new(AtomicBool::new(false));
        if tokio::runtime::Handle::try_current().is_ok() && self.large_projection() {
            let mut work = self.projection_work.borrow_mut();
            work.cache_bytes = cache
                .retained_bytes()
                .max(crate::messages::MAX_CACHED_BYTES + crate::messages::MAX_INDEX_BYTES);
            #[cfg(test)]
            let hold = work.hold.clone();
            #[cfg(test)]
            {
                work.started += 1;
            }
            let cancelled = cancel.clone();
            let handle = tokio::task::spawn_blocking(move || {
                #[cfg(test)]
                if let Some(hold) = hold {
                    hold.wait(&cancelled);
                }
                run(input, cache, cancelled)
            });
            work.flight = Some(Flight {
                key,
                cancel,
                handle,
            });
        } else {
            self.accept_projection(run(input, cache, cancel));
        }
    }

    fn accept_projection(&self, output: Output) {
        if output.key.epoch != self.projection_work.borrow().epoch {
            return;
        }
        *self.markdown_cache.borrow_mut() = output.cache;
        if self.projection_work.borrow().suspended
            || output.key != self.projection_key(output.key.dimensions)
            || self.projection_work.borrow().desired.as_ref() != Some(&output.key)
        {
            return;
        }
        if let Some(view) = output.view {
            self.reasoning_spinner_running.set(output.reasoning);
            *self.visible_projection.borrow_mut() = Some(view);
        }
    }

    /// Finished handles are polled/joined only; unfinished polling does not
    /// compose sources, traverse history, mutate geometry, or request a draw.
    pub fn poll_transcript_projection(&self) -> bool {
        let result = {
            let mut work = self.projection_work.borrow_mut();
            let Some(flight) = work.flight.as_mut() else {
                return false;
            };
            // The UI waker fires once the output is stored, not on a timer.
            match Pin::new(&mut flight.handle)
                .poll(&mut Context::from_waker(oc_core::wake::waker()))
            {
                Poll::Ready(output) => {
                    work.flight.take();
                    Some(output)
                }
                Poll::Pending => None,
            }
        };
        if let Some(output) = result {
            if let Ok(output) = output {
                self.accept_projection(output);
            }
            return true;
        }
        false
    }

    pub fn has_pending_transcript_projection(&self) -> bool {
        self.projection_work.borrow().flight.is_some()
    }

    pub fn cancel_transcript_projection(&self) {
        let mut work = self.projection_work.borrow_mut();
        work.suspended = true;
        if let Some(flight) = &work.flight {
            flight.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn resume_transcript_projection(&self) {
        self.projection_work.borrow_mut().suspended = false;
    }

    pub async fn finish_transcript_projection(&self) {
        // Keep ownership in Work even if this cleanup future is itself dropped.
        // The RefCell borrow exists only for one poll, never across an await.
        let result = std::future::poll_fn(|cx| {
            let mut work = self.projection_work.borrow_mut();
            let Some(flight) = work.flight.as_mut() else {
                return Poll::Ready(None);
            };
            match Pin::new(&mut flight.handle).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(result) => {
                    work.flight.take();
                    Poll::Ready(Some(result))
                }
            }
        })
        .await;
        if let Some(Ok(output)) = result {
            self.accept_projection(output);
        }
    }

    pub(super) fn reset_transcript_projection(&mut self) {
        self.cancel_transcript_projection();
        let work = self.projection_work.get_mut();
        work.epoch = work.epoch.wrapping_add(1);
        work.desired = None;
        work.painted_top = None;
        work.suspended = false;
        self.visible_projection.get_mut().take();
        *self.markdown_cache.get_mut() = Default::default();
    }

    pub(super) fn projection_cache_bytes(&self) -> usize {
        if self.has_pending_transcript_projection() {
            self.projection_work.borrow().cache_bytes
        } else {
            self.markdown_cache.borrow().retained_bytes()
        }
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        // Controllers normally cancel/join before destruction. This synchronous
        // cancellation join is only the ownership safety net (including unwind),
        // never an async-join claim or a detached reaper.
        if let Some(mut flight) = self.flight.take() {
            flight.cancel.store(true, Ordering::Relaxed);
            loop {
                if flight.handle.is_finished()
                    && Pin::new(&mut flight.handle)
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_ready()
                {
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct TestHold {
    entered: AtomicBool,
    release: AtomicBool,
}
#[cfg(test)]
impl TestHold {
    fn wait(&self, cancel: &AtomicBool) {
        self.entered.store(true, Ordering::Release);
        while !self.release.load(Ordering::Acquire) && !cancel.load(Ordering::Relaxed) {
            std::thread::park_timeout(Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> TuiState {
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, SessionId("projection-worker".into()));
        state.live_parts.push(LivePart::Text(
            "retained immutable source\n\n".repeat(20_000).into(),
        ));
        state
    }

    #[tokio::test]
    async fn held_cold_projection_keeps_input_and_paint_bounded_and_reuses_content_stale_cache() {
        let mut state = state();
        let mut text = String::with_capacity(32 * 1024 * 1024 + 1024);
        text.push_str(&"x".repeat(32 * 1024 * 1024));
        state.live_text = text.into();
        let pointer = state.live_text.as_ptr();
        let hold = Arc::new(TestHold::default());
        state.projection_work.borrow_mut().hold = Some(hold.clone());
        let frame = Rect::new(0, 0, 90, 24);
        let area = crate::shell::transcript_area(&state, frame);
        let (lines, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().contains("Loading transcript"))
        );
        state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
        state.paint_transcript_at(area, &lines, total, scroll, Some(frame), &targets);
        assert!(state.viewport.get().is_none());
        assert!(state.painted_transcript.borrow().is_none());
        assert!(targets.iter().all(Option::is_none));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !hold.entered.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let builds = state.visible_projection_builds.get();
        let started = Instant::now();
        state.handle_key(KeyAction::Char('z')).await;
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(state.input(), "z");
        state.live_text.push_str("delta");
        state.invalidate_transcript();
        assert_eq!(
            state.live_text.as_ptr(),
            pointer,
            "worker snapshot must not pin the 32 MiB open allocation"
        );
        assert_eq!(state.live_text.len(), 32 * 1024 * 1024 + 5);
        for _ in 0..8 {
            assert!(!state.poll_transcript_projection());
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        }
        assert_eq!(state.visible_projection_builds.get(), builds);
        assert_eq!(state.projection_work.borrow().started, 1);
        hold.release.store(true, Ordering::Release);
        state.finish_transcript_projection().await;
        assert!(!state.projection_ready((area.width, frame.width, area.height)));
        let indexes = state.markdown_cache.borrow().source_index_builds();
        assert!(
            indexes > 0,
            "content-stale build returns its useful immutable-source index"
        );
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.finish_transcript_projection().await;
        assert!(state.projection_ready((area.width, frame.width, area.height)));
        assert_eq!(state.markdown_cache.borrow().source_index_builds(), indexes);
        let (lines, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
        state.paint_transcript_at(area, &lines, total, scroll, Some(frame), &targets);
        assert!(state.painted_transcript.borrow().is_some());
        state.live_text.push_str("more");
        assert_eq!(
            state.live_text.as_ptr(),
            pointer,
            "painted completion metadata must not pin open source bytes either"
        );
        assert!(state.markdown_cache.borrow().index_bytes() <= crate::messages::MAX_INDEX_BYTES);
        assert!(
            state.markdown_cache.borrow().retained_bytes()
                <= crate::messages::MAX_CACHED_BYTES + crate::messages::MAX_INDEX_BYTES
        );
        state.cancel_transcript_projection();
        state.finish_transcript_projection().await;
        assert!(!state.has_pending_transcript_projection());
    }

    #[tokio::test]
    async fn projection_width_reset_cancels_and_joins_before_latest_snapshot_and_idle_has_no_poll()
    {
        let mut state = state();
        let hold = Arc::new(TestHold::default());
        state.projection_work.borrow_mut().hold = Some(hold.clone());
        state.visible_transcript_at_viewport(60, 80, 12);
        assert!(state.next_ui_deadline().is_some());
        let mut cleanup = Box::pin(state.finish_transcript_projection());
        assert!(
            cleanup
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        drop(cleanup);
        let owned = state.has_pending_transcript_projection();
        if !owned {
            hold.release.store(true, Ordering::Release);
        }
        assert!(
            owned,
            "dropping a cleanup future must not detach the worker or make room for a second cache owner"
        );
        state.live_parts[0] = LivePart::Text("latest source\n\n".repeat(20_000).into());
        state.invalidate_transcript();
        state.visible_transcript_at_viewport(70, 90, 14);
        state.visible_transcript_at_viewport(80, 100, 16);
        assert_eq!(state.projection_work.borrow().started, 1);
        state.finish_transcript_projection().await;
        assert!(!state.projection_ready((60, 80, 12)));
        assert!(!state.has_pending_transcript_projection());
        hold.release.store(true, Ordering::Release);
        state.visible_transcript_at_viewport(80, 100, 16);
        assert_eq!(state.projection_work.borrow().started, 2);
        state.finish_transcript_projection().await;
        assert!(state.projection_ready((80, 100, 16)));
        assert!(
            state
                .visible_projection
                .borrow()
                .as_ref()
                .unwrap()
                .lines
                .iter()
                .any(|line| line.plain_text().contains("latest source"))
        );
        state.projection_work.borrow_mut().hold = Some(Arc::new(TestHold::default()));
        state.visible_transcript_at_viewport(100, 120, 18);
        state.visible_transcript_at_viewport(80, 100, 16);
        assert!(
            state.projection_ready((80, 100, 16)),
            "returning to the exact warm viewport is immediate"
        );
        assert!(
            state
                .projection_work
                .borrow()
                .flight
                .as_ref()
                .unwrap()
                .cancel
                .load(Ordering::Relaxed),
            "returning to a warm width still cancels the obsolete other-width worker"
        );
        state.reset_transcript_projection();
        assert!(
            state.has_pending_transcript_projection(),
            "reset retains the cancelled handle until joined"
        );
        state.finish_transcript_projection().await;
        assert!(state.visible_projection.borrow().is_none());
        assert_eq!(state.markdown_cache.borrow().retained_bytes(), 0);
        assert!(state.next_ui_deadline().is_none());
    }

    #[test]
    fn open_reasoning_preview_is_bounded_marked_and_does_not_pin_mutable_source() {
        let mut source = String::with_capacity(32 * 1024 * 1024 + 100);
        source.push_str(&"中".repeat(11_000_000));
        let mut source: crate::history::SourceText = source.into();
        let pointer = source.as_ptr();
        let preview = source.preview(
            crate::messages::LIVE_MARKDOWN_BYTES,
            "\n\n… [reasoning preview limited]",
        );
        assert!(preview.len() < crate::messages::LIVE_MARKDOWN_BYTES + 100);
        assert!(preview.ends_with("[reasoning preview limited]"));
        assert!(
            source.starts_with(preview.completion_prefix(crate::messages::LIVE_MARKDOWN_BYTES))
        );
        source.push_str("next");
        assert_eq!(source.as_ptr(), pointer);
        assert_eq!(source.len(), 33_000_004);
    }
}
