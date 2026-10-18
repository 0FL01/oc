//! Explicit Linux host-clipboard read. No shell, installation or cross-backend fallback.
use std::{process::Stdio, sync::Arc};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::oneshot,
    time::{Duration, Instant},
};

pub enum Content {
    Png(Arc<[u8]>),
    Text(String),
}

pub async fn read(mut cancel: oneshot::Receiver<()>) -> Result<Content, String> {
    let (program, png, text): (&str, &[&str], &[&str]) = if cfg!(target_os = "linux")
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
    {
        (
            "wl-paste",
            &["--no-newline", "--type", "image/png"],
            &["--no-newline", "--type", "text/plain;charset=utf-8"],
        )
    } else if cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_some() {
        (
            "xclip",
            &["-selection", "clipboard", "-out", "-target", "image/png"],
            &["-selection", "clipboard", "-out", "-target", "UTF8_STRING"],
        )
    } else {
        return Err("host clipboard unavailable (requires Linux Wayland/X11 and wl-paste/xclip; SSH does not transfer images)".into());
    };
    let deadline = Instant::now() + Duration::from_secs(1);
    let content = if let Some(bytes) = command(
        program,
        png,
        oc_core::prompt::IMAGE_BYTES,
        deadline,
        &mut cancel,
    )
    .await?
    {
        oc_adapters::attachments::validate_png(&bytes, &std::sync::atomic::AtomicBool::new(false))
            .map_err(|e| format!("clipboard PNG: {e}"))?;
        Content::Png(bytes.into())
    } else {
        let bytes = command(
            program,
            text,
            oc_core::session::MAX_INPUT_BYTES,
            deadline,
            &mut cancel,
        )
        .await?
        .ok_or("clipboard has no PNG or UTF-8 text")?;
        Content::Text(String::from_utf8(bytes).map_err(|_| "clipboard text is not UTF-8")?)
    };
    if Instant::now() >= deadline {
        return Err("clipboard read timed out".into());
    }
    if cancel.try_recv() != Err(oneshot::error::TryRecvError::Empty) {
        return Err("clipboard paste cancelled".into());
    }
    Ok(content)
}

async fn command(
    program: &str,
    args: &[&str],
    cap: usize,
    deadline: Instant,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<Option<Vec<u8>>, String> {
    // An already-expired shared budget must not race an immediate helper EOF
    // against registration of Tokio's deadline timer (or start another helper).
    if Instant::now() >= deadline {
        return Err("clipboard read timed out".into());
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| format!("host clipboard unavailable: install/configure {program}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("clipboard output unavailable")?
        .take(cap as u64 + 1);
    let mut bytes = Vec::new();
    let outcome = tokio::select! {
        biased;
        _ = cancel => Err("clipboard paste cancelled".to_string()),
        _ = tokio::time::sleep_until(deadline) => Err("clipboard read timed out".to_string()),
        result = async {
            stdout.read_to_end(&mut bytes).await.map_err(|_| "clipboard read failed")?;
            if bytes.len() > cap { return Err("clipboard exceeds size limit") }
            child.wait().await.map_err(|_| "clipboard helper failed")
        } => result.map_err(str::to_string),
    };
    // Kill the captured helper's group as well: a descendant may retain stdout.
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: the child was spawned in its own process group above.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    if outcome.is_err() {
        let _ = child.kill().await;
    }
    let _ = child.wait().await;
    outcome.and_then(|status| {
        if Instant::now() >= deadline {
            Err("clipboard read timed out".into())
        } else {
            Ok(status.success().then_some(bytes))
        }
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
