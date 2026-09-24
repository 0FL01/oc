//! Versioned Location-scoped tab preference, owned by the native application.
//! A restore only projects existing root ids; it never admits a Location,
//! creates a session, or starts a provider request.

use std::collections::HashSet;

use super::{Runtime, RuntimeError, StorageError, selection};
use crate::storage::{
    BoundedPref, Db, MAX_TAB_DECK_BYTES, MAX_TABS, StoredDeck, parse_stored_deck, pref_revision,
    valid_tab_id,
};
use oc_core::domain::SessionId;
use oc_core::queries::TabDeckSnapshot;
use oc_core::session::CoreError;

// The runtime binding, rather than the preference or a frontend path, is
// authoritative. Check the actual row too: an orphaned binding is not a tab.
fn root_exists(db: &Db, runtime: &Runtime<'_>, id: &str) -> Result<bool, CoreError> {
    match runtime.open_session(id) {
        Ok(()) => {}
        Err(RuntimeError::SessionNotFound | RuntimeError::LocationMismatch { .. }) => {
            return Ok(false);
        }
        Err(_) => return Err(CoreError::TabDeckStorage),
    }
    match db.session_meta(id) {
        Ok(meta) => Ok(meta.parent_id.is_none()),
        Err(StorageError::SessionNotFound) => Ok(false),
        Err(_) => Err(CoreError::TabDeckStorage),
    }
}

pub(super) fn load(db: &Db, runtime: &Runtime<'_>) -> Result<TabDeckSnapshot, CoreError> {
    let key = selection::tab_deck_key(runtime.location());
    let location = runtime.location().to_string();
    let pending = db
        .tab_adoptions(runtime.location())
        .map_err(|error| match error {
            StorageError::Io(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                CoreError::StoredTabDeck
            }
            _ => CoreError::TabDeckStorage,
        })?;
    let raw = match db
        .get_pref_bounded(&key, MAX_TAB_DECK_BYTES)
        .map_err(|_| CoreError::TabDeckStorage)?
    {
        BoundedPref::Missing => None,
        BoundedPref::TooLarge => return Err(CoreError::StoredTabDeck),
        BoundedPref::Value(raw) => Some(raw),
    };
    let stored = parse_stored_deck(raw.as_deref()).map_err(|_| CoreError::StoredTabDeck)?;
    let revision = raw.as_deref().map(pref_revision);
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();
    let mut projected = false;
    for id in stored.sessions {
        if valid_tab_id(&id) && seen.insert(id.clone()) && root_exists(db, runtime, &id)? {
            sessions.push(SessionId(id));
        } else {
            projected = true;
        }
    }
    let mut active = stored.active.as_ref().and_then(|id| {
        sessions
            .iter()
            .find(|session| session.0 == *id)
            .or_else(|| sessions.first())
            .cloned()
    });
    if active.as_ref().map(|session| session.0.as_str()) != stored.active.as_deref() {
        projected = true;
    }
    for id in &pending {
        // A pending root must still be owned by this exact Location, exist as
        // a root, and not be silently omitted by projection.
        if !root_exists(db, runtime, id)? {
            return Err(CoreError::StoredTabDeck);
        }
        if seen.insert(id.clone()) {
            sessions.push(SessionId(id.clone()));
        }
        active = Some(SessionId(id.clone()));
    }
    if sessions.len() > MAX_TABS {
        return Err(CoreError::StoredTabDeck);
    }
    // Home occupies a display slot. Never silently hide an existing real
    // tab: a later save of that projected deck would erase the hidden ID.
    if active.is_none() && sessions.len() == MAX_TABS {
        return Err(CoreError::StoredTabDeck);
    }
    let mut snapshot = TabDeckSnapshot {
        location,
        revision,
        sessions,
        active,
    };
    if projected {
        snapshot.mark_projected();
    }
    Ok(snapshot)
}

pub(super) fn save(
    db: &Db,
    runtime: &Runtime<'_>,
    deck: &TabDeckSnapshot,
) -> Result<TabDeckSnapshot, CoreError> {
    if deck.location != runtime.location() {
        return Err(CoreError::TabDeckConflict);
    }
    if deck.projected() {
        return Err(CoreError::TabDeckConflict);
    }
    if deck.sessions.len() > MAX_TABS || (deck.active.is_none() && deck.sessions.len() == MAX_TABS)
    {
        return Err(CoreError::InvalidTabDeck);
    }
    let mut seen = HashSet::new();
    for id in &deck.sessions {
        if !valid_tab_id(&id.0) || !seen.insert(&id.0) {
            return Err(CoreError::InvalidTabDeck);
        }
        if !root_exists(db, runtime, &id.0)? {
            return Err(CoreError::InvalidTabDeck);
        }
    }
    if let Some(active) = &deck.active
        && (!valid_tab_id(&active.0) || !seen.contains(&active.0))
    {
        return Err(CoreError::InvalidTabDeck);
    }
    let stored = StoredDeck {
        version: 1,
        sessions: deck.sessions.iter().map(|id| id.0.clone()).collect(),
        active: deck.active.as_ref().map(|id| id.0.clone()),
    };
    let raw = serde_json::to_string(&stored).map_err(|_| CoreError::InvalidTabDeck)?;
    if raw.len() > MAX_TAB_DECK_BYTES {
        return Err(CoreError::InvalidTabDeck);
    }
    // A caller may rebuild a snapshot and forget the projection marker. Check
    // the current preference too: no visible-only deck may replace hidden IDs.
    if load(db, runtime)
        .map_err(|error| match error {
            CoreError::StoredTabDeck => CoreError::TabDeckConflict,
            other => other,
        })?
        .projected()
    {
        return Err(CoreError::TabDeckConflict);
    }
    let key = selection::tab_deck_key(runtime.location());
    let updated = db
        .compare_set_tab_deck(
            &key,
            deck.revision.as_deref(),
            &raw,
            runtime.location(),
            &stored.sessions,
        )
        .map_err(|error| match error {
            StorageError::Io(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                CoreError::TabDeckConflict
            }
            _ => CoreError::TabDeckStorage,
        })?;
    if !updated {
        return Err(CoreError::TabDeckConflict);
    }
    Ok(TabDeckSnapshot {
        revision: Some(pref_revision(&raw)),
        ..deck.clone()
    })
}
