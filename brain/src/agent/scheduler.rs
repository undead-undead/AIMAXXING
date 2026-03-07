use std::sync::Arc;
use std::sync::Weak;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use dashmap::DashMap;
use tracing::{info, error, debug};
use tokio_cron_scheduler::JobScheduler;

#[cfg(feature = "cron")]
use redb::{Database, ReadableTable, TableDefinition};

use futures::future::BoxFuture;
use crate::error::{Error, Result};
use crate::agent::multi_agent::{Coordinator, AgentRole};

const CRON_JOBS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cron_jobs");

/// Trait for persistent storage of cron jobs
#[async_trait::async_trait]
pub trait CronStore: Send + Sync {
    /// Save a job
    async fn save_job(&self, job: &CronJob) -> Result<()>;
    /// Remove a job
    async fn remove_job(&self, id: Uuid) -> Result<()>;
    /// Load all jobs
    async fn load_all_jobs(&self) -> Result<Vec<CronJob>>;
}

/// Redb implementation of CronStore
pub struct RedbCronStore {
    db: Arc<Database>,
}

impl RedbCronStore {
    /// Create a new Redb cron store at the given path
    pub fn new(path: &str) -> Result<Self> {
        let db = Database::create(path)
            .map_err(|e| Error::Internal(format!("Failed to create Redb: {}", e)))?;
        
        let write_txn = db.begin_write()
            .map_err(|e| Error::Internal(format!("Failed to begin write txn: {}", e)))?;
        {
            let _ = write_txn.open_table(CRON_JOBS_TABLE)
                .map_err(|e| Error::Internal(format!("Failed to open cron table: {}", e)))?;
        }
        write_txn.commit()
            .map_err(|e| Error::Internal(format!("Failed to commit init txn: {}", e)))?;

        Ok(Self {
            db: Arc::new(db),
        })
    }
}

#[async_trait::async_trait]
impl CronStore for RedbCronStore {
    async fn save_job(&self, job: &CronJob) -> Result<()> {
        let id_str = job.id.to_string();
        let data = serde_json::to_vec(job)
            .map_err(|e| Error::Internal(format!("Failed to serialize job: {}", e)))?;

        let write_txn = self.db.begin_write()
            .map_err(|e| Error::Internal(format!("Failed to begin write txn: {}", e)))?;
        {
            let mut table = write_txn.open_table(CRON_JOBS_TABLE)
                .map_err(|e| Error::Internal(format!("Failed to open cron table: {}", e)))?;
            table.insert(id_str.as_str(), data.as_slice())
                .map_err(|e| Error::Internal(format!("Failed to insert job: {}", e)))?;
        }
        write_txn.commit()
            .map_err(|e| Error::Internal(format!("Failed to commit save txn: {}", e)))?;
        
        Ok(())
    }

    async fn remove_job(&self, id: Uuid) -> Result<()> {
        let id_str = id.to_string();
        let write_txn = self.db.begin_write()
            .map_err(|e| Error::Internal(format!("Failed to begin write txn: {}", e)))?;
        {
            let mut table = write_txn.open_table(CRON_JOBS_TABLE)
                .map_err(|e| Error::Internal(format!("Failed to open cron table: {}", e)))?;
            table.remove(id_str.as_str())
                .map_err(|e| Error::Internal(format!("Failed to remove job: {}", e)))?;
        }
        write_txn.commit()
            .map_err(|e| Error::Internal(format!("Failed to commit remove txn: {}", e)))?;
        Ok(())
    }

    async fn load_all_jobs(&self) -> Result<Vec<CronJob>> {
        let read_txn = self.db.begin_read()
            .map_err(|e| Error::Internal(format!("Failed to begin read txn: {}", e)))?;
        let table = read_txn.open_table(CRON_JOBS_TABLE)
            .map_err(|e| Error::Internal(format!("Failed to open cron table: {}", e)))?;
        
        let mut jobs = Vec::new();
        for entry in table.iter().map_err(|e| Error::Internal(format!("Failed to iter table: {}", e)))? {
            let (_, value) = entry.map_err(|e| Error::Internal(format!("Failed to get entry: {}", e)))?;
            let job: CronJob = serde_json::from_slice(value.value())
                .map_err(|e| Error::Internal(format!("Failed to deserialize job: {}", e)))?;
            jobs.push(job);
        }
        Ok(jobs)
    }
}

/// Schedule for a job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobSchedule {
    /// One-shot at absolute time
    #[serde(rename_all = "camelCase")]
    At { 
        #[serde(with = "chrono::serde::ts_seconds")]
        at: DateTime<Utc> 
    },
    /// Recurring interval
    #[serde(rename_all = "camelCase")]
    Every { 
        interval_secs: u64 
    },
    /// Cron expression
    #[serde(rename_all = "camelCase")]
    Cron { 
        expr: String 
    },
}

/// Payload for a job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobPayload {
    /// Run an agent process with a prompt
    #[serde(rename_all = "camelCase")]
    AgentTurn {
        role: AgentRole,
        prompt: String,
    },
    /// Generate a summary for a document and store it
    #[serde(rename_all = "camelCase")]
    SummarizeDoc {
        collection: String,
        path: String,
        content: String,
    },
}

/// A scheduled job (Metadata for listing/canceling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// Unique ID (Matches the one in tokio-cron-scheduler)
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Schedule definition
    pub schedule: JobSchedule,
    /// Payload to execute
    pub payload: JobPayload,
    /// Whether the job is enabled
    pub enabled: bool,
    /// When the job was last executed
    pub last_run_at: Option<DateTime<Utc>>,
    /// Number of consecutive errors
    pub error_count: u32,
    /// Maximum number of retries for one-shot jobs
    pub max_retries: u32,
}

/// Scheduler service wrapping tokio-cron-scheduler
pub struct Scheduler {
    /// Registered jobs metadata
    jobs: DashMap<Uuid, CronJob>,
    /// The underlying scheduler
    scheduler: tokio::sync::Mutex<JobScheduler>,
    /// Weak reference to coordinator for execution
    coordinator: Weak<Coordinator>,
    /// Optional persistent store
    store: Option<Box<dyn CronStore>>,
}

impl Scheduler {
    /// Create a new scheduler with persistence
    pub async fn new(coordinator: Weak<Coordinator>, store: Option<Box<dyn CronStore>>) -> Arc<Self> {
        let scheduler = JobScheduler::new().await.expect("Failed to initialize JobScheduler");
        Arc::new(Self {
            jobs: DashMap::new(),
            scheduler: tokio::sync::Mutex::new(scheduler),
            coordinator,
            store,
        })
    }

    /// Load jobs from the persistent store
    pub async fn load_jobs(self: &Arc<Self>) -> Result<()> {
        if let Some(store) = &self.store {
            let jobs = store.load_all_jobs().await?;
            info!("Loading {} jobs from cron store", jobs.len());
            for cron_job in jobs {
                // Re-register each job in the runtime scheduler
                self.clone().register_job_runtime(cron_job).await?;
            }
        }
        Ok(())
    }

    /// Internal helper to register a job in the tokio-cron-scheduler runtime
    fn register_job_runtime(self: Arc<Self>, mut cron_job: CronJob) -> BoxFuture<'static, Result<Uuid>> {
        Box::pin(async move {
            let coordinator_weak = self.coordinator.clone();
            let scheduler_weak = Arc::downgrade(&self);
            let payload_clone = cron_job.payload.clone();
            let name_clone = cron_job.name.clone();
            let id_original = cron_job.id;
            
            // 1. Create the job based on schedule type
            let job = match &cron_job.schedule {
                JobSchedule::At { at } => {
                    let now = Utc::now();
                    if *at <= now {
                        tracing::warn!("Skipping one-shot job {} scheduled in the past: {}", name_clone, at);
                        return Ok(id_original);
                    }
                    let duration = at.signed_duration_since(now).to_std().unwrap_or_default();
                    
                    tokio_cron_scheduler::Job::new_one_shot_async(duration, move |uuid, _l| {
                        let coordinator_weak = coordinator_weak.clone();
                        let scheduler_weak = scheduler_weak.clone();
                        let payload = payload_clone.clone();
                        let name = name_clone.clone();
                        Box::pin(async move {
                            let success = Self::execute_payload(&coordinator_weak, &name, payload).await.is_ok();
                            if let Some(s) = scheduler_weak.upgrade() {
                                let _ = s.update_job_status(uuid, success).await;
                            }
                        })
                    }).map_err(|e| Error::Internal(format!("Failed to create one-shot job: {}", e)))?
                }
                JobSchedule::Every { interval_secs } => {
                    let duration = std::time::Duration::from_secs(*interval_secs);
                    tokio_cron_scheduler::Job::new_repeated_async(duration, move |uuid, _l| {
                        let coordinator_weak = coordinator_weak.clone();
                        let scheduler_weak = scheduler_weak.clone();
                        let payload = payload_clone.clone();
                        let name = name_clone.clone();
                        Box::pin(async move {
                            let success = Self::execute_payload(&coordinator_weak, &name, payload).await.is_ok();
                            if let Some(s) = scheduler_weak.upgrade() {
                                let _ = s.update_job_status(uuid, success).await;
                            }
                        })
                    }).map_err(|e| Error::Internal(format!("Failed to create repeated job: {}", e)))?
                }
                JobSchedule::Cron { expr } => {
                    tokio_cron_scheduler::Job::new_async(expr.as_str(), move |uuid, _l| {
                        let coordinator_weak = coordinator_weak.clone();
                        let scheduler_weak = scheduler_weak.clone();
                        let payload = payload_clone.clone();
                        let name = name_clone.clone();
                        Box::pin(async move {
                            let success = Self::execute_payload(&coordinator_weak, &name, payload).await.is_ok();
                            if let Some(s) = scheduler_weak.upgrade() {
                                let _ = s.update_job_status(uuid, success).await;
                            }
                        })
                    }).map_err(|e| Error::Internal(format!("Failed to create cron job: {}", e)))?
                }
            };

            // 2. Add to underlying scheduler
            let sched = self.scheduler.lock().await;
            let registered_id = sched.add(job).await
                .map_err(|e| Error::Internal(format!("Failed to add job to scheduler: {}", e)))?;
            
            // Update the job metadata
            cron_job.id = registered_id;
            
            // 3. Store metadata
            self.jobs.insert(registered_id, cron_job.clone());

            // 4. Update store if ID changed
            if registered_id != id_original {
                self.jobs.remove(&id_original);
                if let Some(store) = &self.store {
                    let _ = store.remove_job(id_original).await;
                }
            }
            
            if let Some(store) = &self.store {
                store.save_job(&cron_job).await?;
            }
            
            Ok(registered_id)
        })
    }

    /// Add a job
    pub async fn add_job(self: &Arc<Self>, name: String, schedule: JobSchedule, payload: JobPayload) -> Result<Uuid> {
        self.add_job_with_retries(name, schedule, payload, 3).await
    }

    /// Add a job with custom max retries
    pub async fn add_job_with_retries(self: &Arc<Self>, name: String, schedule: JobSchedule, payload: JobPayload, max_retries: u32) -> Result<Uuid> {
        let cron_job = CronJob {
            id: Uuid::new_v4(),
            name,
            schedule,
            payload,
            enabled: true,
            last_run_at: None,
            error_count: 0,
            max_retries,
        };

        self.clone().register_job_runtime(cron_job).await
    }

    /// Update job status after execution
    pub async fn update_job_status(self: &Arc<Self>, id: Uuid, success: bool) -> Result<()> {
        let mut retry_needed = false;
        let mut job_to_retry = None;

        if let Some(mut job) = self.jobs.get_mut(&id) {
            job.last_run_at = Some(Utc::now());
            if success {
                job.error_count = 0;
            } else {
                job.error_count += 1;
                if job.error_count > job.max_retries {
                    tracing::warn!("Disabling job {} after {} consecutive failures", job.name, job.error_count);
                    job.enabled = false;
                    // Remove from runtime scheduler
                    let sched = self.scheduler.lock().await;
                    let _ = sched.remove(&id).await;
                } else if let JobSchedule::At { .. } = job.schedule {
                    retry_needed = true;
                    job_to_retry = Some(job.clone());
                }
            }
            
            let job_clone = job.clone();
            if let Some(store) = &self.store {
                store.save_job(&job_clone).await?;
            }
        }

        if retry_needed {
            if let Some(mut job) = job_to_retry {
                info!("Retrying one-shot job {} (attempt {})", job.name, job.error_count);
                // Reschedule for 30 seconds later
                job.schedule = JobSchedule::At { at: Utc::now() + chrono::Duration::seconds(30) };
                let _ = self.clone().register_job_runtime(job).await;
            }
        }
        Ok(())
    }

    /// List all jobs
    pub fn list_jobs(&self) -> Vec<CronJob> {
        self.jobs.iter().map(|r| r.value().clone()).collect()
    }

    /// Remove a job
    pub async fn remove_job(&self, id: Uuid) -> Result<bool> {
        let sched = self.scheduler.lock().await;
        let _ = sched.remove(&id).await;
        
        if let Some(store) = &self.store {
            let _ = store.remove_job(id).await;
        }

        Ok(self.jobs.remove(&id).is_some())
    }

    /// Start the scheduler loop
    pub async fn run(&self) {
        let sched = self.scheduler.lock().await;
        if let Err(e) = sched.start().await {
            error!("Failed to start scheduler: {}", e);
        }
    }

    async fn execute_payload(coordinator_weak: &Weak<Coordinator>, name: &str, payload: JobPayload) -> Result<()> {
        info!("Executing scheduled job: {}", name);
        
        let coordinator = coordinator_weak.upgrade()
            .ok_or_else(|| Error::AgentCoordination("Coordinator dropped".to_string()))?;
            
        match payload {
            JobPayload::AgentTurn { role, prompt } => {
                if let Some(agent) = coordinator.get(&role) {
                    debug!("Triggering proactive process for agent {:?}", role);
                    agent.process(&prompt).await?;
                } else {
                    return Err(Error::AgentCoordination(format!("Target agent {:?} not found", role)));
                }
            }
            JobPayload::SummarizeDoc { collection, path, content } => {
                // Use Assistant or Researcher to summarize
                let agent = coordinator.get(&AgentRole::Assistant)
                    .or_else(|| coordinator.get(&AgentRole::Researcher))
                    .ok_or_else(|| Error::AgentCoordination("No agent available for summarization".to_string()))?;
                
                let prompt = format!(
                    "Summarize the following document in about 200 words. Focus on core concepts and key information.\n\nDocument Content:\n{}", 
                    content
                );
                
                debug!("Generating summary for {}/{}", collection, path);
                let summary = agent.process(&prompt).await?;
                
                // We need to update the summary in memory. 
                // Since EngramMemory is usually the LTM, we'll try to update it through an agent if possible,
                // or we might need a more direct way if the Coordinator doesn't expose memory.
                // For now, we'll look for a way to get the memory from the agent.
                // NOTE: This assumes the agent's process call doesn't already do this.
                // In our " Tiered RAG" design, the worker does the update.
                
                // Since we don't have a clean way to get Memory from the MultiAgent trait right now,
                // we'll use a placeholder/TODO or fix the trait.
                // Actually, let's assume the coordinator might have a way or we add it.
                
                info!("Summary generated for {}/{} ({} chars)", collection, path, summary.len());
                
                if let Some(memory) = coordinator.memory.get() {
                    memory.update_summary(&collection, &path, &summary).await?;
                    info!("Successfully updated summary in memory for {}/{}", collection, path);
                } else {
                    tracing::warn!("Generated summary but no shared memory found in coordinator");
                }
            }
        }
        
        Ok(())
    }
}
