# Database Migrations

<!--
==============================================================================
README.md - Genetics Database Migrations Guide
==============================================================================
Description: Instructions for applying and managing database schema migrations
Author: Matt Barham
Created: 2025-11-18
Modified: 2026-01-17
Version: 1.0.0
Repository: https://github.com/captainzonks/GeneGnome
==============================================================================
-->

## Overview

This directory contains sequential database schema migrations for the genetics processing service. Migrations are numbered and must be applied in order.

**Current Schema Version**: 1.1.0
**Database**: genetics
**User**: genetics_api

## Migration Files

| File | Description | Schema Version | Status |
|------|-------------|----------------|--------|
| `001_add_email_downloads.sql` | Email-based download security system | 1.0.2 → 1.1.0 | ✅ Applied |

## Applying Migrations

### Method 1: Direct Application (Recommended)

Apply a migration by piping it through docker exec:

```bash
docker exec -i genetics-postgres psql -U genetics_api -d genetics < 001_add_email_downloads.sql
```

### Method 2: Interactive psql Session

Enter the container and run migrations interactively:

```bash
# Copy migration to container
docker cp 001_add_email_downloads.sql genetics-postgres:/tmp/

# Execute in container
docker exec genetics-postgres psql -U genetics_api -d genetics -f /tmp/001_add_email_downloads.sql
```

### Method 3: From Host (if psql installed)

If PostgreSQL client tools are installed on the host:

```bash
psql -h localhost -p 5433 -U genetics_api -d genetics -f 001_add_email_downloads.sql
```

## Verification

After applying a migration, verify it was recorded in the audit log:

```bash
docker exec genetics-postgres psql -U genetics_api -d genetics -c \
  "SELECT event_type, action, details->>'migration' as migration, details->>'new_version' as version
   FROM genetics.genetics_audit
   WHERE action = 'schema_migration'
   ORDER BY timestamp DESC
   LIMIT 5;"
```

## Migration Standards

### File Naming Convention

Migrations use sequential numbering with descriptive names:

```
NNN_descriptive_name.sql
```

- `NNN`: Zero-padded sequential number (001, 002, 003, etc.)
- `descriptive_name`: Snake_case description of changes
- Examples: `001_add_email_downloads.sql`, `002_add_user_preferences.sql`

### Migration File Structure

Each migration must include:

1. **Header Block**: Metadata following project standards
   - Description, author, dates, version
   - Schema version transition (e.g., 1.0.2 → 1.1.0)
   - Dependencies and rollback reference

2. **SET search_path**: Always set to `genetics, public`

3. **Schema Changes**: DDL statements (CREATE, ALTER, etc.)

4. **Audit Entry**: INSERT into genetics_audit recording the migration

5. **Rollback Section**: Commented-out SQL to revert changes

### Migration Content Rules

- **Idempotency**: Use `IF EXISTS` / `IF NOT EXISTS` where possible
- **Transactions**: Migrations run in a transaction by default (automatic rollback on error)
- **Comments**: Use `COMMENT ON` to document schema elements
- **Grants**: Update permissions for genetics_api role
- **Audit**: Always log the migration in genetics_audit table

## Schema Version Tracking

The schema version is tracked in multiple places:

1. **init.sql header**: Updated to reflect latest migration
2. **genetics_audit table**: Each migration logs its version transition
3. **Migration file headers**: Document the version change

## Rollback Procedure

Each migration includes a commented rollback script at the end of the file. To rollback:

1. Extract the rollback section from the migration file
2. Review carefully - rollback may result in data loss
3. Apply in a test environment first
4. Execute the rollback SQL

**Warning**: Rollbacks are destructive and may lose data. Always backup first.

## Production Deployment

### Pre-Deployment Checklist

- [ ] Migration tested in development environment
- [ ] Backup database before applying migration
- [ ] Verify no active jobs are processing
- [ ] Review migration audit log
- [ ] Test rollback procedure in development

### Deployment Steps

1. **Backup Database**
   ```bash
   docker exec genetics-postgres pg_dump -U genetics_api -d genetics > backup_$(date +%Y%m%d_%H%M%S).sql
   ```

2. **Apply Migration**
   ```bash
   docker exec -i genetics-postgres psql -U genetics_api -d genetics < NNN_migration.sql
   ```

3. **Verify Application**
   ```bash
   # Check audit log
   docker exec genetics-postgres psql -U genetics_api -d genetics -c \
     "SELECT * FROM genetics.genetics_audit WHERE action = 'schema_migration' ORDER BY timestamp DESC LIMIT 1;"
   ```

4. **Update init.sql**: Update the version in the init.sql header to match the new schema version

5. **Document**: Update this README with the migration status

## Troubleshooting

### Migration Fails Midway

PostgreSQL automatically rolls back the entire migration on error. Check the error message and fix the migration file before retrying.

### Permission Denied

Ensure you're using the `genetics_api` user, not `postgres`. The genetics_api role has appropriate permissions for all genetics schema operations.

### Table Already Exists

If re-running a migration, you may encounter "already exists" errors. Check if the migration was partially applied by querying the audit log.

## Best Practices

1. **Test First**: Always apply migrations in development before production
2. **Backup**: Always backup before applying migrations in production
3. **Sequential**: Apply migrations in numerical order
4. **Verify**: Check audit log after each migration
5. **Document**: Update this README after applying migrations
6. **Review**: Have migrations peer-reviewed before production deployment

## References

- [PostgreSQL Documentation](https://www.postgresql.org/docs/current/)
- [Project Documentation](../../docs/)

## Support

For issues with migrations:
1. Check the genetics-postgres container logs: `docker logs genetics-postgres`
2. Review the audit log: `SELECT * FROM genetics.genetics_audit WHERE severity IN ('error', 'critical')`
3. Consult the rollback section in the migration file

---

**Note**: This is a production database system. Always exercise caution when applying schema changes.
