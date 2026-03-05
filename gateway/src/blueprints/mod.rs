//! Blueprint Gallery — Compile-time embedded expert persona templates.
//!
//! All templates are statically compiled into the binary via `include_str!`.
//! Zero network fetch, zero file I/O at runtime.

use serde::Serialize;

/// A blueprint persona template.
#[derive(Debug, Clone, Serialize)]
pub struct Blueprint {
    /// Machine-readable identifier (e.g. "rust_architect")
    pub id: String,
    /// Human-readable display name
    pub name: String,
    /// Category for UI grouping
    pub category: String,
    /// Short description for the gallery
    pub description: String,
    /// Full template content (SOUL.md body)
    pub template: String,
}

/// Return all built-in blueprints.
pub fn all_blueprints() -> Vec<Blueprint> {
    vec![
        // ── Industry Experts ─────────────────────────────────────────────
        Blueprint {
            id: "ceo_strategy_advisor".into(),
            name: "CEO Strategy Advisor (Bezos)".into(),
            category: "Industry Experts".into(),
            description: "Strategic decision-making with Day 1 mindset, customer obsession, and flywheel thinking.".into(),
            template: include_str!("templates/ceo_strategy_advisor.md").into(),
        },
        Blueprint {
            id: "fullstack_developer".into(),
            name: "Fullstack Developer (DHH)".into(),
            category: "Industry Experts".into(),
            description: "Pragmatic full-stack development: convention over configuration, majestic monolith.".into(),
            template: include_str!("templates/fullstack_developer.md").into(),
        },
        Blueprint {
            id: "product_designer".into(),
            name: "Product Designer (Don Norman)".into(),
            category: "Industry Experts".into(),
            description: "Human-centered design with affordance, mental models, and cognitive analysis.".into(),
            template: include_str!("templates/product_designer.md").into(),
        },
        Blueprint {
            id: "growth_operator".into(),
            name: "Growth Operator (Paul Graham)".into(),
            category: "Industry Experts".into(),
            description: "Early-stage growth: do things that don't scale, PMF-first, ramen profitability.".into(),
            template: include_str!("templates/growth_operator.md").into(),
        },

        // ── Technical Specialists ────────────────────────────────────────
        Blueprint {
            id: "rust_architect".into(),
            name: "Rust Systems Architect".into(),
            category: "Technical Specialists".into(),
            description: "Zero-cost abstractions, ownership-driven design, fearless concurrency in Rust.".into(),
            template: include_str!("templates/rust_architect.md").into(),
        },
        Blueprint {
            id: "cybersecurity_auditor".into(),
            name: "Cybersecurity Auditor (CISO)".into(),
            category: "Technical Specialists".into(),
            description: "Threat assessment, vulnerability analysis, zero-trust architecture, and incident response.".into(),
            template: include_str!("templates/cybersecurity_auditor.md").into(),
        },
        Blueprint {
            id: "quantitative_strategist".into(),
            name: "Quantitative Strategist".into(),
            category: "Technical Specialists".into(),
            description: "Algorithmic trading strategy design, risk management, and systematic portfolio construction.".into(),
            template: include_str!("templates/quantitative_strategist.md").into(),
        },

        // ── Productivity ─────────────────────────────────────────────────
        Blueprint {
            id: "research_analyst".into(),
            name: "Research Analyst".into(),
            category: "Productivity".into(),
            description: "Deep research with triangulation, source verification, and structured output.".into(),
            template: include_str!("templates/research_analyst.md").into(),
        },
        Blueprint {
            id: "daily_secretary".into(),
            name: "Daily Secretary".into(),
            category: "Productivity".into(),
            description: "Task management, schedule tracking, commitment follow-ups, and operational rhythms.".into(),
            template: include_str!("templates/daily_secretary.md").into(),
        },
        Blueprint {
            id: "knowledge_curator".into(),
            name: "Knowledge Curator".into(),
            category: "Productivity".into(),
            description: "Knowledge capture, Zettelkasten organization, associative linking, and second aimaxxing_core.".into(),
            template: include_str!("templates/knowledge_curator.md").into(),
        },
    ]
}

/// Get a specific blueprint by ID.
pub fn get_blueprint(id: &str) -> Option<Blueprint> {
    all_blueprints().into_iter().find(|b| b.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_blueprints_count() {
        assert_eq!(
            all_blueprints().len(),
            10,
            "Should have exactly 10 blueprints"
        );
    }

    #[test]
    fn test_get_blueprint_by_id() {
        assert!(get_blueprint("rust_architect").is_some());
        assert!(get_blueprint("nonexistent").is_none());
    }

    #[test]
    fn test_templates_contain_frontmatter() {
        for bp in all_blueprints() {
            assert!(
                bp.template.contains("---"),
                "Blueprint '{}' should contain YAML frontmatter",
                bp.id
            );
        }
    }
}
