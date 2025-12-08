use std::time::SystemTime;

pub async fn hello(start: SystemTime, domain: &str, check: &str) {
    println!(
        "[{:04} secs] Hello from monitor. {} {}",
        SystemTime::now().duration_since(start).unwrap().as_secs(),
        domain,
        check,
    );
}
