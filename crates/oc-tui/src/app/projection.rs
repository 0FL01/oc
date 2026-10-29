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
    structure: u64,
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

impl Key {
    // The only permitted staleness is an append to already-open sources.
    // Every structural, viewport, owner and presentation fence stays exact.
    fn content_compatible(&self, current: &Self) -> bool {
        let mut comparable = current.clone();
        comparable.revision = self.revision;
        comparable.lengths = self.lengths;
        self == &comparable
            && current.revision.wrapping_sub(self.revision) <= u64::MAX / 2
            && self.lengths.0 <= current.lengths.0
            && self.lengths.1 <= current.lengths.1
            && (self.lengths.0 == 0) == (current.lengths.0 == 0)
            && (self.lengths.1 == 0) == (current.lengths.1 == 0)
    }
}

#[derive(Clone)]
pub(super) struct PaintedTop {
    pub(super) part: transcript::CompletionPart,
    pub(super) row: usize,
    pub(super) screen_row: usize,
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
    selection: Vec<Arc<transcript::CompletionPart>>,
    open_reasoning: Option<live::ReasoningSnapshot>,
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
    let open_text = input.live.map(|index| input.rows[index].text.clone());
    let open_reasoning = input.open_reasoning.clone();
    input.materialize_reasoning()?;
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
    let positions = |part: usize, offset: usize, screen_row: usize| {
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
        top.map(|top| {
            total
                .saturating_sub(height as usize)
                .saturating_sub(top.saturating_sub(screen_row))
        })
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
                positions(index, anchor.row, 0)
            })
            .or_else(|| {
                input
                    .painted_top
                    .as_ref()
                    .filter(|top| top.requested_scroll == key.scroll)
                    .and_then(|top| positions(top.part.resolve(rows)?, top.row, top.screen_row))
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
            let (_, total, _, _, _, _, _) = render(0, 0);
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
    let (mut lines, total, mut targets, mut children, mut hits, mut top, mut positions) =
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
        (lines, _, targets, children, hits, top, positions) = render(height as usize, anchored);
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
        screen_row: 0,
        requested_scroll: key.scroll,
    });
    let parts = selection_parts(
        rows,
        input.live,
        &positions,
        total.saturating_sub(height as usize).saturating_sub(scroll),
        lines.len(),
        &input.selection,
    );
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
            open_text,
            open_reasoning,
            top,
            parts,
        },
        reasoning,
    ))
}

impl Input {
    fn materialize_reasoning(&mut self) -> Option<()> {
        if let Some(snapshot) = self.open_reasoning.take()
            && let Some(reasoning) = self
                .live
                .and_then(|index| self.rows.get_mut(index))
                .and_then(|row| row.reasoning.as_mut())
        {
            reasoning.text = snapshot.into_source_unless_cancelled()?;
        }
        Some(())
    }
}

/// Parts a selection point can attach to: every part intersecting the
/// painted rows (plus the one owning a gap above them), and the parts of the
/// current selection endpoints wherever they are now laid out.
fn selection_parts(
    rows: &[HistoryRow],
    live: Option<usize>,
    positions: &[(usize, usize)],
    first: usize,
    height: usize,
    selection: &[Arc<transcript::CompletionPart>],
) -> Vec<(Arc<transcript::CompletionPart>, usize, usize)> {
    let last = first + height;
    let owner = positions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, (start, end))| start < end && *start <= first)
        .map(|(index, _)| index);
    let mut parts: Vec<(Arc<transcript::CompletionPart>, usize, usize)> = positions
        .iter()
        .enumerate()
        .filter(|&(index, &(start, end))| {
            start < end && (Some(index) == owner || (start < last && end > first))
        })
        .map(|(index, &(start, end))| {
            (
                Arc::new(transcript::CompletionPart::capture(
                    rows,
                    index,
                    live == Some(index),
                )),
                start,
                end,
            )
        })
        .collect();
    for part in selection {
        if parts.iter().any(|(known, _, _)| known == part) {
            continue;
        }
        if let Some(&(start, end)) = part.resolve(rows).and_then(|index| positions.get(index)) {
            parts.push((part.clone(), start, end));
        }
    }
    parts
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
            structure: self.transcript_structure,
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
                    + self.live_reasoning.len(),
                |sum, bytes| {
                    let total = sum.saturating_add(bytes);
                    (total <= SYNC_SOURCE_BYTES).then_some(total)
                },
            )
            .is_none()
    }

    fn projection_input(&self, key: Key) -> Input {
        let (rows, live, runs) = self.compose_structure(true);
        let open_reasoning = live
            .filter(|&index| rows[index].reasoning.is_some())
            .map(|_| self.live_reasoning.snapshot());
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
            selection: self
                .selection
                .borrow()
                .iter()
                .flat_map(|selected| [&selected.anchor, &selected.focus])
                .filter_map(|point| point.part.clone())
                .collect(),
            open_reasoning,
        }
    }

    /// Materialize the rows between two selection points from the current
    /// sources, once per copy. Offscreen rows are rendered through the same
    /// bounded painter; nothing is kept beyond this call.
    #[allow(clippy::type_complexity)]
    pub(super) fn selection_lines(
        &self,
        widths: (u16, u16),
        anchor: &SelectionPoint,
        focus: &SelectionPoint,
    ) -> Result<Option<(Vec<Line>, usize, ((usize, usize), (usize, usize)))>, &'static str> {
        let (width, terminal_width) = widths;
        let height = self
            .viewport
            .get()
            .map_or(VIEWPORT_LINES as u16, |view| view.height);
        let painted = self.painted_transcript.borrow();
        let mut input = if let Some((painted, key)) = painted.as_ref().and_then(|painted| {
            painted
                .key
                .as_ref()
                .filter(|key| {
                    key.dimensions == (width, terminal_width, height)
                        && self.projection_content_compatible(key)
                })
                .map(|key| (painted, key))
        }) {
            let mut input = self.projection_input(key.clone());
            if let Some(index) = input.live
                && let Some(text) = &painted.open_text
            {
                input.rows[index].text = text.clone();
            }
            input.open_reasoning = painted.open_reasoning.clone();
            input
        } else {
            self.projection_input(self.projection_key((width, terminal_width, height)))
        };
        drop(painted);
        if input.materialize_reasoning().is_none() {
            return Ok(None);
        }
        finish_reasoning(
            &mut input.rows,
            &input.runs,
            input.key.thinking,
            &input.key.reasoning,
        );
        let rows = &input.rows;
        let key = &input.key;
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
        let cache = &self.markdown_cache;
        let mut positions = vec![(0, 0); rows.len()];
        let total = crate::messages::transcript_part_positions(
            rows,
            Theme::dark(),
            (width, terminal_width, input.live),
            &color,
            cache,
            &expanded,
            |index, start, end| positions[index] = (start, end),
        );
        let resolve = |point: &SelectionPoint| match &point.part {
            None => Some(point.offset),
            Some(part) => part
                .resolve(rows)
                .and_then(|index| positions.get(index))
                .map(|(start, _)| start + point.offset),
        };
        let (Some(a), Some(b)) = (resolve(anchor), resolve(focus)) else {
            return Ok(None);
        };
        let (from, to) = {
            let a = (a, anchor.byte);
            let b = (b, focus.byte);
            if a <= b { (a, b) } else { (b, a) }
        };
        let last = total.saturating_sub(1);
        if from.0 > last {
            return Ok(None);
        }
        let to = if to.0 > last { (last, usize::MAX) } else { to };
        if to.0 - from.0 >= crate::app::MAX_SELECTION_BYTES {
            return Err("Selection exceeds clipboard size limit");
        }
        let (lines, ..) = crate::messages::visible_transcript_targets(
            rows,
            Theme::dark(),
            (width, terminal_width),
            (to.0 + 1 - from.0, total - (to.0 + 1), input.live),
            color,
            cache,
            &expanded,
        );
        Ok(Some((lines, from.0, (from, to))))
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

    pub(super) fn projection_content_compatible(&self, key: &Key) -> bool {
        key.content_compatible(&self.projection_key(key.dimensions))
    }

    pub(super) fn projection_geometry_compatible(
        &self,
        dimensions: (u16, u16, u16),
        total: usize,
        scroll: usize,
    ) -> bool {
        self.visible_projection
            .borrow()
            .as_ref()
            .is_some_and(|view| {
                view.total == total
                    && view.scroll == scroll
                    && view.dimensions == dimensions
                    && self.projection_content_compatible(&view.key)
            })
    }

    /// The visible projection was built for `dimensions`, possibly from
    /// older content while its successor is in flight: its rows and hit
    /// regions are what the transcript paints, so hover follows them.
    pub(super) fn projection_painted(&self, dimensions: (u16, u16, u16)) -> bool {
        self.visible_projection
            .borrow()
            .as_ref()
            .is_some_and(|view| {
                view.viewport.is_some_and(|view| {
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
            || !self.projection_content_compatible(&output.key)
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

    /// A hovered expandable shell block keeps its hover highlight while live
    /// output makes a newer projection build in the background: the painted
    /// (older) projection's rows and hit regions still paint it. A click in
    /// that window acts on nothing until the current projection lands.
    #[tokio::test]
    async fn hover_stays_on_the_painted_projection_while_a_newer_one_builds() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, SessionId("hover-stable".into()));
        let row = |role: &str, text: String, tool| crate::history::HistoryRow {
            position: None,
            message_id: None,
            seq: 1,
            role: role.into(),
            text: text.into(),
            agent: None,
            agent_color_index: None,
            pending_delivery: None,
            chips: vec![],
            reasoning: None,
            child_notice: None,
            shell_notice: None,
            meta: None,
            tool,
        };
        state
            .window
            .push_row(row("user", "filler line\n".repeat(8_000), None));
        let output = format!(
            "exit 0\n{}",
            (0..30)
                .map(|i| format!("recorded output {i}\n"))
                .collect::<String>()
        );
        let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 1,
            op: "shell-op".into(),
            name: "bash".into(),
            state: "completed".into(),
            input: Some(r#"{"argv":["fixture"]}"#.into()),
            output_bytes: output.len() as i64,
            output: Some(output),
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        state
            .window
            .push_row(row("tool", String::new(), Some(Arc::new(card))));
        assert!(
            state.large_projection(),
            "the window builds in the background"
        );
        let frame = Rect::new(0, 0, 80, 48);
        let area = crate::shell::transcript_area(&state, frame);
        let dimensions = (area.width, frame.width, area.height);
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.finish_transcript_projection().await;
        assert!(state.projection_ready(dimensions));
        let (rows, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
        let shell = rows
            .iter()
            .position(|line| line.plain_text().contains("$ fixture"))
            .unwrap();
        let (x, y) = (area.x + 5, area.y + shell as u16);
        let mouse = |kind| MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        state.handle_mouse(mouse(MouseEventKind::Moved), frame);
        let theme = crate::theme::Theme::dark();
        let hovered = Some(theme.decrease(theme.background_raised()));
        let painted = state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
        assert_eq!(painted[shell].style().bg, hovered);
        // Live output: a newer projection is in flight; the older one paints.
        let hold = Arc::new(TestHold::default());
        state.projection_work.borrow_mut().hold = Some(hold.clone());
        state.live_text.push_str("delta");
        state.invalidate_transcript();
        let (stale, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !hold.entered.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!state.projection_ready(dimensions));
        assert_eq!(stale[..], rows[..]);
        for _ in 0..3 {
            let painted =
                state.paint_transcript_at(area, &stale, total, scroll, Some(frame), &targets);
            assert_eq!(
                painted[shell].style().bg,
                hovered,
                "hover kept while building"
            );
        }
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            state.handle_mouse(mouse(kind), frame);
        }
        assert!(
            state.exploration_expanded.is_empty(),
            "actions wait for the current projection"
        );
        hold.release.store(true, Ordering::Release);
        state.finish_transcript_projection().await;
    }

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

    #[tokio::test]
    async fn open_reasoning_projection_shares_chunks_and_receives_the_full_snapshot() {
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, SessionId("reasoning-worker".into()));
        let turn = WorkerTurnId("reasoning-turn".into());
        state.active_turn = Some(turn.clone());
        let body = format!(
            "**Thinking**\n\n{}\n\nTHE END",
            "中 reasoning\n\n".repeat(8_000)
        );
        state.apply_reasoning_delta(&turn, &body);
        assert!(state.large_projection());
        let first = state.live_reasoning.first_chunk_ptr().unwrap();
        let hold = Arc::new(TestHold::default());
        state.projection_work.borrow_mut().hold = Some(hold.clone());
        state.visible_transcript_at_viewport(80, 100, 20);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !hold.entered.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        state.apply_reasoning_delta(&turn, " next");
        assert_eq!(state.live_reasoning.first_chunk_ptr(), Some(first));
        assert_eq!(state.live_reasoning.len(), body.len() + " next".len());
        assert_eq!(state.projection_work.borrow().started, 1);
        hold.release.store(true, Ordering::Release);
        state.finish_transcript_projection().await;
        let input = state.projection_input(state.projection_key((80, 100, 20)));
        let source = input.open_reasoning.unwrap().into_source();
        assert_eq!(source.as_str(), format!("{body} next"));
        assert!(!source.contains("reasoning preview limited"));
        state.cancel_transcript_projection();
        state.finish_transcript_projection().await;
    }

    #[tokio::test]
    async fn live_reasoning_click_and_scroll_survive_an_append_between_press_and_release() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, SessionId("reasoning-live-click".into()));
        let turn = WorkerTurnId("reasoning-live-turn".into());
        state.active_turn = Some(turn.clone());
        let mut body = String::from("**Streaming**\n\n");
        for i in 0..2500 {
            body.push_str(&format!(
                "line-{i:04} public reasoning content goes here\n\n"
            ));
        }
        body.push_str("TAIL-END");
        state.apply_reasoning_delta(&turn, &body);
        let frame = Rect::new(0, 0, 90, 24);
        let area = crate::shell::transcript_area(&state, frame);
        let dimensions = (area.width, frame.width, area.height);
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.finish_transcript_projection().await;
        let (rows, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
        state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
        let header = rows
            .iter()
            .position(|line| line.plain_text().contains("Thinking"))
            .unwrap();
        let mouse = |kind| MouseEvent {
            kind,
            column: area.x + 5,
            row: area.y + header as u16,
            modifiers: KeyModifiers::NONE,
        };
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), frame);
        state.apply_reasoning_delta(&turn, " after-delta");
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), frame);
        assert!(
            state
                .reasoning_expanded
                .contains(&crate::messages::ReasoningIdentity::Live(
                    state.reasoning_epoch,
                    0
                ))
        );
        for _ in 0..3 {
            state.finish_transcript_projection().await;
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
            if state.projection_ready(dimensions) {
                break;
            }
        }
        let (expanded, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        assert!(state.projection_ready(dimensions));
        assert!(
            expanded[header].plain_text().contains("Thinking"),
            "header remains anchored"
        );
        state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
        state.paint_transcript_at(area, &expanded, total, scroll, Some(frame), &targets);

        state.scroll = 0;
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.finish_transcript_projection().await;
        let (tail, total, _) =
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        assert!(
            tail.iter()
                .any(|line| line.plain_text().contains("TAIL-END after-delta"))
        );
        state.scroll = total.saturating_sub(area.height as usize);
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.finish_transcript_projection().await;
        let (head, _, _) =
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        assert!(
            head.iter()
                .any(|line| line.plain_text().contains("line-0000"))
        );
        state.scroll = total.saturating_sub(area.height as usize) / 2;
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.finish_transcript_projection().await;
        let (middle, _, _) =
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        assert!(
            middle
                .iter()
                .any(|line| line.plain_text().contains("line-12"))
        );
        assert!(
            !middle
                .iter()
                .any(|line| line.plain_text().contains("preview limited"))
        );
        state.apply_reasoning_item_ended(&turn);
        assert!(
            matches!(&state.live_parts[0], LivePart::Reasoning { text, .. }
            if text.ends_with("TAIL-END after-delta"))
        );
    }
}
