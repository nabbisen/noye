use std::env::args;

use noye_storage_init::init_db;

#[tokio::main]
async fn main() {
    let arg = args().nth(1);
    if let Some(arg) = arg {
        if &arg == "init-db" {
            let _ = init_db().await;
            return;
        }
    }

    run()
}

fn run() {
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
