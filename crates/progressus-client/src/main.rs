use std::process::ExitCode;

const USAGE: &str = "usage: progressus-client [--seed <u64>] [--diagnostics]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Options {
    seed: u64,
    diagnostics: bool,
}

fn parse_options(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut seed = None;
    let mut diagnostics = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--seed" => {
                if seed.is_some() {
                    return Err(format!("duplicate --seed; {USAGE}"));
                }
                let value = arguments.next().ok_or_else(|| USAGE.to_owned())?;
                seed = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid seed '{value}'"))?,
                );
            }
            "--diagnostics" => {
                if diagnostics {
                    return Err(format!("duplicate --diagnostics; {USAGE}"));
                }
                diagnostics = true;
            }
            _ => return Err(format!("unknown argument '{argument}'; {USAGE}")),
        }
    }
    Ok(Options {
        seed: seed.unwrap_or(0),
        diagnostics,
    })
}

fn main() -> ExitCode {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("progressus-client: {error}");
            return ExitCode::from(2);
        }
    };
    match progressus_client::run_with_options(options.seed, options.diagnostics) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("progressus-client: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Options, parse_options};

    #[test]
    fn options_default_and_parse_in_any_order() {
        assert_eq!(
            parse_options(Vec::<String>::new()).unwrap(),
            Options {
                seed: 0,
                diagnostics: false
            }
        );
        assert_eq!(
            parse_options([
                "--diagnostics".to_owned(),
                "--seed".to_owned(),
                "73".to_owned(),
            ])
            .unwrap(),
            Options {
                seed: 73,
                diagnostics: true
            }
        );
        assert!(parse_options(["--seed".to_owned(), "nope".to_owned()]).is_err());
        assert!(parse_options(["--diagnostics".to_owned(), "--diagnostics".to_owned(),]).is_err());
    }
}
