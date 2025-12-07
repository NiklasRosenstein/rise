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
cargo build --bin rise
```

**Note**: If using `direnv`, the CLI is available as `rise` directly. Otherwise, use `./target/debug/rise`.

## First Steps

### 1. Login

```bash
rise login
```

This will:
1. Open your browser to Dex authentication
2. Start a local callback server on port 8765 (or 8766/8767 if occupied)
3. Redirect back to CLI after successful authentication

**Default Dex credentials:**
- Email: `admin@example.com` or `test@example.com`
- Password: `password`

See [Authentication](../core-concepts/authentication.md) for more details on authentication flows.

### 2. Create a Project

```bash
rise project create my-app --visibility public
```

### 3. Create a Team

```bash
rise team create devops
```

### 4. Transfer Ownership

```bash
rise project update my-app --owner team:devops
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
