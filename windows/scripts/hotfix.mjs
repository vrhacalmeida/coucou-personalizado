// Packages only changed native files; it never publishes, installs, or reads user data.
import { readFileSync, mkdirSync, copyFileSync, existsSync, writeFileSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const index = process.argv.indexOf('--base');
if (index !== -1 && !process.argv[index + 1]) throw new Error('Use --base <pasta dos binarios anteriores>');
const base = index === -1 ? null : resolve(process.argv[index + 1]);
const revision = 'custom-' + new Date().toISOString().replace(/[:.]/g, '-');
const dest = join(root, 'release', 'hotfix-' + revision);
const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
const files = [];
for (const name of ['coucou.exe', 'coucou-hook.exe']) {
  const source = join(root, 'target', 'release', name);
  if (!existsSync(source)) throw new Error(`Compile no Windows primeiro. Arquivo ausente: ${name}`);
  const sha256 = hash(source);
  const previous = base && join(base, name);
  if (previous && existsSync(previous) && hash(previous) === sha256) continue;
  files.push({ name, sha256, source });
}
if (!files.length) { console.log('Nenhum binario mudou. Nenhum hotfix criado.'); process.exit(0); }
mkdirSync(join(dest, 'payload'), { recursive: true });
try {
  for (const file of files) copyFileSync(file.source, join(dest, 'payload', file.name));
  const manifest = { schema: 1, revision, version: JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version,
    files: files.map(({ name, sha256 }) => ({ name, sha256 })) };
  writeFileSync(join(dest, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  copyFileSync(join(root, 'hotfix', 'Aplicar-Hotfix.ps1'), join(dest, 'Aplicar-Hotfix.ps1'));
  writeFileSync(join(dest, 'LEIA-ME.txt'), 'Extraia todos os arquivos. No PowerShell dentro desta pasta execute:\r\n' +
    'powershell -NoProfile -ExecutionPolicy Bypass -File .\\Aplicar-Hotfix.ps1\r\n' +
    'Se necessario acrescente -InstallDir "C:\\caminho\\do\\Coucou".\r\n' +
    'O aplicador verifica SHA256, pede para fechar o app e salva os binarios antigos em .coucou-hotfix-backups.\r\n' +
    'So aplique pacotes da fonte em que voce confia: hashes verificam integridade, nao identidade do autor.\r\n');
  console.log('Hotfix preparado em ' + dest);
  console.log('Compacte ESTA pasta em ZIP e distribua. Nao e preciso reinstalar o app.');
} catch (error) { rmSync(dest, { recursive: true, force: true }); throw error; }
