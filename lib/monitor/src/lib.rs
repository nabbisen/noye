use std::{
    thread::sleep,
    time::{Duration, SystemTime},
};

const CHECKS: [&str; 3] = ["http", "https cert", "email"];
const INTERVAL: Duration = Duration::from_secs(10);

pub fn run() -> () {
    let start = SystemTime::now();

    loop {
        for check in CHECKS {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(hello(start.clone(), check));
        }

        sleep(INTERVAL);
    }
}

async fn hello(start: SystemTime, check: &str) {
    println!(
        "[{:04} secs] Hello from monitor. {}",
        SystemTime::now().duration_since(start).unwrap().as_secs(),
        check,
    );
}
