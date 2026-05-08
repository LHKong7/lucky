//! Skill lifecycle manager — tracks active skills per session.

use std::collections::{HashMap, HashSet};

use crate::borderless::agent_core::SkillContext;

use super::registry::SkillRegistry;

/// Manages the lifecycle of active skills within a session.
pub struct SkillLifecycleManager {
    /// Currently active (loaded) skill names.
    active: HashSet<String>,
    /// Scratch data per skill.
    scratch: HashMap<String, serde_json::Map<String, serde_json::Value>>,
    /// Session ID.
    session_id: Option<String>,
}

impl SkillLifecycleManager {
    pub fn new(session_id: Option<String>) -> Self {
        Self {
            active: HashSet::new(),
            scratch: HashMap::new(),
            session_id,
        }
    }

    /// Load a skill and its dependencies.
    pub async fn load(
        &mut self,
        skill_name: &str,
        registry: &SkillRegistry,
    ) -> Result<Vec<String>, String> {
        if self.active.contains(skill_name) {
            return Ok(Vec::new()); // Already loaded
        }

        // Resolve dependencies
        let deps = registry.resolve_dependencies(skill_name)?;
        let mut loaded = Vec::new();

        for dep_name in &deps {
            if self.active.contains(dep_name) {
                continue;
            }

            if let Some(skill) = registry.get(dep_name) {
                // Call onLoad hook
                if let Some(ref on_load) = skill.on_load {
                    let ctx = SkillContext {
                        session_id: self.session_id.clone(),
                        scratch: self
                            .scratch
                            .entry(dep_name.clone())
                            .or_default()
                            .clone(),
                    };
                    on_load(ctx).await;
                }
                self.active.insert(dep_name.clone());
                loaded.push(dep_name.clone());
            }
        }

        Ok(loaded)
    }

    /// Unload a skill.
    pub async fn unload(
        &mut self,
        skill_name: &str,
        registry: &SkillRegistry,
    ) -> bool {
        if !self.active.remove(skill_name) {
            return false;
        }

        if let Some(skill) = registry.get(skill_name) {
            if let Some(ref on_unload) = skill.on_unload {
                let ctx = SkillContext {
                    session_id: self.session_id.clone(),
                    scratch: self.scratch.remove(skill_name).unwrap_or_default(),
                };
                on_unload(ctx).await;
            }
        }

        true
    }

    /// Check if a skill is active.
    pub fn is_active(&self, skill_name: &str) -> bool {
        self.active.contains(skill_name)
    }

    /// Get all active skill names.
    pub fn active_skills(&self) -> Vec<&str> {
        self.active.iter().map(|s| s.as_str()).collect()
    }

    /// Get the body text of all active skills (for context injection).
    pub fn get_active_skill_bodies(&self, registry: &SkillRegistry) -> Vec<(String, String)> {
        self.active
            .iter()
            .filter_map(|name| {
                registry.get(name).map(|s| (s.name.clone(), s.body.clone()))
            })
            .collect()
    }

    /// Check triggers against user input and auto-load matching skills.
    pub async fn match_and_load(
        &mut self,
        input: &str,
        registry: &SkillRegistry,
    ) -> Vec<String> {
        let triggered = registry.match_triggers(input);
        let mut loaded = Vec::new();

        for name in triggered {
            if !self.active.contains(name) {
                if let Ok(deps) = self.load(name, registry).await {
                    loaded.extend(deps);
                }
            }
        }

        loaded
    }
}
