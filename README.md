# GlauDB

High-performance, embedded document database engine with an integrated Web Admin Panel and binary TCP protocol.

## Features

- **Single Binary**: The entire server, REST API, WebSocket real-time feed, high-speed binary TCP query engine, and embedded React Admin Dashboard are packed into a single executable (`glaudb` / `glaudb.exe`).
- **Storage Engine**: Transactional ACID key-value storage powered by [redb](https://github.com/cberner/redb).
- **Pro Query Engine**: Filtering (`eq`, `gt`, `gte`, `lt`, `lte`, `contains`, `in`, `nin`, `startswith`, `endswith`, `exists`), multiple sorting, offset/limit pagination, aggregation pipelines, and trigram-based full-text search.
- **Embedded Web Admin UI**: Responsive dark/light dashboard with real-time metrics, collection visualizer, document CRUD, API token management, webhooks, and live server log stream.
- **RayQuiro SDK**: Dedicated driver library in `glaudb-rqio/` for seamless integration with the [RayQuiro](https://rayquiro.raytolfas.com) language.

---

## Quick Start

### 1. Launch Server

```bash
# Windows
.\glaudb.exe serve

# Linux / Docker
./glaudb serve
```

Default endpoints:
- **Web Admin UI**: `http://localhost:8080/admin`
- **REST API**: `http://localhost:8080/api`
- **High-speed Binary TCP**: `127.0.0.1:8081`

### 2. Configuration (`glaudb.json` or Environment)

```json
{
  "admin_password": "your_secure_password_here_12_chars",
  "http_port": 8080,
  "tcp_port": 8081,
  "db_path": "glaudb.db",
  "uploads_dir": "uploads",
  "backups_dir": "backups"
}
```

Or pass via command-line flags:
```bash
glaudb serve --db ./data.db --http 8080 --tcp 8081
```

### 3. CLI Queries

Send direct queries without external tools:

```bash
# Insert document
glaudb query -k "your_password" -c insert -p '{"collection":"users","data":{"name":"Alex","email":"alex@example.com"}}'

# Query documents
glaudb query -k "your_password" -c query -p '{"collection":"users","query":{"filters":[{"field":"name","op":"eq","value":"Alex"}]}}'
```

---

## Docker Support

```bash
# Build and run
docker compose up --build -d
```

---

## RayQuiro Language Integration

Check the [glaudb-rqio](glaudb-rqio/) directory:

```rq
import "glaudb";

async fn main() {
    glaudb.connect("127.0.0.1:8081", "your_password_or_token");

    var user = glaudb.insert("users", "{\"name\":\"Alex\"}");
    print("Created ID: " + user?.["id"]);
}

main();
```

---

## License

[Apache License 2.0](https://github.com/Raytolfas/GlauDB/blob/main/LICENSE)
