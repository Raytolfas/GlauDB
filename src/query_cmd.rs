use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use bytes::Bytes;
use std::process;

pub async fn run(args: Vec<String>) {
    let mut addr = "127.0.0.1:8081".to_string();
    let mut auth = String::new();
    let mut cmd_str = String::new();
    let mut payload_str = String::new();
    let mut out_path = String::new();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--addr" | "-a" => {
                if i + 1 < args.len() {
                    addr = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --addr");
                    process::exit(1);
                }
            }
            "--auth" | "-k" | "--key" => {
                if i + 1 < args.len() {
                    auth = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --auth");
                    process::exit(1);
                }
            }
            "--cmd" | "-c" => {
                if i + 1 < args.len() {
                    cmd_str = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --cmd");
                    process::exit(1);
                }
            }
            "--payload" | "-p" => {
                if i + 1 < args.len() {
                    payload_str = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --payload");
                    process::exit(1);
                }
            }
            "--out" | "-o" => {
                if i + 1 < args.len() {
                    out_path = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --out");
                    process::exit(1);
                }
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                process::exit(1);
            }
        }
    }

    if cmd_str.is_empty() {
        eprintln!("Missing required argument: --cmd");
        print_help();
        process::exit(1);
    }

    let cmd: u8 = match cmd_str.parse() {
        Ok(c) => c,
        Err(_) => match cmd_str.to_lowercase().as_str() {
            "create-collection" => 1,
            "delete-collection" => 2,
            "insert" => 3,
            "get" => 4,
            "update" => 5,
            "delete" => 6,
            "query" => 7,
            "backup" => 8,
            "patch" => 9,
            "bulk-insert" => 10,
            "bulk-delete" => 11,
            "search" => 12,
            "aggregate" => 13,
            "count" => 14,
            "upsert" => 15,
            "find-one" => 16,
            _ => {
                eprintln!("Unknown command: {}. Use a number (1-16) or a command name.", cmd_str);
                process::exit(1);
            }
        },
    };

    let payload: Value = if payload_str.starts_with('@') {
        let path = &payload_str[1..];
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read payload file {}: {}", path, e);
                process::exit(1);
            }
        };
        serde_json::from_str(&content).unwrap_or(Value::Null)
    } else if payload_str.is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(&payload_str) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Invalid JSON payload: {}", e);
                process::exit(1);
            }
        }
    };

    let stream = match TcpStream::connect(&addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to connect to GlauDB at {}: {}", addr, e);
            process::exit(1);
        }
    };

    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    let packet = serde_json::to_vec(&json!({
        "auth": if auth.is_empty() { Value::Null } else { Value::String(auth) },
        "cmd": cmd,
        "payload": payload
    })).unwrap();

    if let Err(e) = framed.send(Bytes::from(packet)).await {
        eprintln!("Failed to send query: {}", e);
        process::exit(1);
    }

    match framed.next().await {
        Some(Ok(resp_bytes)) => {
            if resp_bytes.is_empty() {
                eprintln!("Empty response from server");
                process::exit(1);
            }
            let status = resp_bytes[0];
            let body = &resp_bytes[1..];
            match serde_json::from_slice::<Value>(body) {
                Ok(val) => {
                    if status == 0 {
                        let formatted = serde_json::to_string_pretty(&val).unwrap();
                        if !out_path.is_empty() {
                            if let Err(e) = std::fs::write(&out_path, &formatted) {
                                eprintln!("Failed to write output: {}", e);
                                process::exit(1);
                            }
                        } else {
                            println!("{}", formatted);
                        }
                    } else {
                        let msg = val["error"].as_str().unwrap_or("Unknown server error");
                        eprintln!("Database Error: {}", msg);
                        process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse response: {}", e);
                    process::exit(1);
                }
            }
        }
        Some(Err(e)) => {
            eprintln!("Failed to read response: {}", e);
            process::exit(1);
        }
        None => {
            eprintln!("Connection closed by server");
            process::exit(1);
        }
    }
}

fn print_help() {
    println!("glaudb query — Send a TCP query to a running GlauDB server");
    println!();
    println!("USAGE:");
    println!("  glaudb query --cmd <CMD> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("  1,  create-collection    Create a collection");
    println!("  2,  delete-collection    Delete a collection");
    println!("  3,  insert               Insert a document");
    println!("  4,  get                  Get document by ID");
    println!("  5,  update               Replace document");
    println!("  6,  delete               Delete document by ID");
    println!("  7,  query                Query documents");
    println!("  8,  backup               Backup database file");
    println!("  9,  patch                Partial update (PATCH)");
    println!("  10, bulk-insert          Insert multiple documents");
    println!("  11, bulk-delete          Delete multiple documents by ID");
    println!("  12, search               Full-text search");
    println!("  13, aggregate            Aggregation pipeline");
    println!("  14, count                Count matching documents");
    println!("  15, upsert               Insert or update by field");
    println!("  16, find-one             Find first matching document");
    println!();
    println!("OPTIONS:");
    println!("  -a, --addr <IP:PORT>     Server TCP address (default: 127.0.0.1:8081)");
    println!("  -k, --auth <KEY>         Authentication password or API token");
    println!("  -c, --cmd <CMD>          Command number or name");
    println!("  -p, --payload <JSON>     JSON payload or @file.json");
    println!("  -o, --out <FILE>         Write response JSON to file");
    println!("  -h, --help               Show this help");
}
