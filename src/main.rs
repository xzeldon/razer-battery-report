use watchman::Watchman;

mod controller;
mod devices;
mod manager;
mod watchman;

fn main() {
    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();
    let checker = Watchman::new();
    checker.run();
}
