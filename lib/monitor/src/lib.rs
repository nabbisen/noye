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
            println!(
                "[{:04} secs] Hello from monitor. {}",
                SystemTime::now().duration_since(start).unwrap().as_secs(),
                check,
            );
        }

        sleep(INTERVAL);
    }
}
