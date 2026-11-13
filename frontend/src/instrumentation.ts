export async function register() {
  if (process.env.NEXT_RUNTIME === 'nodejs') {
    const { databaseConfig } = await import('./lib/database');
    databaseConfig();
    if (Boolean(process.env.NEWS_ADMIN_USER) !== Boolean(process.env.NEWS_ADMIN_PASSWORD)) {
      throw new Error('Configure both NEWS_ADMIN_USER and NEWS_ADMIN_PASSWORD, or leave administration disabled.');
    }
  }
}
