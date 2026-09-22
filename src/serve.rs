use crate::core::{Database, GlauConfig};

pub fn run(args: Vec<String>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(run_async(args));
}

async fn run_async(args: Vec<String>) {
    let mut config = GlauConfig::load();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&config.log_level)).init();

    log::info!("GlauDB v{} starting...", env!("CARGO_PKG_VERSION"));

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--db" | "-d" => {
                if i + 1 < args.len() {
                    config.db_path = args[i + 1].clone();
                    i += 2;
                } else {
                    log::error!("Missing value for --db");
                    return;
                }
            }
            "--http" => {
                if i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse() {
                        config.http_port = p;
                    }
                    i += 2;
                } else {
                    log::error!("Missing value for --http");
                    return;
                }
            }
            "--tcp" => {
                if i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse() {
                        config.tcp_port = p;
                    }
                    i += 2;
                } else {
                    log::error!("Missing value for --tcp");
                    return;
                }
            }
            "--uploads" => {
                if i + 1 < args.len() {
                    config.uploads_dir = args[i + 1].clone();
                    i += 2;
                } else {
                    log::error!("Missing value for --uploads");
                    return;
                }
            }
            "--backups" => {
                if i + 1 < args.len() {
                    config.backups_dir = args[i + 1].clone();
                    i += 2;
                } else {
                    log::error!("Missing value for --backups");
                    return;
                }
            }
            "--help" | "-h" => {
                println!("glaudb serve — Start the GlauDB server");
                println!();
                println!("USAGE:");
                println!("  glaudb serve [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("  -d, --db <PATH>       Database file path (default: glaudb.db)");
                println!("  --http <PORT>         HTTP port for Admin UI & REST API (default: 8080)");
                println!("  --tcp <PORT>          TCP port for high-speed binary queries (default: 8081)");
                println!("  --uploads <PATH>      Upload storage directory (default: uploads)");
                println!("  --backups <PATH>      Backup storage directory (default: backups)");
                println!("  -h, --help            Show this help");
                return;
            }
            _ => {
                log::warn!("Unknown option: {}", args[i]);
                i += 1;
            }
        }
    }

    if config.admin_password.len() < 12 {
        eprintln!("ERROR: admin_password must be at least 12 characters (set in glaudb.json or GLAUDB_ADMIN_PASSWORD env).");
        std::process::exit(1);
    }

    log::info!("Opening database: {}", config.db_path);
    let db = match Database::open(&config.db_path) {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to open database: {}", e);
            std::process::exit(1);
        }
    };

    let db_tcp = db.clone();
    let tcp_addr = format!("127.0.0.1:{}", config.tcp_port);
    let tcp_cfg = config.clone();
    tokio::spawn(async move {
        crate::tcp::start_tcp_server(db_tcp, tcp_cfg, &tcp_addr).await;
    });

    let http_addr = format!("0.0.0.0:{}", config.http_port);
    crate::http::start_http_server(db, config, &http_addr).await;
}
