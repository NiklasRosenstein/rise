# Getting Started

## Prerequisites

- Docker and Docker Compose
- Rust 1.91+ (for CLI development)

## Launch Services

```bash
docker compose up -d
```

This starts:
- **Backend API**: http://localhost:3001
- **PocketBase**: http://localhost:8090

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

Database stored in `pb_data/` (gitignored). To reset:

```bash
docker compose down
rm -rf pb_data/
docker compose up -d
```

PocketBase will recreate from migrations in `pb_migrations/`.
