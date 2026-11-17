import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, copyFileSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'coucou-hotfix-'));
  for (const dir of ['scripts', 'hotfix', 'target/release', 'old']) mkdirSync(join(root, dir), { recursive: true });
  copyFileSync(new URL('../scripts/hotfix.mjs', import.meta.url), join(root, 'scripts/hotfix.mjs'));
  writeFileSync(join(root, 'package.json'), JSON.stringify({ version: '0.5.9' }));
  writeFileSync(join(root, 'hotfix/Aplicar-Hotfix.ps1'), '# test fixture');
  return root;
}
test('hotfix packages changed binaries only with verifiable hashes and applicator', () => {
  const root = fixture();
  try {
    writeFileSync(join(root, 'target/release/coucou.exe'), 'new-app');
    writeFileSync(join(root, 'target/release/coucou-hook.exe'), 'same-hook');
    writeFileSync(join(root, 'old/coucou.exe'), 'old-app');
    writeFileSync(join(root, 'old/coucou-hook.exe'), 'same-hook');
    const run = spawnSync(process.execPath, [join(root, 'scripts/hotfix.mjs'), '--base', join(root, 'old')], { encoding: 'utf8' });
    assert.equal(run.status, 0, run.stderr);
    const dest = join(root, 'release', readdirSync(join(root, 'release'))[0]);
    const manifest = JSON.parse(readFileSync(join(dest, 'manifest.json'), 'utf8'));
    assert.deepEqual(manifest.files.map((file) => file.name), ['coucou.exe']);
    assert.equal(manifest.files[0].sha256, createHash('sha256').update(readFileSync(join(dest, 'payload/coucou.exe'))).digest('hex'));
    assert.ok(readFileSync(join(dest, 'Aplicar-Hotfix.ps1')).length);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
test('hotfix refuses an incomplete Windows build', () => {
  const root = fixture();
  try {
    writeFileSync(join(root, 'target/release/coucou.exe'), 'app');
    const run = spawnSync(process.execPath, [join(root, 'scripts/hotfix.mjs')], { encoding: 'utf8' });
    assert.notEqual(run.status, 0);
    assert.match(run.stderr, /coucou-hook.exe/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
