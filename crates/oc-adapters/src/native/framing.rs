//! Bounded length-prefixed fragments. A frame limit is not an application reply
//! limit: the existing 50 x 1 MiB history may need six JSON bytes per text byte.
use serde::{Serialize, de::DeserializeOwned};
use std::{io, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{OwnedSemaphorePermit, Semaphore},
};

const FRAME: usize = 64 * 1024;
pub(super) const LOGICAL: usize = 6 * 50 * 1024 * 1024 + 1024 * 1024;
// One maximum legal reply plus room for normal concurrent requests. Admission
// is by bytes, before allocating or reading a declared message body.
const AGGREGATE: usize = LOGICAL + 64 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct Budget(Arc<Semaphore>);
impl Default for Budget {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(AGGREGATE)))
    }
}
impl Budget {
    fn reserve(&self, bytes: usize) -> io::Result<OwnedSemaphorePermit> {
        if bytes == 0 || bytes > LOGICAL {
            return Err(invalid());
        }
        self.0
            .clone()
            .try_acquire_many_owned(bytes as u32)
            .map_err(|_| invalid())
    }
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "native message budget or framing refused",
    )
}

/// One encoded logical message. Clones share the bytes and their single
/// budget reservation, so a fanout to several peers serializes once.
#[derive(Clone)]
pub(super) struct Message(Arc<Encoded>);
struct Encoded {
    bytes: Vec<u8>,
    _permit: OwnedSemaphorePermit,
}
impl Message {
    fn new(bytes: Vec<u8>, permit: OwnedSemaphorePermit) -> Self {
        Self(Arc::new(Encoded {
            bytes,
            _permit: permit,
        }))
    }
    pub(super) fn decode<T: DeserializeOwned>(&self) -> io::Result<T> {
        serde_json::from_slice(&self.0.bytes).map_err(|_| invalid())
    }
    pub(super) fn encode(value: &impl Serialize, budget: &Budget) -> io::Result<Self> {
        // Count without allocating first, then hold the exact serialization
        // budget through queueing and the final write.
        struct Count(usize);
        impl io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|&size| size <= LOGICAL)
                    .ok_or_else(invalid)?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, value).map_err(|_| invalid())?;
        let permit = budget.reserve(count.0)?;
        let mut bytes = Vec::with_capacity(count.0);
        serde_json::to_writer(&mut bytes, value).map_err(|_| invalid())?;
        Ok(Self::new(bytes, permit))
    }
}

pub(super) async fn read(
    reader: &mut (impl AsyncRead + Unpin),
    budget: &Budget,
) -> io::Result<Message> {
    read_limited(reader, budget, LOGICAL).await
}

pub(super) async fn read_limited(
    reader: &mut (impl AsyncRead + Unpin),
    budget: &Budget,
    limit: usize,
) -> io::Result<Message> {
    let total = reader.read_u32().await? as usize;
    if total > limit {
        return Err(invalid());
    }
    let permit = budget.reserve(total)?;
    let mut bytes = vec![0; total];
    let mut offset = 0;
    while offset < total {
        let length = reader.read_u32().await? as usize;
        if length == 0 || length > FRAME || length > total - offset {
            return Err(invalid());
        }
        reader
            .read_exact(&mut bytes[offset..offset + length])
            .await?;
        offset += length;
    }
    Ok(Message::new(bytes, permit))
}

pub(super) async fn write(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &Message,
) -> io::Result<()> {
    write_unflushed(writer, message).await?;
    writer.flush().await
}

/// Frame one message without flushing; a buffered writer batches several.
pub(super) async fn write_unflushed(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &Message,
) -> io::Result<()> {
    let bytes = &message.0.bytes;
    writer.write_u32(bytes.len() as u32).await?;
    for frame in bytes.chunks(FRAME) {
        writer.write_u32(frame.len() as u32).await?;
        writer.write_all(frame).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
