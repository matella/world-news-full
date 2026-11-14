import pg, { type QueryResultRow } from 'pg';

let pool: pg.Pool | undefined;

export function databaseConfig(env: NodeJS.ProcessEnv = process.env): pg.PoolConfig {
  const connectionString = env.DATABASE_URL;
  if (!connectionString) throw new Error('DATABASE_URL is required.');
  let url: URL;
  try { url = new URL(connectionString); } catch { throw new Error('DATABASE_URL must be a Postgres URL.'); }
  if (!['postgres:', 'postgresql:'].includes(url.protocol)) throw new Error('DATABASE_URL must be a Postgres URL.');
  if (!url.hostname || !url.pathname.slice(1)) throw new Error('DATABASE_URL must specify a host and database.');
  return { connectionString, max: 10, connectionTimeoutMillis: 3000, idleTimeoutMillis: 30000, statement_timeout: 5000, query_timeout: 6000 };
}

export class DatabaseUnavailableError extends Error {
  constructor() { super('Le service de données est temporairement indisponible.'); this.name = 'DatabaseUnavailableError'; }
}

export async function query<T extends QueryResultRow = QueryResultRow>(operation: string, sql: string, values?: unknown[]) {
  try {
    if (!pool) {
      pool = new pg.Pool(databaseConfig());
      pool.on('error', () => console.error(JSON.stringify({ event: 'database_pool_error' })));
    }
    return await pool.query<T>(sql, values);
  } catch (error) {
    const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : 'UNKNOWN';
    console.error(JSON.stringify({ event: 'database_query_failed', operation, code: /^[A-Z0-9_]{1,32}$/.test(code) ? code : 'UNKNOWN' }));
    // No error cause: Next's server logger must not print driver messages/credentials.
    throw new DatabaseUnavailableError();
  }
}
