import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { cleanAppName, setAppName, appName, brandText } from '../src/core/branding.ts';
import { setLanguage, t } from '../src/i18n/i18n.ts';
afterEach(() => { setAppName('Coucou'); setLanguage('en'); });
test('names allow bounded natural labels and reject paths or shell metacharacters', () => {
  assert.equal(cleanAppName('  Jarvis  Azul  '), 'Jarvis Azul');
  assert.equal(cleanAppName('Lua São'), 'Lua São');
  for (const name of ['../Jarvis', 'x&calc', 'x%PATH%', '---', 'a'.repeat(33), '', null]) assert.equal(cleanAppName(name), 'Coucou');
});
test('custom name updates translated labels and wake instructions without rewriting technical IDs', () => {
  setAppName('Jarvis');
  assert.equal(appName(), 'Jarvis');
  assert.equal(t('Open Coucou'), 'Open Jarvis');
  setLanguage('fr');
  assert.equal(t('Open Coucou'), 'Ouvrir Jarvis');
  assert.equal(t('Name: {name}', { name: 'Coucou' }), 'Name: Coucou');
  assert.equal(brandText('OK Coucou · coucou-hook.exe'), 'OK Jarvis · coucou-hook.exe');
});
