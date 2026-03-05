//! Phase 13: Lightweight Knowledge Graph (KG Lite).
//!
//! Provides entity-relation triple storage backed by an in-memory store
//! with optional SQLite persistence. Enables the agent to answer
//! relationship-based queries like "A is B's mentor" that require
//! logical hops beyond flat document retrieval.

use std::collections::HashMap;
use std::sync::Arc;

/// A single entity-relation triple: (subject, predicate, object)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Triple {
    pub subject: Arc<str>,
    pub predicate: Arc<str>,
    pub object: Arc<str>,
    /// Optional metadata (source, confidence, timestamp)
    pub metadata: HashMap<String, String>,
}

/// A node in the knowledge graph (an entity)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entity {
    pub id: Arc<str>,
    pub entity_type: Arc<str>,
    pub properties: HashMap<String, String>,
}

/// Result of a graph query
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphQueryResult {
    pub triples: Vec<Triple>,
    pub entities: Vec<Entity>,
}

/// Lightweight in-memory knowledge graph.
///
/// Uses a simple triple store design:
/// - Triples indexed by subject AND object for bidirectional traversal
/// - Entity registry for node metadata
/// - Supports 1-hop and 2-hop path queries
pub struct KnowledgeGraph {
    /// All triples in the graph
    triples: Vec<Triple>,
    /// Entity registry: id -> Entity
    entities: HashMap<Arc<str>, Entity>,
    /// Subject index: subject -> [triple indices]
    subject_index: HashMap<Arc<str>, Vec<usize>>,
    /// Object index: object -> [triple indices]
    object_index: HashMap<Arc<str>, Vec<usize>>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            triples: Vec::new(),
            entities: HashMap::new(),
            subject_index: HashMap::new(),
            object_index: HashMap::new(),
        }
    }

    /// Register an entity in the graph
    pub fn add_entity(&mut self, entity: Entity) {
        self.entities.insert(Arc::clone(&entity.id), entity);
    }

    /// Add a triple (relationship) to the graph
    pub fn add_triple(&mut self, triple: Triple) {
        let idx = self.triples.len();
        self.subject_index
            .entry(Arc::clone(&triple.subject))
            .or_default()
            .push(idx);
        self.object_index
            .entry(Arc::clone(&triple.object))
            .or_default()
            .push(idx);

        // Auto-register entities if not already present
        if !self.entities.contains_key(&triple.subject) {
            self.entities.insert(
                Arc::clone(&triple.subject),
                Entity {
                    id: Arc::clone(&triple.subject),
                    entity_type: "auto".into(),
                    properties: HashMap::new(),
                },
            );
        }
        if !self.entities.contains_key(&triple.object) {
            self.entities.insert(
                Arc::clone(&triple.object),
                Entity {
                    id: Arc::clone(&triple.object),
                    entity_type: "auto".into(),
                    properties: HashMap::new(),
                },
            );
        }

        self.triples.push(triple);
    }

    /// Query all triples where the given entity is the subject
    pub fn query_subject(&self, subject: &str) -> Vec<&Triple> {
        self.subject_index
            .get(subject)
            .map(|indices| indices.iter().map(|&i| &self.triples[i]).collect())
            .unwrap_or_default()
    }

    /// Query all triples where the given entity is the object
    pub fn query_object(&self, object: &str) -> Vec<&Triple> {
        self.object_index
            .get(object)
            .map(|indices| indices.iter().map(|&i| &self.triples[i]).collect())
            .unwrap_or_default()
    }

    /// Query triples by predicate (relationship type)
    pub fn query_predicate(&self, predicate: &str) -> Vec<&Triple> {
        self.triples
            .iter()
            .filter(|t| t.predicate.as_ref() == predicate)
            .collect()
    }

    /// Find specific relationship: (subject, predicate, ?)
    pub fn find_objects(&self, subject: &str, predicate: &str) -> Vec<Arc<str>> {
        self.query_subject(subject)
            .into_iter()
            .filter(|t| t.predicate.as_ref() == predicate)
            .map(|t| Arc::clone(&t.object))
            .collect()
    }

    /// Find specific relationship: (?, predicate, object)
    pub fn find_subjects(&self, predicate: &str, object: &str) -> Vec<Arc<str>> {
        self.query_object(object)
            .into_iter()
            .filter(|t| t.predicate.as_ref() == predicate)
            .map(|t| Arc::clone(&t.subject))
            .collect()
    }

    /// 2-hop path query: find all entities reachable from `start` via exactly 2 hops
    pub fn query_2hop(&self, start: &str) -> Vec<(Arc<str>, Arc<str>, Arc<str>, Arc<str>)> {
        let mut results = Vec::new();
        // First hop: start -> ? via pred1
        for t1 in self.query_subject(start) {
            // Second hop: intermediate -> ? via pred2
            for t2 in self.query_subject(&t1.object) {
                results.push((
                    Arc::clone(&t1.predicate),
                    Arc::clone(&t1.object),
                    Arc::clone(&t2.predicate),
                    Arc::clone(&t2.object),
                ));
            }
        }
        results
    }

    /// Get an entity by ID
    pub fn get_entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Get all entities
    pub fn all_entities(&self) -> Vec<&Entity> {
        self.entities.values().collect()
    }

    /// Get total triple count
    pub fn triple_count(&self) -> usize {
        self.triples.len()
    }

    /// Get total entity count
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Remove all triples involving an entity (as subject or object)
    pub fn remove_entity(&mut self, entity_id: &str) {
        self.triples
            .retain(|t| t.subject.as_ref() != entity_id && t.object.as_ref() != entity_id);
        self.entities.remove(entity_id);
        self.rebuild_indices();
    }

    /// Rebuild indices after mutation
    fn rebuild_indices(&mut self) {
        self.subject_index.clear();
        self.object_index.clear();
        for (idx, triple) in self.triples.iter().enumerate() {
            self.subject_index
                .entry(Arc::clone(&triple.subject))
                .or_default()
                .push(idx);
            self.object_index
                .entry(Arc::clone(&triple.object))
                .or_default()
                .push(idx);
        }
    }

    /// Serialize the graph to JSON
    pub fn to_json(&self) -> serde_json::Result<String> {
        let data = serde_json::json!({
            "entities": self.entities,
            "triples": self.triples,
        });
        serde_json::to_string_pretty(&data)
    }

    /// Deserialize the graph from JSON
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        #[derive(serde::Deserialize)]
        struct GraphData {
            entities: HashMap<Arc<str>, Entity>,
            triples: Vec<Triple>,
        }
        let data: GraphData = serde_json::from_str(json)?;
        let mut graph = Self::new();
        graph.entities = data.entities;
        for triple in data.triples {
            graph.add_triple(triple);
        }
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> KnowledgeGraph {
        let mut kg = KnowledgeGraph::new();
        kg.add_triple(Triple {
            subject: "Alice".into(),
            predicate: "mentor_of".into(),
            object: "Bob".into(),
            metadata: HashMap::new(),
        });
        kg.add_triple(Triple {
            subject: "Bob".into(),
            predicate: "works_at".into(),
            object: "ACME".into(),
            metadata: HashMap::new(),
        });
        kg.add_triple(Triple {
            subject: "Alice".into(),
            predicate: "works_at".into(),
            object: "ACME".into(),
            metadata: HashMap::new(),
        });
        kg.add_triple(Triple {
            subject: "Bob".into(),
            predicate: "knows".into(),
            object: "Charlie".into(),
            metadata: HashMap::new(),
        });
        kg
    }

    #[test]
    fn test_basic_queries() {
        let kg = sample_graph();
        assert_eq!(kg.triple_count(), 4);
        assert_eq!(kg.entity_count(), 4); // Alice, Bob, ACME, Charlie

        let alice_rels = kg.query_subject("Alice");
        assert_eq!(alice_rels.len(), 2);

        let acme_incoming = kg.query_object("ACME");
        assert_eq!(acme_incoming.len(), 2);
    }

    #[test]
    fn test_find_objects() {
        let kg = sample_graph();
        let mentees = kg.find_objects("Alice", "mentor_of");
        assert_eq!(mentees, vec![Arc::from("Bob")]);

        let workplaces = kg.find_objects("Bob", "works_at");
        assert_eq!(workplaces, vec![Arc::from("ACME")]);
    }

    #[test]
    fn test_find_subjects() {
        let kg = sample_graph();
        let mentors = kg.find_subjects("mentor_of", "Bob");
        assert_eq!(mentors, vec![Arc::from("Alice")]);
    }

    #[test]
    fn test_2hop_query() {
        let kg = sample_graph();
        // Alice -> mentor_of -> Bob -> works_at -> ACME
        // Alice -> mentor_of -> Bob -> knows -> Charlie
        let paths = kg.query_2hop("Alice");
        assert_eq!(paths.len(), 2);
        // Should find: mentor_of -> Bob -> works_at -> ACME
        assert!(paths.iter().any(|(p1, mid, p2, end)| {
            p1.as_ref() == "mentor_of"
                && mid.as_ref() == "Bob"
                && p2.as_ref() == "works_at"
                && end.as_ref() == "ACME"
        }));
    }

    #[test]
    fn test_serialization() {
        let kg = sample_graph();
        let json = kg.to_json().unwrap();
        let restored = KnowledgeGraph::from_json(&json).unwrap();
        assert_eq!(restored.triple_count(), 4);
        assert_eq!(restored.entity_count(), 4);
    }

    #[test]
    fn test_remove_entity() {
        let mut kg = sample_graph();
        kg.remove_entity("Bob");
        assert_eq!(kg.triple_count(), 1); // Only Alice works_at ACME remains
        assert!(kg.get_entity("Bob").is_none());
    }
}
