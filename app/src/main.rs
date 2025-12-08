#[tokio::main]
async fn main() {
    let _ = noye_common::config();

    tokio::spawn(async {
        let _ = noye_monitor::run().await;
    });

    std::thread::spawn(|| {
        let _ = noye_web::run();
    });

    loop {
        std::thread::park();
    }
}
