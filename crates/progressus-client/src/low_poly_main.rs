fn main() -> std::process::ExitCode {
    match progressus_client::low_poly::run(std::env::args().skip(1)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("progressus-low-poly: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
