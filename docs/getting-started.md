# Getting Started

## Prerequisites

- Docker and Docker Compose
- Rust 1.91+ (for CLI development)

## Launch Services

```bash
docker compose up -d
```

This starts:
- **Backend API**: http://localhost:3000
- **PostgreSQL**: localhost:5432
- **Dex Auth**: http://localhost:5556/dex

## Build CLI

```bash
cargo build --bin rise-cli
```

## First Steps

### 1. Login

```bash
./target/debug/rise-cli login --email test@example.com --password test1234
```

Default credentials: `test@example.com` / `test1234`

### 2. Create a Project

```bash
./target/debug/rise-cli project create my-app --visibility public
```

### 3. Create a Team

```bash
./target/debug/rise-cli team create devops
```

### 4. Transfer Ownership

```bash
./target/debug/rise-cli project update my-app --owner team:devops
```

## Development Database

Database stored in PostgreSQL Docker volume. To reset:

```bash
docker compose down -v  # -v removes volumes
docker compose up -d
cd rise-backend
sqlx migrate run
```

SQLX will apply all migrations from `rise-backend/migrations/`.
