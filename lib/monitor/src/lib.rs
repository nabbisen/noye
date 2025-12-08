use std::{
    sync::Arc,
    thread::sleep,
    time::{Duration, SystemTime},
};

use tokio::sync::Semaphore;

mod monitor;

use monitor::http::hello;

const DOMAINS: [&str; 2] = ["dom1", "dom2"];
const CHECKS: [&str; 3] = ["http", "https cert", "email"];
const INTERVAL: Duration = Duration::from_secs(10);
const MAX_CONCURRENCY: usize = 4;

pub async fn run() -> () {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENCY));
    let mut handles = Vec::new();

    let start = SystemTime::now();

    loop {
        for domain in DOMAINS {
            for check in CHECKS {
                let semaphore = semaphore.clone();

                let handle = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    hello(start.clone(), domain, check).await;
                });

                handles.push(handle);
            }
        }

        for h in &handles {
            let _ = h;
        }

        sleep(INTERVAL);
    }
}
