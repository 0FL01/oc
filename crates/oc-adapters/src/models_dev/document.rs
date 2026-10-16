//! Frame complete top-level members across HTTP chunks. Serde remains the JSON
//! validator (including foreign numbers/depth/UTF-8); only delimiters are scanned.
//! One member's wire bytes coexist with the selected provider slices, not the
//! entire public catalog. No partially parsed document can be published.

use super::PublicDocument;
use crate::discovery::{DISCOVERY_BODY_CAP, DiscoveryError, append_discovery_body};
use std::collections::BTreeMap;

enum Phase {
    Start,
    First,
    Next,
    Member,
    End,
}

pub(super) struct Chunks {
    phase: Phase,
    bytes: usize,
    depth: usize,
    quoted: bool,
    escaped: bool,
    member: Vec<u8>,
    document: PublicDocument,
}

impl Chunks {
    pub(super) fn new() -> Self {
        Self {
            phase: Phase::Start,
            bytes: 0,
            depth: 1,
            quoted: false,
            escaped: false,
            member: Vec::new(),
            document: PublicDocument(BTreeMap::new()),
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<(), DiscoveryError> {
        self.bytes = self
            .bytes
            .checked_add(chunk.len())
            .filter(|bytes| *bytes <= DISCOVERY_BODY_CAP)
            .ok_or(DiscoveryError::InvalidResponse)?;
        for &byte in chunk {
            match self.phase {
                Phase::Start => match byte {
                    b' ' | b'\n' | b'\r' | b'\t' => {}
                    b'{' => self.phase = Phase::First,
                    _ => return Err(DiscoveryError::InvalidResponse),
                },
                Phase::First | Phase::Next => match byte {
                    b' ' | b'\n' | b'\r' | b'\t' => {}
                    b'}' if matches!(self.phase, Phase::First) => self.phase = Phase::End,
                    b'"' => {
                        self.member.clear();
                        append_discovery_body(&mut self.member, b"{\"")?;
                        self.quoted = true;
                        self.escaped = false;
                        self.depth = 1;
                        self.phase = Phase::Member;
                    }
                    _ => return Err(DiscoveryError::InvalidResponse),
                },
                Phase::Member => {
                    if self.quoted {
                        if self.escaped {
                            self.escaped = false;
                        } else if byte == b'\\' {
                            self.escaped = true;
                        } else if byte == b'"' {
                            self.quoted = false;
                        }
                    } else {
                        match byte {
                            b'"' => self.quoted = true,
                            b'{' | b'[' => self.depth += 1,
                            b',' | b'}' if self.depth == 1 => {
                                self.commit()?;
                                self.phase = if byte == b',' {
                                    Phase::Next
                                } else {
                                    Phase::End
                                };
                                continue;
                            }
                            b'}' | b']' => {
                                self.depth = self
                                    .depth
                                    .checked_sub(1)
                                    .ok_or(DiscoveryError::InvalidResponse)?;
                                if self.depth == 0 {
                                    return Err(DiscoveryError::InvalidResponse);
                                }
                            }
                            _ => {}
                        }
                    }
                    append_discovery_body(&mut self.member, &[byte])?;
                }
                Phase::End => match byte {
                    b' ' | b'\n' | b'\r' | b'\t' => {}
                    _ => return Err(DiscoveryError::InvalidResponse),
                },
            }
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DiscoveryError> {
        append_discovery_body(&mut self.member, b"}")?;
        let member: PublicDocument =
            serde_json::from_slice(&self.member).map_err(|_| DiscoveryError::InvalidResponse)?;
        self.document.0.extend(member.0);
        self.member.clear();
        Ok(())
    }

    pub(super) fn finish(self) -> Result<PublicDocument, DiscoveryError> {
        if !matches!(self.phase, Phase::End) {
            return Err(DiscoveryError::InvalidResponse);
        }
        Ok(self.document)
    }
}

#[cfg(test)]
mod tests;
