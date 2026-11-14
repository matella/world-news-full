import assert from 'node:assert/strict';
import test from 'node:test';
import { adminAccess } from '../src/lib/access.ts';
import { databaseConfig } from '../src/lib/database.ts';
import { validEditionDate, validStoryId } from '../src/lib/validation.ts';

const basic = (value) => `Basic ${Buffer.from(value).toString('base64')}`;

test('admin fails closed when unconfigured and checks both credentials', () => {
  assert.equal(adminAccess(null), 'unconfigured');
  assert.equal(adminAccess(basic('reader:test-only'), 'editor'), 'unconfigured');
  assert.equal(adminAccess(null, 'editor', 'test-only'), 'unauthorised');
  assert.equal(adminAccess(basic('editor:wrong'), 'editor', 'test-only'), 'unauthorised');
  assert.equal(adminAccess(basic('wrong:test-only'), 'editor', 'test-only'), 'unauthorised');
  assert.equal(adminAccess(basic('editor:test-only'), 'editor', 'test-only'), 'allowed');
  assert.equal(adminAccess(basic('editor:test:only'), 'editor', 'test:only'), 'allowed');
  assert.equal(adminAccess('Basic !!!', 'editor', 'test-only'), 'unauthorised');
  assert.equal(adminAccess('Basic ' + 'A'.repeat(3000), 'editor', 'test-only'), 'unauthorised');
});

test('dates reject invalid calendar values before queries and retain leap days', () => {
  for (const value of ['2026-02-29', '2026-13-01', '2026-04-31', '2026-1-01', 'x', '0000-01-01']) assert.equal(validEditionDate(value), false);
  for (const value of ['2024-02-29', '2026-12-31', '2026-01-01']) assert.equal(validEditionDate(value), true);
});

test('opaque legacy IDs are bounded without assuming UUIDs', () => {
  for (const value of ['', 'a/b', 'a\\b', 'a\nb', 'x'.repeat(201)]) assert.equal(validStoryId(value), false);
  assert.equal(validStoryId('legacy-a'), true);
});

test('database config requires an explicit URL and never echoes bad input', () => {
  assert.throws(() => databaseConfig({}), /DATABASE_URL is required/);
  assert.throws(() => databaseConfig({ DATABASE_URL: 'not-a-url-private-value' }), (error) => !error.message.includes('private-value'));
  assert.throws(() => databaseConfig({ DATABASE_URL: 'https://db.invalid/news' }), /Postgres URL/);
  assert.throws(() => databaseConfig({ DATABASE_URL: 'postgresql://db.invalid/' }), /database/);
  assert.equal(databaseConfig({ DATABASE_URL: 'postgresql://reader:fixture@db.invalid/news' }).max, 10);
});
