const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(`${__dirname}/../src/lib/transcript-format.ts`, 'utf8');
const compiled = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS}}).outputText;
const sandbox = {exports: {}};
vm.runInNewContext(compiled, sandbox);
const {formatTranscript, normalizeTranscript} = sandbox.exports;

test('normalizes all historical formats and preserves edited speakers', () => {
  const rows = [{text: 'Décision validée.', speaker: 'Speaker É', start: 4354, end: 4358}];
  for (const input of [rows, JSON.stringify(rows), {segments: rows}, {transcript: JSON.stringify(rows)},
    {transcripts: [{text: JSON.stringify(rows)}]}]) {
    assert.equal(formatTranscript(input), '[01:12:34] Speaker É : Décision validée.');
  }
});
test('copies compact plain text without leaking JSON or inventing wall-clock time', () => {
  const result = formatTranscript([{text: 'Bonjour', audio_start_time: 0}, {text: 'Merci', speaker: 'Speaker A'}]);
  assert.equal(result, '[00:00:00] Intervenant inconnu : Bonjour\n[--:--:--] Speaker A : Merci');
});
test('rejects corrupted nested JSON and missing text', () => {
  for (const input of ['not json', {unexpected: []}, [{text: '[{"broken":1}]'}], [{speaker: 'Speaker A'}], []]) {
    assert.throws(() => formatTranscript(input));
  }
});
test('accepts ordinary bracketed speech and does not mutate the source', () => {
  const input = [{text: '[Speaker A] Bonjour', speaker: 'Speaker A', start: 4}];
  const snapshot = JSON.stringify(input);
  assert.equal(normalizeTranscript(input)[0].text, '[Speaker A] Bonjour');
  assert.equal(JSON.stringify(input), snapshot);
});

test('preserves word alignments when speaker names are edited and compacts newlines', () => {
  const rows = [{text: 'Bonjour\n  Speaker É', speaker: 'Speaker A', start: 0, words: [{word: 'Bonjour', start: 0, end: 1, probability: 0.9}]}];
  assert.equal(JSON.stringify(normalizeTranscript(rows)[0].words), JSON.stringify(rows[0].words));
  assert.equal(formatTranscript(rows), '[00:00:00] Speaker A : Bonjour Speaker É');
  assert.throws(() => formatTranscript([{text: '[ {"broken":1} ]'}]));
});
