//! Skill registry — indexed storage of available skills.

use std::collections::HashMap;

use crate::borderless::agent_core::{SkillDefinition, SkillTrigger};

/// Registry of available skills, indexed by name, category, and tag.
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
    by_category: HashMap<String, Vec<String>>,
    by_tag: HashMap<String, Vec<String>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
        }
    }

    /// Register a skill.
    pub fn register(&mut self, skill: SkillDefinition) {
        let name = skill.name.clone();

        // Index by category
        for cat in &skill.categories {
            self.by_category
                .entry(cat.clone())
                .or_default()
                .push(name.clone());
        }

        // Index by tag
        for tag in &skill.tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .push(name.clone());
        }

        self.skills.insert(name, skill);
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    /// Check if a skill exists.
    pub fn has(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// List all skill names.
    pub fn list(&self) -> Vec<&str> {
        self.skills.keys().map(|k| k.as_str()).collect()
    }

    /// List skills in a category.
    pub fn list_by_category(&self, category: &str) -> Vec<&str> {
        self.by_category
            .get(category)
            .map(|names| names.iter().map(|n| n.as_str()).collect())
            .unwrap_or_default()
    }

    /// List skills with a tag.
    pub fn list_by_tag(&self, tag: &str) -> Vec<&str> {
        self.by_tag
            .get(tag)
            .map(|names| names.iter().map(|n| n.as_str()).collect())
            .unwrap_or_default()
    }

    /// Full-text search across skill names and descriptions.
    pub fn search(&self, query: &str) -> Vec<&SkillDefinition> {
        let query_lower = query.to_lowercase();
        self.skills
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Match skills whose triggers fire for the given input.
    pub fn match_triggers(&self, input: &str) -> Vec<&str> {
        self.skills
            .values()
            .filter(|s| {
                s.trigger.as_ref().map_or(false, |trigger| match trigger {
                    SkillTrigger::Substring(sub) => input.contains(sub.as_str()),
                    SkillTrigger::Regex(re) => re.is_match(input),
                })
            })
            .map(|s| s.name.as_str())
            .collect()
    }

    /// Resolve dependencies for a skill (topological sort with cycle detection).
    pub fn resolve_dependencies(&self, skill_name: &str) -> Result<Vec<String>, String> {
        let mut resolved = Vec::new();
        let mut visiting = std::collections::HashSet::new();
        let mut visited = std::collections::HashSet::new();

        self.resolve_dfs(skill_name, &mut resolved, &mut visiting, &mut visited)?;
        Ok(resolved)
    }

    fn resolve_dfs(
        &self,
        name: &str,
        resolved: &mut Vec<String>,
        visiting: &mut std::collections::HashSet<String>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        if visited.contains(name) {
            return Ok(());
        }
        if visiting.contains(name) {
            return Err(format!("Circular dependency detected involving '{}'", name));
        }

        visiting.insert(name.to_string());

        if let Some(skill) = self.skills.get(name) {
            for dep in &skill.dependencies {
                self.resolve_dfs(dep, resolved, visiting, visited)?;
            }
        }

        visiting.remove(name);
        visited.insert(name.to_string());
        resolved.push(name.to_string());

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(name: &str, deps: Vec<&str>) -> SkillDefinition {
        SkillDefinition {
            name: name.into(),
            description: format!("{} skill", name),
            body: String::new(),
            version: "1.0.0".into(),
            tags: Vec::new(),
            categories: Vec::new(),
            dependencies: deps.into_iter().map(String::from).collect(),
            trigger: None,
            examples: Vec::new(),
            on_load: None,
            on_unload: None,
        }
    }

    #[test]
    fn test_dependency_resolution() {
        let mut registry = SkillRegistry::new();
        registry.register(make_skill("a", vec!["b", "c"]));
        registry.register(make_skill("b", vec!["c"]));
        registry.register(make_skill("c", vec![]));

        let resolved = registry.resolve_dependencies("a").unwrap();
        assert_eq!(resolved, vec!["c", "b", "a"]);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut registry = SkillRegistry::new();
        registry.register(make_skill("a", vec!["b"]));
        registry.register(make_skill("b", vec!["a"]));

        assert!(registry.resolve_dependencies("a").is_err());
    }
}
