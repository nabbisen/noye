mod monitor;

use monitor::{http::check_http, smtp::check_smtp};

use noye_storage::core::{database::connect, monitor_config::select_all};
use tokio::sync::Semaphore;

use std::{
    sync::Arc,
    thread::sleep,
    time::{Duration, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(10);
const MAX_CONCURRENCY: usize = 4;

pub async fn run() -> () {
    // let config = Config::default();

    let pool = match connect().await {
        Ok(x) => x,
        Err(err) => panic!("{:?}", err),
    };

    let monitor_configs = match select_all(&pool).await {
        Ok(x) => x,
        Err(err) => panic!("{:?}", err),
    };

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENCY));
    let mut handles = Vec::new();

    let start = SystemTime::now();

    loop {
        let _ = monitor_configs.iter().for_each(|monitor_config| {
            let semaphore = semaphore.clone();

            let host = monitor_config.host.to_owned();
            let port = monitor_config
                .port
                .try_into()
                .expect("port cannot be converted to usize");
            let protocol = monitor_config.protocol.to_owned();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                // todo
                println!(
                    "[{:04} secs] Hello from monitor. {} {}",
                    SystemTime::now().duration_since(start).unwrap().as_secs(),
                    host,
                    protocol,
                );

                // todo
                let ret = match protocol.as_str() {
                    // todo port
                    "http" => check_http(host.as_str(), port).await,
                    "smtp" => check_smtp(host.as_str(), port),
                    _ => Ok(0),
                };
                println!("{} {} = {:?}", host, protocol, ret);
            });

            handles.push(handle);
        });

        for h in &handles {
            let _ = h;
        }

        sleep(INTERVAL);
    }
}
