use std::process::ExitCode;

const USAGE: &str = "usage: progressus-client [--seed <u64>]";

fn parse_seed(arguments: impl IntoIterator<Item = String>) -> Result<u64, String> {
    let mut arguments = arguments.into_iter();
    let Some(argument) = arguments.next() else {
        return Ok(0);
    };
    if argument != "--seed" {
        return Err(format!("unknown argument '{argument}'; {USAGE}"));
    }
    let value = arguments.next().ok_or_else(|| USAGE.to_owned())?;
    if arguments.next().is_some() {
        return Err(USAGE.to_owned());
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid seed '{value}'"))
}

fn main() -> ExitCode {
    let seed = match parse_seed(std::env::args().skip(1)) {
        Ok(seed) => seed,
        Err(error) => {
            eprintln!("progressus-client: {error}");
            return ExitCode::from(2);
        }
    };
    match progressus_client::run_with_seed(seed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("progressus-client: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_seed;

    #[test]
    fn seed_argument_defaults_to_zero_and_parses_explicit_values() {
        assert_eq!(parse_seed(Vec::<String>::new()).unwrap(), 0);
        assert_eq!(
            parse_seed(["--seed".to_owned(), "73".to_owned()]).unwrap(),
            73
        );
        assert!(parse_seed(["--seed".to_owned(), "nope".to_owned()]).is_err());
    }
}
