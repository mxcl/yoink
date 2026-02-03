use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        print_usage();
        return ExitCode::from(2);
    };

    if first == "-h" || first == "--help" {
        print_usage();
        return ExitCode::SUCCESS;
    }

    if yoink::is_repo_shape(&first) {
        if args.next().is_some() {
            eprintln!("yoink: extra arguments after repo are not supported yet");
            return ExitCode::from(2);
        }
        match yoink::install(&first) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("yoink: {err:?}");
                ExitCode::from(1)
            }
        }
    } else {
        eprintln!("yoink: expected owner/repo as the first argument");
        print_usage();
        ExitCode::from(2)
    }
}

fn print_usage() {
    eprintln!("usage: yoink <owner/repo>");
}
