//! Encoded-message admission before the SDK retains stdio/HTTP/SSE payloads.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

// rmcp may report only a closed response channel after a receive failure.
// Retain that connection's numeric cause, not remote bytes or error strings.
#[derive(Clone, Default)]
pub(crate) struct Fault(Arc<Mutex<Option<(usize, usize)>>>);

impl Fault {
    pub(crate) fn check(&self, bytes: usize, limit: usize) -> io::Result<()> {
        if bytes <= limit {
            return Ok(());
        }
        self.0
            .lock()
            .expect("MCP message budget")
            .get_or_insert((bytes, limit));
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MCP message byte budget exceeded",
        ))
    }

    pub(crate) fn get(&self) -> Option<(usize, usize)> {
        *self.0.lock().expect("MCP message budget")
    }

    pub(crate) async fn wait(&self) -> (usize, usize) {
        loop {
            if let Some(facts) = self.get() {
                return facts;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

pub(crate) struct LineIo<R> {
    inner: R,
    bytes: usize,
    fault: Fault,
}

impl<R> LineIo<R> {
    pub(crate) fn new(inner: R, fault: Fault) -> Self {
        Self {
            inner,
            bytes: 0,
            fault,
        }
    }

    fn observe(&self, chunk: &[u8]) -> io::Result<usize> {
        let mut bytes = self.bytes;
        for byte in chunk {
            if *byte == b'\n' {
                bytes = 0;
                continue;
            }
            bytes += 1;
            self.fault
                .check(bytes, crate::mcp_remote::MESSAGE_BYTES_CAP)?;
        }
        Ok(bytes)
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for LineIo<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let start = buffer.filled().len();
        ready!(Pin::new(&mut this.inner).poll_read(cx, buffer))?;
        this.bytes = this.observe(&buffer.filled()[start..])?;
        Poll::Ready(Ok(()))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for LineIo<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        // Reject an oversized encoded frame before any of this write is sent.
        this.observe(bytes)?;
        let written = ready!(Pin::new(&mut this.inner).poll_write(cx, bytes))?;
        this.bytes = this.observe(&bytes[..written])?;
        Poll::Ready(Ok(written))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    #[tokio::test]
    async fn oversized_encoded_request_is_rejected_before_writing_to_the_pipe() {
        let fault = Fault::default();
        let (writer, mut reader) = tokio::io::duplex(32);
        let mut writer = LineIo::new(writer, fault.clone());
        let mut request =
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{\"padding\":\""
                .to_vec();
        request.resize(crate::mcp_remote::MESSAGE_BYTES_CAP + 1, b'x');
        request.extend_from_slice(b"\"}}\n");
        assert!(writer.write_all(&request).await.is_err());
        drop(writer);
        let mut received = Vec::new();
        assert_eq!(reader.read_to_end(&mut received).await.unwrap(), 0);
        assert_eq!(
            fault.get(),
            Some((
                crate::mcp_remote::MESSAGE_BYTES_CAP + 1,
                crate::mcp_remote::MESSAGE_BYTES_CAP
            ))
        );
    }
}
