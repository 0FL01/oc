//! Clipboard work shares the controller's bounded job scheduler and shutdown.
use super::*;

pub(super) struct ClipboardJob {
    target: oc_tui::app::ClipboardTarget,
    location: Option<String>,
    tab: Option<usize>,
    cancel: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<Result<crate::clipboard::Content, String>>,
}

impl ClipboardJob {
    pub(super) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
    pub(super) async fn cancel(self) {
        let _ = self.cancel.send(());
        let _ = self.task.await;
    }
}

pub(super) fn start_clipboard(state: &mut TuiState, deck: &mut LoopState) {
    let Some(target) = state.clipboard_target().filter(|_| !deck.read_only) else {
        return;
    };
    if deck.clipboard_job.is_some() {
        state.push_note("clipboard paste pending");
        return;
    }
    let (cancel, cancelled) = tokio::sync::oneshot::channel();
    deck.clipboard_job = Some(ClipboardJob {
        target,
        location: deck.location.clone(),
        tab: deck.active_tab,
        cancel,
        task: oc_core::wake::spawn(crate::clipboard::read(cancelled)),
    });
}

pub(super) async fn sync_clipboard(state: &mut TuiState, deck: &mut LoopState) -> bool {
    let Some(job) = deck.clipboard_job.as_ref() else {
        return false;
    };
    let current = !deck.read_only
        && job.location == deck.location
        && job.tab == deck.active_tab
        && state.clipboard_target().as_ref() == Some(&job.target);
    if !current {
        deck.clipboard_job
            .take()
            .expect("clipboard job")
            .cancel()
            .await;
        return true;
    }
    if !job.task.is_finished() {
        return false;
    }
    let job = deck.clipboard_job.take().expect("finished clipboard job");
    let outcome = match job.task.await {
        Ok(Ok(crate::clipboard::Content::Png(png))) => state.paste_clipboard_image(png),
        Ok(Ok(crate::clipboard::Content::Text(text))) => state.handle_paste(&text),
        Ok(Err(error)) => {
            state.push_note(&error);
            return true;
        }
        Err(_) => {
            state.push_note("clipboard read failed");
            return true;
        }
    };
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    true
}

#[cfg(test)]
mod tests;
