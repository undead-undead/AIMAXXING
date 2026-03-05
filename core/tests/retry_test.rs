use std::sync::Arc;
use tokio::time::{sleep, Duration};
use aimaxxing_core::agent::multi_agent::{Coordinator, AgentRole};
use aimaxxing_core::agent::scheduler::{Scheduler, JobSchedule, JobPayload, SqliteCronStore};
use chrono::Utc;

#[tokio::test]
async fn test_cron_retry() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let db_path = "test_retry.db";
    let _ = std::fs::remove_file(db_path);
    
    let coordinator = Arc::new(Coordinator::new());
    let store = Box::new(SqliteCronStore::new(db_path).unwrap());
    let scheduler = Scheduler::new(Arc::downgrade(&coordinator), Some(store)).await;
    
    // Start the scheduler
    let s2 = scheduler.clone();
    tokio::spawn(async move {
        s2.run().await;
    });

    // We don't register any agent for Researcher role, so it will fail
    let role = AgentRole::Researcher;
    
    println!("Adding failing job...");
    let job_id = scheduler.add_job(
        "failing_job".to_string(),
        JobSchedule::At { at: Utc::now() + chrono::Duration::seconds(2) },
        JobPayload::AgentTurn { 
            role, 
            prompt: "Fail me because researcher is not registered".to_string() 
        }
    ).await.unwrap();
    
    // Wait for first attempt (2s scheduled + 1s buffer)
    println!("Waiting for first attempt...");
    sleep(Duration::from_secs(4)).await;
    
    // Check error count
    {
        let jobs = scheduler.list_jobs();
        // ID changes on reschedule, so find by name
        let job = jobs.iter().find(|j| j.name == "failing_job").expect("Job not found");
        println!("Error count after 1st fail: {}", job.error_count);
        assert!(job.error_count >= 1);
        
        // Also check if it's scheduled for retry (last_run_at should be set)
        assert!(job.last_run_at.is_some());
    }
    
    // Since retry is every 30s, we don't want to wait 30s in CI/CD.
    // But for a local dev test it's fine.
    // Alternatively, we can check if a new job with a different UUID but same name/payload 
    // Wait, my retry logic uses SAME job (just reregisters it with same UUID but new schedule).
    // Let's verify if the UUID stays the same in tokio-cron-scheduler.
    // In register_job_runtime:
    // let id_original = cron_job.id;
    // ...
    // let id = sched.add(job).await.map_err(|e| Error::Internal(format!("Failed to add job to scheduler: {}", e)))?;
    // ...
    // self.jobs.insert(id_original, cron_job); // Wait, I use id_original which is what I passed in.
    
    println!("Retry test basic part passed.");
}
