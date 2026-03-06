use brain::agent::scheduler::{Scheduler, JobSchedule, JobPayload, RedbCronStore};
use brain::agent::multi_agent::{Coordinator, AgentRole};
use std::sync::{Arc, Weak};
use uuid::Uuid;
use tempfile::NamedTempFile;

#[tokio::test(flavor = "multi_thread")]
async fn test_cron_persistence() {
    let db_file = NamedTempFile::new().unwrap();
    let db_path = db_file.path().to_str().unwrap().to_string();
    
    let coordinator = Arc::new(Coordinator::new());
    let store = Box::new(RedbCronStore::new(&db_path).unwrap());
    
    // 1. Create scheduler and add a job
    let scheduler = Scheduler::new(Arc::downgrade(&coordinator), Some(store)).await;
    scheduler.load_jobs().await.unwrap();
    
    let job_id = scheduler.add_job(
        "test_job".to_string(),
        JobSchedule::Every { interval_secs: 60 },
        JobPayload::AgentTurn { 
            role: AgentRole::Assistant, 
            prompt: "ping".to_string() 
        }
    ).await.unwrap();
    
    // 2. verify job in memory
    let jobs = scheduler.list_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].name, "test_job");
    
    // 3. Drop scheduler (simulated restart)
    drop(scheduler);
    
    // 4. Create new scheduler with same store
    let store2 = Box::new(RedbCronStore::new(&db_path).unwrap());
    let scheduler2 = Scheduler::new(Arc::downgrade(&coordinator), Some(store2)).await;
    scheduler2.load_jobs().await.unwrap();
    
    // 5. Verify job was reloaded
    let jobs2 = scheduler2.list_jobs();
    assert_eq!(jobs2.len(), 1, "Job should be reloaded from persistence");
    assert_eq!(jobs2[0].name, "test_job");
    
    // 6. Remove job
    scheduler2.remove_job(jobs2[0].id).await.unwrap();
    assert_eq!(scheduler2.list_jobs().len(), 0);
    
    // 7. Restart again to verify it's gone from DB
    let store3 = Box::new(RedbCronStore::new(&db_path).unwrap());
    let scheduler3 = Scheduler::new(Arc::downgrade(&coordinator), Some(store3)).await;
    scheduler3.load_jobs().await.unwrap();
    assert_eq!(scheduler3.list_jobs().len(), 0, "Job should be gone from persistence");
}
