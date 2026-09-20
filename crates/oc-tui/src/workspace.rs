//! Workspace registry for one config generation (UI06).
//!
//! Primary-agent selection, skill catalog cards, redacted provenance, and
//! Location/session switch actions bound to a single generation id. Stale
//! actions fail; removed entries disappear. Agent definitions load with
//! the configured workspace (T25/A13); here the mechanics are proven with
//! caller-supplied entries plus real [`SkillMeta`] and redacted
//! [`explain_redacted`] output.

use std::collections::BTreeMap;

use oc_adapters::config::{Generation, SkillMeta, explain_redacted};
use oc_adapters::storage::Db;

/// Prefs key holding the persisted primary agent JSON.
pub const PREF_PRIMARY_AGENT: &str = "tui.primary_agent";

/// One selectable agent profile (populated by the workspace loader).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    /// Profile id.
    pub id: String,
    /// Bounded description.
    pub description: String,
    /// Validated model id.
    pub model: String,
    /// Validated variant, if any.
    pub variant: Option<String>,
}

/// Typed workspace errors (ids only, no secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// Action carried an older generation than the registry.
    StaleGeneration {
        /// Action generation.
        want: u64,
        /// Registry generation.
        got: u64,
    },
    /// Unknown agent id with the actionable available list.
    UnknownAgent {
        /// Requested id.
        id: String,
        /// Sorted available ids.
        available: String,
    },
    /// Location switch to an unloaded Location.
    UnknownLocation {
        /// Requested id.
        id: String,
    },
    /// No primary agent selected.
    NoPrimaryAgent,
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleGeneration { want, got } => {
                write!(f, "stale generation {want}; current is {got}")
            }
            Self::UnknownAgent { id, available } => {
                write!(f, "unknown agent {id}; available: {available}")
            }
            Self::UnknownLocation { id } => write!(f, "unknown location {id}"),
            Self::NoPrimaryAgent => write!(f, "no primary agent selected"),
        }
    }
}

/// Single-generation workspace registry (UI06 view-model).
pub struct WorkspaceRegistry {
    generation: u64,
    location: String,
    agents: BTreeMap<String, AgentEntry>,
    skills: Vec<SkillMeta>,
    redacted: serde_json::Value,
    primary_agent: Option<String>,
}

impl WorkspaceRegistry {
    /// Bind entries to one generation. The generation counter is assigned
    /// by the loader (monotonic per config load); full D10
    /// `ConfigGenerationId` plumbing arrives with the runtime (T24/T25).
    pub fn bind(
        generation: u64,
        location: &str,
        config: &Generation,
        agents: Vec<AgentEntry>,
        skills: Vec<SkillMeta>,
    ) -> Self {
        Self {
            generation,
            location: location.to_string(),
            agents: agents.into_iter().map(|a| (a.id.clone(), a)).collect(),
            skills,
            redacted: explain_redacted(config),
            primary_agent: None,
        }
    }

    /// Registry generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Loaded Location id.
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Redacted provenance/diagnostics view (secrets are `***` by construction).
    pub fn redacted(&self) -> &serde_json::Value {
        &self.redacted
    }

    /// Sorted agent ids.
    pub fn agent_ids(&self) -> Vec<&str> {
        self.agents.keys().map(String::as_str).collect()
    }

    /// Skill cards: id/name/description only, never bodies.
    pub fn skill_cards(&self) -> Vec<(&str, &str, &str)> {
        self.skills
            .iter()
            .map(|s| (s.id.as_str(), s.name.as_str(), s.description.as_str()))
            .collect()
    }

    /// Select the primary agent for this generation and persist it.
    pub fn select_primary(
        &mut self,
        id: &str,
        generation: u64,
        db: &Db,
    ) -> Result<(), WorkspaceError> {
        self.check_generation(generation)?;
        if !self.agents.contains_key(id) {
            return Err(self.unknown_agent(id));
        }
        self.primary_agent = Some(id.to_string());
        let raw = serde_json::json!({"id": id, "generation": generation}).to_string();
        db.set_pref(PREF_PRIMARY_AGENT, &raw)
            .map_err(|_| WorkspaceError::NoPrimaryAgent)?;
        Ok(())
    }

    /// Resolve the persisted primary against this generation: stale
    /// records and vanished ids fail visibly and block the next turn.
    pub fn load_primary(&mut self, db: &Db) -> Result<String, WorkspaceError> {
        let raw = db
            .get_pref(PREF_PRIMARY_AGENT)
            .map_err(|_| WorkspaceError::NoPrimaryAgent)?
            .ok_or(WorkspaceError::NoPrimaryAgent)?;
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| WorkspaceError::NoPrimaryAgent)?;
        let generation = value.get("generation").and_then(|v| v.as_u64());
        if generation != Some(self.generation) {
            self.primary_agent = None;
            return Err(WorkspaceError::StaleGeneration {
                want: generation.unwrap_or(0),
                got: self.generation,
            });
        }
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or(WorkspaceError::NoPrimaryAgent)?;
        if !self.agents.contains_key(id) {
            self.primary_agent = None;
            return Err(self.unknown_agent(id));
        }
        self.primary_agent = Some(id.to_string());
        Ok(id.to_string())
    }

    /// Current primary agent, if resolved.
    pub fn primary_agent(&self) -> Option<&str> {
        self.primary_agent.as_deref()
    }

    /// Require a resolved primary before a turn (no silent fallback).
    pub fn require_primary(&self) -> Result<&str, WorkspaceError> {
        self.primary_agent
            .as_deref()
            .ok_or(WorkspaceError::NoPrimaryAgent)
    }

    /// Location switch: only the loaded Location succeeds; anything else
    /// is refused without touching another registry (there is none).
    pub fn switch_location(&self, id: &str, generation: u64) -> Result<(), WorkspaceError> {
        self.check_generation(generation)?;
        if id == self.location {
            return Ok(());
        }
        Err(WorkspaceError::UnknownLocation { id: id.to_string() })
    }

    fn check_generation(&self, generation: u64) -> Result<(), WorkspaceError> {
        if generation != self.generation {
            return Err(WorkspaceError::StaleGeneration {
                want: generation,
                got: self.generation,
            });
        }
        Ok(())
    }

    fn unknown_agent(&self, id: &str) -> WorkspaceError {
        WorkspaceError::UnknownAgent {
            id: id.to_string(),
            available: self.agent_ids().join(", "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentEntry, WorkspaceError, WorkspaceRegistry};
    use oc_adapters::config::{Generation, SkillMeta};
    use oc_adapters::storage::Db;

    fn agent(id: &str) -> AgentEntry {
        AgentEntry {
            id: id.to_string(),
            description: format!("{id} agent"),
            model: "m".to_string(),
            variant: None,
        }
    }

    fn make_registry(generation: u64) -> WorkspaceRegistry {
        WorkspaceRegistry::bind(
            generation,
            "work",
            &Generation::default(),
            vec![agent("code"), agent("review")],
            vec![SkillMeta {
                id: "s1".to_string(),
                name: "S1".to_string(),
                description: "does things".to_string(),
            }],
        )
    }

    fn test_db(name: &str) -> Db {
        let root =
            std::env::temp_dir().join(format!("oc-tui-workspace-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Db::open(&root).expect("db")
    }

    #[test]
    fn primary_select_persist_resolve() {
        let db = test_db("primary");
        let mut registry = make_registry(7);
        registry.select_primary("code", 7, &db).expect("select");
        assert_eq!(registry.require_primary(), Ok("code"));

        // Same generation reload resolves.
        let mut same = make_registry(7);
        assert_eq!(same.load_primary(&db), Ok("code".to_string()));

        // New generation: stale record fails, no fallback.
        let mut next = make_registry(8);
        assert_eq!(
            next.load_primary(&db),
            Err(WorkspaceError::StaleGeneration { want: 7, got: 8 })
        );
        assert_eq!(next.require_primary(), Err(WorkspaceError::NoPrimaryAgent));
    }

    #[test]
    fn vanished_agent_blocks() {
        let db = test_db("vanished");
        let mut registry = make_registry(7);
        registry.select_primary("code", 7, &db).expect("select");
        let mut without = WorkspaceRegistry::bind(
            7,
            "work",
            &Generation::default(),
            vec![agent("review")],
            Vec::new(),
        );
        assert!(matches!(
            without.load_primary(&db),
            Err(WorkspaceError::UnknownAgent { .. })
        ));
    }

    #[test]
    fn stale_actions_fail_and_locations_are_single() {
        let db = test_db("stale");
        let mut registry = make_registry(7);
        assert_eq!(
            registry.select_primary("code", 6, &db),
            Err(WorkspaceError::StaleGeneration { want: 6, got: 7 })
        );
        assert_eq!(
            registry.select_primary("ghost", 7, &db),
            Err(WorkspaceError::UnknownAgent {
                id: "ghost".to_string(),
                available: "code, review".to_string(),
            })
        );
        assert!(registry.switch_location("work", 7).is_ok());
        assert_eq!(
            registry.switch_location("elsewhere", 7),
            Err(WorkspaceError::UnknownLocation {
                id: "elsewhere".to_string(),
            })
        );
        assert_eq!(registry.skill_cards().len(), 1);
        assert!(registry.redacted().is_object());
    }
}
