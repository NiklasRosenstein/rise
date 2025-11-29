# PocketBase Migrations

Auto-generated JavaScript migrations that define the database schema for the Rise project.

## How Migrations Work

- Migrations are **auto-generated** when you modify collections via the Admin UI
- PocketBase runs migrations automatically on startup (`--automigrate` flag enabled)
- Migration files are committed to git and tracked in version control
- Applied migrations are tracked in the internal `_migrations` table in PocketBase

## Making Schema Changes

1. Start PocketBase: `docker-compose up -d pocketbase`
2. Open Admin UI: http://localhost:8090/_/
3. Login with credentials:
   - Email: `admin@example.com`
   - Password: `admin123`
4. Navigate to Collections and modify as needed
5. PocketBase automatically generates migration file in this directory
6. Review the generated file: `cat pb_migrations/[newest-file].js`
7. Test your changes locally
8. Commit the migration: `git add pb_migrations/ && git commit -m "feat: description of schema change"`

## Fresh Environment Setup

When setting up a new development environment or deploying:

1. Clone the repository (includes all migration files)
2. Run `docker-compose up -d`
3. PocketBase automatically applies all migrations on startup
4. Collections are created with the correct schema
5. System is ready to use

## Migration File Format

Migration files follow this structure:

```javascript
migrate((db) => {
  // Up migration - creates/modifies schema
  const collection = new Collection({
    "id": "...",
    "name": "collection_name",
    "type": "base",
    "schema": [/* field definitions */]
  })
  return db.saveCollection(collection)
}, (db) => {
  // Down migration - rollback changes
  return db.deleteCollection("...")
})
```

Files are named: `[timestamp]_[description].js`

## Troubleshooting

**Migration file not appearing:**
- Ensure `pb_migrations/` volume is mounted in `docker-compose.yml`
- Check PocketBase logs: `docker-compose logs pocketbase`
- Verify you saved the collection changes in Admin UI

**Migration failed:**
- Check error in PocketBase logs
- Verify migration file syntax is valid JavaScript
- Ensure no conflicting migrations

**Schema out of sync:**
- Use Admin UI to manually export/import schema if needed
- Check that all migrations have been applied: look at `_migrations` table

## References

- [PocketBase JS Migrations Documentation](https://pocketbase.io/docs/js-migrations/)
- [PocketBase Collections API](https://pocketbase.io/docs/api-collections/)
