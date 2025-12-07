# Contributing

## Development Setup

1. **Start services:**
   ```bash
   docker compose up -d
   ```

2. **Build CLI:**
   ```bash
   cargo build --bin rise-cli
   ```

3. **Login:**
   ```bash
   ./target/debug/rise-cli login --email test@example.com --password test1234
   ```

## Making Changes

### Backend Changes

```bash
cd rise-backend
cargo build
cargo test

# Rebuild container
docker compose build rise-backend
docker compose up -d rise-backend
```

### CLI Changes

```bash
cargo build --bin rise-cli
./target/debug/rise-cli <command>
```

### Schema Changes

1. Create a new SQLX migration:
   ```bash
   cd rise-backend
   sqlx migrate add <migration_name>
   ```
2. Edit the migration file in `migrations/`
3. Run migrations: `sqlx migrate run`
4. Update SQLX cache: `cargo sqlx prepare`
5. Review and commit migration files

## Code Style

**Keep it simple:**
- Avoid over-engineering for hypothetical use cases
- Don't add abstractions until needed
- Three similar lines > premature abstraction

**Error handling:**
- Use `anyhow::Result` for application code
- Use typed errors only when callers need to handle specific cases
- Provide context with `.context("what failed")`

**Documentation:**
- Document non-obvious behavior
- Explain "why" not "what" (code shows "what")
- Update mdbook when adding features

## Testing

```bash
# Backend tests
cd rise-backend && cargo test

# Full integration test
docker compose down -v  # -v removes volumes
docker compose up -d
cd rise-backend && sqlx migrate run && cd ..
./target/debug/rise-cli login --email test@example.com --password test1234
./target/debug/rise-cli project create test-app
```

## Commit Messages

Use conventional commits:
```
feat: add ECR registry support
fix: validate JWT before database queries
docs: update registry security notes
refactor: extract fuzzy matching to module
```

Include co-authorship when using AI assistance:
```
Co-Authored-By: Claude <noreply@anthropic.com>
```

## Pull Requests

1. Create feature branch
2. Make focused changes (one feature per PR)
3. Add tests if adding features
4. Update documentation
5. Ensure `cargo test` passes
6. Include migration files if schema changed
