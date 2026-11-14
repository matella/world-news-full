import assert from 'node:assert/strict';
import test from 'node:test';
import { classify, legacySection } from '../src/lib/classification.ts';

test('independent topic/geography fixtures and explicit unknowns', () => {
  for (const [language, topics, geography, expected] of [
    ['fr', ['science'], ['world'], 'science-tech-economy'],
    ['nl', ['politics'], ['be'], 'belgium'],
    ['es', ['society'], ['world'], 'international'],
    ['fr', ['science', 'sport'], ['be', 'world'], 'sport'],
    ['fr', ['politics'], [], 'unclassified'],
  ]) {
    assert.equal(classify(topics, geography).section.id, expected, language);
  }
  assert.equal(legacySection('Monde').id, 'unclassified');
  assert.equal(legacySection('tech').id, 'science-tech-economy');
  assert.equal(legacySection('').id, 'unclassified');
});
