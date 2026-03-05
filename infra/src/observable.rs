use dashmap::DashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use async_trait::async_trait;
use brain::agent::core::{AgentEvent, AgentEventData};


/// Trait for observing agent events
#[async_trait]
pub trait AgentObserver: Send + Sync {
    /// Handle an agent event
    async fn on_event(&self, event: &AgentEvent) -> brain::error::Result<()>;
}

/// A dispatcher that forwards events from a broadcast channel to multiple observers
#[derive(Default)]
pub struct EventDispatcher {
    observers: Vec<Box<dyn AgentObserver>>,
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_observer(&mut self, observer: Box<dyn AgentObserver>) {
        self.observers.push(observer);
    }

    pub async fn dispatch(&self, event: &AgentEvent) {
        for observer in &self.observers {
            if let Err(e) = observer.on_event(event).await {
                tracing::error!("Observer failed to handle event: {}", e);
            }
        }
    }
}

/// Type of a metric
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MetricType {
    /// Monotonically increasing counter
    Counter,
    /// Arbitrary value that can go up or down
    Gauge,
    /// Distribution of values
    Histogram,
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

impl MetricValue {
    pub fn as_f64(&self) -> f64 {
        match self {
            MetricValue::Counter(v) => *v as f64,
            MetricValue::Gauge(v) => *v,
            MetricValue::Histogram { sum, .. } => *sum,
        }
    }
}

/// A central registry for all agent metrics
#[derive(Debug, Clone, Default)]
pub struct MetricsRegistry {
    metrics: Arc<DashMap<String, RwLock<MetricValue>>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an increment to a counter
    pub fn counter_inc(&self, name: &str, val: u64) {
        self.metrics.entry(name.to_string()).or_insert_with(|| RwLock::new(MetricValue::Counter(0)));
        if let Some(entry) = self.metrics.get(name) {
            let mut guard = entry.write();
            if let MetricValue::Counter(c) = &mut *guard {
                *c += val;
            }
        }
    }

    /// Set the value of a gauge
    pub fn gauge_set(&self, name: &str, val: f64) {
        self.metrics.insert(name.to_string(), RwLock::new(MetricValue::Gauge(val)));
    }

    /// Record a value in a histogram
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

    /// Get all metrics as a snapshot
    pub fn get_snapshot(&self) -> std::collections::HashMap<String, MetricValue> {
        self.metrics
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().read().clone()))
            .collect()
    }

    /// Export metrics in Prometheus text format
    pub fn render_prometheus(&self) -> String {
        let mut output = String::new();
        for entry in self.metrics.iter() {
            let name = entry.key().replace(&[':', '-', '.'][..], "_");
            let value = entry.value().read();
            
            match &*value {
                MetricValue::Counter(v) => {
                    output.push_str(&format!("# TYPE {} counter\n", name));
                    output.push_str(&format!("{} {}\n", name, v));
                }
                MetricValue::Gauge(v) => {
                    output.push_str(&format!("# TYPE {} gauge\n", name));
                    output.push_str(&format!("{} {}\n", name, v));
                }
                MetricValue::Histogram { count, sum, min, max } => {
                    output.push_str(&format!("# TYPE {}_stats summary\n", name));
                    output.push_str(&format!("{}_count {}\n", name, count));
                    output.push_str(&format!("{}_sum {}\n", name, sum));
                    output.push_str(&format!("{}_min {}\n", name, min));
                    output.push_str(&format!("{}_max {}\n", name, max));
                }
            }
        }
        output
    }
}

/// An observer that updates the metrics registry based on agent events
pub struct MetricsObserver {
    registry: Arc<MetricsRegistry>,
    agent_name: String,
}

impl MetricsObserver {
    pub fn new(registry: Arc<MetricsRegistry>, agent_name: String) -> Self {
        Self { registry, agent_name }
    }

    fn m(&self, name: &str) -> String {
        format!("{}:{}", self.agent_name, name)
    }
}

#[async_trait]
impl AgentObserver for MetricsObserver {
    async fn on_event(&self, event: &AgentEvent) -> brain::error::Result<()> {
        match &event.data {
            AgentEventData::StepStart { .. } => {
                self.registry.counter_inc(&self.m("steps_total"), 1);
            }
            AgentEventData::Thinking { .. } => {
                self.registry.counter_inc(&self.m("thinking_starts_total"), 1);
            }
            AgentEventData::Thought { .. } => {
                self.registry.counter_inc(&self.m("thoughts_total"), 1);
            }
            AgentEventData::ToolExecutionEnd { duration_ms, success, .. } => {
                self.registry.counter_inc(&self.m("tool_calls_total"), 1);
                if !success {
                    self.registry.counter_inc(&self.m("tool_errors_total"), 1);
                }
                self.registry.histogram_observe(&self.m("tool_duration_ms"), *duration_ms as f64);
            }
            AgentEventData::Error { .. } => {
                self.registry.counter_inc(&self.m("errors_total"), 1);
            }
            AgentEventData::LatencyTTFT { duration_ms } => {
                self.registry.histogram_observe(&self.m("latency_ttft_ms"), *duration_ms as f64);
            }
            AgentEventData::TokenUsage { usage } => {
                self.registry.counter_inc(&self.m("tokens_prompt_total"), usage.prompt_tokens as u64);
                self.registry.counter_inc(&self.m("tokens_completion_total"), usage.completion_tokens as u64);
                self.registry.counter_inc(&self.m("tokens_total"), usage.total_tokens as u64);
            }
            AgentEventData::Cancelled { .. } => {
                self.registry.counter_inc(&self.m("cancelled_total"), 1);
            }
            _ => {}
        }
        Ok(())
    }
}

