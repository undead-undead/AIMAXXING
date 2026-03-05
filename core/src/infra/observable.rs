use dashmap::DashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use async_trait::async_trait;
use crate::agent::core::{AgentEvent, AgentEventData};

/// Trait for observing agent events
#[async_trait]
pub trait AgentObserver: Send + Sync {
    /// Handle an agent event
    async fn on_event(&self, event: &AgentEvent) -> crate::error::Result<()>;
}

/// A central registry for all agent metrics
#[derive(Debug, Clone, Default)]
pub struct MetricsRegistry {
    metrics: Arc<DashMap<String, RwLock<MetricValue>>>,
}

/// Current value of a metric
#[derive(Debug, Clone, serde::Serialize)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram {
        count: u64,
        sum: f64,
        min: f64,
        max: f64,
    },
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn counter_inc(&self, name: &str, val: u64) {
        self.metrics.entry(name.to_string()).or_insert_with(|| RwLock::new(MetricValue::Counter(0)));
        if let Some(entry) = self.metrics.get(name) {
            let mut guard = entry.write();
            if let MetricValue::Counter(c) = &mut *guard {
                *c += val;
            }
        }
    }

    pub fn gauge_set(&self, name: &str, val: f64) {
        self.metrics.insert(name.to_string(), RwLock::new(MetricValue::Gauge(val)));
    }

    pub fn histogram_observe(&self, name: &str, val: f64) {
        self.metrics.entry(name.to_string()).or_insert_with(|| RwLock::new(MetricValue::Histogram {
            count: 0,
            sum: 0.0,
            min: f64::MAX,
            max: f64::MIN,
        }));

        if let Some(entry) = self.metrics.get(name) {
            let mut guard = entry.write();
            if let MetricValue::Histogram { count, sum, min, max } = &mut *guard {
                *count += 1;
                *sum += val;
                if val < *min { *min = val; }
                if val > *max { *max = val; }
            }
        }
    }

    pub fn get_snapshot(&self) -> std::collections::HashMap<String, MetricValue> {
        let mut snapshot = std::collections::HashMap::new();
        for entry in self.metrics.iter() {
            let (name, val) = entry.pair();
            snapshot.insert(name.clone(), val.read().clone());
        }
        snapshot
    }
}
