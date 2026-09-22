pub mod core;
pub mod http;
pub mod tcp;
pub mod serve;
pub mod query_cmd;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let subcmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "serve" | "server" | "start" => {
            serve::run(args);
        }
        "query" | "q" => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(query_cmd::run(args));
        }
        "version" | "--version" | "-v" => {
            println!("glaudb v{}", env!("CARGO_PKG_VERSION"));
        }
        _ => {
            print_help();
        }
    }
}

fn print_help() {
    println!("glaudb v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("  glaudb <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("  serve     Start the GlauDB server (HTTP + TCP)");
    println!("  query     Send a TCP query to a running server");
    println!("  version   Print version");
    println!("  help      Print this help message");
    println!();
    println!("EXAMPLES:");
    println!("  glaudb serve --db ./data.db --http 8080 --tcp 8081");
    println!("  glaudb query -a 127.0.0.1:8081 -k <AUTH> -c insert -p '{{\"collection\":\"users\",\"data\":{{...}}}}'");
    println!();
    println!("Run 'glaudb <COMMAND> --help' for command-specific options.");
}
