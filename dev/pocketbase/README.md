# PocketBase Development Container

Custom PocketBase Docker image for the Rise project with additional tooling and automatic initialization.

## Features

- Based on the official PocketBase image (`ghcr.io/muchobien/pocketbase:latest`)
- Includes `curl` and `jq` for API scripting and automation
- Auto-creates admin superuser and test user on startup
- Automatically runs JavaScript migrations from `pb_migrations/` directory
- Configurable via environment variables

## Environment Variables

All configuration can be overridden via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `POCKETBASE_ADMIN_EMAIL` | `admin@example.com` | Admin superuser email |
| `POCKETBASE_ADMIN_PASSWORD` | `admin123` | Admin superuser password |
| `POCKETBASE_TEST_USER_EMAIL` | `test@example.com` | Test user email |
| `POCKETBASE_TEST_USER_PASSWORD` | `test1234` | Test user password |

## Volume Mounts

The container uses two volume mounts:

1. `./pb_data:/pb_data` - Database and runtime data (gitignored)
   - Contains SQLite databases (`data.db`, `auxiliary.db`)
   - Stores uploaded files and assets
   - Generated TypeScript types (`types.d.ts`)

2. `./pb_migrations:/pb_migrations` - Migration files (committed to git)
   - JavaScript migration files that define schema
   - Auto-generated when collections are modified via Admin UI
   - Version-controlled for reproducible deployments

## Accessing the Admin UI

- **URL**: http://localhost:8090/_/
- **Default Credentials**:
  - Email: `admin@example.com`
  - Password: `admin123`

Use the Admin UI to:
- Create and modify collections
- View and edit records
- Configure API rules and permissions
- Export/import schema
- Monitor logs and metrics

## API Endpoints

- **Admin Dashboard**: http://localhost:8090/_/
- **REST API**: http://localhost:8090/api/
- **Health Check**: http://localhost:8090/api/health

## Startup Process

The `entrypoint.sh` script performs these steps:

1. Start PocketBase with migrations enabled
2. Wait for server to be ready (health check)
3. Create admin superuser using `pocketbase superuser upsert` (idempotent)
4. Authenticate as admin via API
5. Create test user in `users` collection (if it doesn't exist)
6. Report initialization status

## Migrations

PocketBase automatically runs migrations on startup:

```bash
pocketbase serve --migrationsDir=/pb_migrations --automigrate
```

- Migrations in `pb_migrations/` are applied in order by timestamp
- Applied migrations are tracked in the `_migrations` table
- New migrations are generated when you modify collections via Admin UI

For details on managing migrations, see `/pb_migrations/README.md`

## Logs

View PocketBase logs:

```bash
docker-compose logs -f pocketbase
```

Check for:
- Server startup confirmation
- Admin/test user creation status
- Migration application results
- API request logs (if enabled)

## Troubleshooting

**Container won't start:**
- Check logs: `docker-compose logs pocketbase`
- Verify volume mounts are correct
- Ensure port 8090 isn't already in use

**Admin UI not accessible:**
- Verify container is running: `docker-compose ps`
- Check health status: `curl http://localhost:8090/api/health`
- Try restarting: `docker-compose restart pocketbase`

**Migrations not applying:**
- Ensure `pb_migrations/` volume is mounted
- Check migration file syntax
- Review logs for migration errors

**Test user not created:**
- Check if authentication succeeded in logs
- Verify `users` collection exists (created by PocketBase by default)
- Try manually creating via Admin UI

## Development Workflow

1. **Start services**: `docker-compose up -d`
2. **Make schema changes**: Access Admin UI, modify collections
3. **Review migrations**: Check `pb_migrations/` for new files
4. **Test changes**: Verify via API or Admin UI
5. **Commit migrations**: `git add pb_migrations/ && git commit`

## Production Considerations

For production deployments:

- Change default admin credentials via environment variables
- Use secure, randomly-generated passwords
- Consider using external PostgreSQL instead of SQLite
- Enable TLS/HTTPS termination at load balancer
- Set up regular database backups of `pb_data/`
- Monitor PocketBase logs and metrics

## References

- [PocketBase Official Documentation](https://pocketbase.io/docs/)
- [PocketBase API Rules](https://pocketbase.io/docs/api-rules/)
- [JavaScript Migrations Guide](https://pocketbase.io/docs/js-migrations/)
