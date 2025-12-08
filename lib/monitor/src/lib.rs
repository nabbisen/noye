use std::{
    sync::Arc,
    thread::sleep,
    time::{Duration, SystemTime},
};

use tokio::sync::Semaphore;

mod monitor;

use monitor::{http::check_http, smtp::check_smtp};

const DOMAINS: [&str; 2] = ["localhost", "127.0.0.1"];
const CHECKS: [&str; 3] = ["http", "https", "smtp"];
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

                    // todo
                    println!(
                        "[{:04} secs] Hello from monitor. {} {}",
                        SystemTime::now().duration_since(start).unwrap().as_secs(),
                        domain,
                        check,
                    );

                    // todo
                    let ret = match check {
                        // todo port
                        "http" => check_http(domain, 8080).await,
                        "smtp" => check_smtp(domain, 587),
                        _ => Ok(0),
                    };
                    println!("{} {} = {:?}", domain, check, ret);
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
