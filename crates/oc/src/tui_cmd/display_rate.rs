//! Display refresh rate for paint pacing.
//!
//! The paint cap follows the local display: a terminal cannot show frames
//! faster than its monitor refreshes. Detection runs once off the UI task and
//! is best effort: Wayland `wl_output` current mode, then X11 `xrandr`, else
//! 60 Hz. Remote (SSH) sessions keep 60 Hz; the local display is not theirs.

use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) const DEFAULT_HZ: u32 = 60;
const MIN_HZ: u32 = 30;
const MAX_HZ: u32 = 240;
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);
const MAX_OUTPUTS: usize = 16;
const MAX_MESSAGES: usize = 4096;
const MAX_XRANDR_BYTES: usize = 256 * 1024;

/// Paint interval for a refresh rate, clamped to a sane range.
pub(super) fn interval(hz: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(hz.clamp(MIN_HZ, MAX_HZ)))
}

/// Detect once in the background; `None` keeps the default rate.
pub(super) fn spawn_probe() -> tokio::task::JoinHandle<Option<u32>> {
    oc_core::wake::spawn(async {
        tokio::time::timeout(PROBE_TIMEOUT, probe())
            .await
            .ok()
            .flatten()
            .map(|hz| hz.clamp(MIN_HZ, MAX_HZ))
    })
}

async fn probe() -> Option<u32> {
    let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    if var("SSH_CONNECTION").is_some() || var("SSH_TTY").is_some() {
        return None;
    }
    if let Some(path) = wayland_socket(var("WAYLAND_DISPLAY"), var("XDG_RUNTIME_DIR")) {
        let deadline = Instant::now() + PROBE_TIMEOUT;
        if let Ok(Some(hz)) = tokio::task::spawn_blocking(move || wayland(&path, deadline)).await {
            return Some(hz);
        }
    }
    if var("DISPLAY").is_some() {
        return xrandr().await;
    }
    None
}

fn wayland_socket(
    display: Option<std::ffi::OsString>,
    runtime: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let display = PathBuf::from(display?);
    if display.is_absolute() {
        Some(display)
    } else {
        Some(PathBuf::from(runtime?).join(display))
    }
}

/// Minimal Wayland wire exchange: list globals, bind each `wl_output`, and
/// collect the refresh (mHz) of its current mode. No events are retained.
fn wayland(path: &Path, deadline: Instant) -> Option<u32> {
    let stream = UnixStream::connect(path).ok()?;
    let mut wire = Wire { stream, deadline };
    const DISPLAY: u32 = 1;
    const REGISTRY: u32 = 2;
    wire.send(DISPLAY, 1, &[REGISTRY.to_ne_bytes()].concat())?;
    wire.send(DISPLAY, 0, &3_u32.to_ne_bytes())?;
    let mut outputs = Vec::new();
    wire.until_done(3, |object, opcode, args| {
        if object == REGISTRY && opcode == 0 && outputs.len() < MAX_OUTPUTS {
            let name = u32_at(args, 0)?;
            let (interface, rest) = string_at(args, 4)?;
            let version = u32_at(args, rest)?;
            if interface == b"wl_output" {
                outputs.push((name, version));
            }
        }
        Some(())
    })?;
    if outputs.is_empty() {
        return None;
    }
    let first = 4_u32;
    for (offset, (name, version)) in (0_u32..).zip(&outputs) {
        let mut args = name.to_ne_bytes().to_vec();
        push_string(&mut args, b"wl_output");
        args.extend((*version).min(2).to_ne_bytes());
        args.extend((first + offset).to_ne_bytes());
        wire.send(REGISTRY, 0, &args)?;
    }
    let done = first + outputs.len() as u32;
    wire.send(DISPLAY, 0, &done.to_ne_bytes())?;
    let mut best = 0_i32;
    wire.until_done(done, |object, opcode, args| {
        // wl_output.mode(flags, width, height, refresh); flag 1 = current.
        if (first..done).contains(&object) && opcode == 1 {
            let flags = u32_at(args, 0)?;
            let refresh = u32_at(args, 12)? as i32;
            if flags & 1 != 0 {
                best = best.max(refresh);
            }
        }
        Some(())
    })?;
    (best > 0).then(|| ((best + 500) / 1000) as u32)
}

struct Wire {
    stream: UnixStream,
    deadline: Instant,
}

impl Wire {
    fn remaining(&self) -> Option<Duration> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        (!left.is_zero()).then_some(left)
    }

    fn send(&mut self, object: u32, opcode: u16, args: &[u8]) -> Option<()> {
        let size = u32::try_from(8 + args.len()).ok()?;
        let mut message = object.to_ne_bytes().to_vec();
        message.extend(((size << 16) | u32::from(opcode)).to_ne_bytes());
        message.extend(args);
        self.stream.set_write_timeout(self.remaining()).ok()?;
        self.stream.write_all(&message).ok()
    }

    /// Read events until `wl_callback.done` for `callback`; a protocol error
    /// or an exhausted bound/deadline ends the probe.
    fn until_done(
        &mut self,
        callback: u32,
        mut event: impl FnMut(u32, u16, &[u8]) -> Option<()>,
    ) -> Option<()> {
        for _ in 0..MAX_MESSAGES {
            let mut header = [0_u8; 8];
            self.read(&mut header)?;
            let object = u32::from_ne_bytes(header[..4].try_into().ok()?);
            let word = u32::from_ne_bytes(header[4..].try_into().ok()?);
            let (size, opcode) = ((word >> 16) as usize, (word & 0xffff) as u16);
            let mut args = vec![0_u8; size.checked_sub(8)?];
            self.read(&mut args)?;
            match (object, opcode) {
                (1, 0) => return None,
                (id, 0) if id == callback => return Some(()),
                _ => event(object, opcode, &args)?,
            }
        }
        None
    }

    fn read(&mut self, buffer: &mut [u8]) -> Option<()> {
        self.stream.set_read_timeout(self.remaining()).ok()?;
        self.stream.read_exact(buffer).ok()
    }
}

fn u32_at(args: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(args.get(at..at + 4)?.try_into().ok()?))
}

/// A Wayland string: length with NUL, bytes, NUL, padding to 4 bytes.
fn string_at(args: &[u8], at: usize) -> Option<(&[u8], usize)> {
    let len = u32_at(args, at)? as usize;
    let bytes = args.get(at + 4..at + 4 + len)?;
    let next = at + 4 + len.div_ceil(4) * 4;
    Some((bytes.strip_suffix(b"\0").unwrap_or(bytes), next))
}

fn push_string(args: &mut Vec<u8>, value: &[u8]) {
    let len = value.len() + 1;
    args.extend((len as u32).to_ne_bytes());
    args.extend(value);
    args.resize(args.len() + len.div_ceil(4) * 4 - value.len(), 0);
}

async fn xrandr() -> Option<u32> {
    let output = tokio::process::Command::new("xrandr")
        .arg("--current")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    let text = output
        .stdout
        .get(..MAX_XRANDR_BYTES)
        .unwrap_or(&output.stdout);
    current_xrandr_rate(&String::from_utf8_lossy(text))
}

/// Highest refresh among current modes (`*` marker) in `xrandr` output.
fn current_xrandr_rate(text: &str) -> Option<u32> {
    text.split_whitespace()
        .filter(|token| token.contains('*'))
        .filter_map(|token| token.trim_end_matches(['*', '+']).parse::<f64>().ok())
        .filter(|hz| hz.is_finite() && *hz > 0.0)
        .map(|hz| hz.round() as u32)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn event(object: u32, opcode: u16, args: &[u8]) -> Vec<u8> {
        let size = (8 + args.len()) as u32;
        let mut message = object.to_ne_bytes().to_vec();
        message.extend(((size << 16) | u32::from(opcode)).to_ne_bytes());
        message.extend(args);
        message
    }

    fn global(name: u32, interface: &[u8], version: u32) -> Vec<u8> {
        let mut args = name.to_ne_bytes().to_vec();
        push_string(&mut args, interface);
        args.extend(version.to_ne_bytes());
        event(2, 0, &args)
    }

    fn mode(output: u32, flags: u32, refresh_mhz: u32) -> Vec<u8> {
        let args = [flags, 2560, 1440, refresh_mhz]
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect::<Vec<_>>();
        event(output, 1, &args)
    }

    fn read_requests(stream: &mut UnixStream, count: usize) -> Vec<(u32, u16, Vec<u8>)> {
        (0..count)
            .map(|_| {
                let mut header = [0_u8; 8];
                stream.read_exact(&mut header).unwrap();
                let object = u32::from_ne_bytes(header[..4].try_into().unwrap());
                let word = u32::from_ne_bytes(header[4..].try_into().unwrap());
                let mut args = vec![0_u8; (word >> 16) as usize - 8];
                stream.read_exact(&mut args).unwrap();
                (object, (word & 0xffff) as u16, args)
            })
            .collect()
    }

    #[test]
    fn wayland_current_mode_of_the_fastest_output_wins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wayland-test");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let first = read_requests(&mut stream, 2);
            assert_eq!((first[0].0, first[0].1), (1, 1), "get_registry");
            assert_eq!((first[1].0, first[1].1), (1, 0), "sync");
            let mut reply = global(7, b"wl_output", 4);
            reply.extend(global(8, b"wl_seat", 7));
            reply.extend(global(9, b"wl_output", 3));
            reply.extend(event(3, 0, &0_u32.to_ne_bytes()));
            stream.write_all(&reply).unwrap();
            let binds = read_requests(&mut stream, 3);
            let (_, rest) = string_at(&binds[0].2, 4).unwrap();
            assert_eq!(u32_at(&binds[0].2, rest), Some(2), "bound at version 2");
            assert_eq!(u32_at(&binds[1].2, 0), Some(9));
            let mut reply = mode(4, 0, 240_000);
            reply.extend(mode(4, 3, 59_951));
            reply.extend(mode(5, 1, 143_998));
            reply.extend(event(6, 0, &0_u32.to_ne_bytes()));
            stream.write_all(&reply).unwrap();
        });
        let hz = wayland(&path, Instant::now() + Duration::from_secs(5));
        server.join().unwrap();
        assert_eq!(hz, Some(144), "only current modes count");
    }

    #[test]
    fn wayland_without_outputs_or_with_protocol_error_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wayland-error");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_requests(&mut stream, 2);
            stream.write_all(&event(1, 0, &[0; 12])).unwrap();
        });
        assert_eq!(
            wayland(&path, Instant::now() + Duration::from_secs(5)),
            None
        );
        server.join().unwrap();
        let silent = dir.path().join("wayland-silent");
        let _listener = UnixListener::bind(&silent).unwrap();
        let started = Instant::now();
        assert_eq!(wayland(&silent, started + Duration::from_millis(50)), None);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "deadline bounds a silent peer"
        );
    }

    #[test]
    fn xrandr_takes_the_highest_current_mode() {
        let text = "Screen 0: minimum 16 x 16, current 4480 x 1440\n\
            DP-1 connected primary 2560x1440+0+0\n   2560x1440    164.96*+  143.97    59.95\n\
            HDMI-1 connected 1920x1080+2560+0\n   1920x1080     60.00*+   50.00\n";
        assert_eq!(current_xrandr_rate(text), Some(165));
        assert_eq!(current_xrandr_rate("DP-1 disconnected\n"), None);
    }

    #[test]
    fn socket_path_and_interval_bounds() {
        assert_eq!(
            wayland_socket(Some("wayland-0".into()), Some("/run/user/1".into())),
            Some(PathBuf::from("/run/user/1/wayland-0"))
        );
        assert_eq!(
            wayland_socket(Some("/tmp/w".into()), None),
            Some(PathBuf::from("/tmp/w"))
        );
        assert_eq!(wayland_socket(Some("wayland-0".into()), None), None);
        assert_eq!(interval(60), Duration::from_nanos(16_666_666));
        assert_eq!(interval(1_000), interval(MAX_HZ));
        assert_eq!(interval(1), interval(MIN_HZ));
    }
}
