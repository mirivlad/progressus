fn main() {
    if let Err(error) = progressus_client::run() {
        eprintln!("progressus-client: {error}");
        std::process::exit(1);
    }
}
