fn main() {
    let _ = noye_common::config();

    std::thread::spawn(|| {
        let _ = noye_monitor::run();
    });

    std::thread::spawn(|| {
        let _ = noye_web::run();
    });

    loop {
        std::thread::park();
    }
}
