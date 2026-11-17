# Exercises real Windows file replacement in disposable folders; never runs an executable.
$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('coucou-hotfix-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
function Check($condition, [string]$message) { if (!$condition) { throw $message } }
function Put([string]$path, [string]$value) { [IO.File]::WriteAllText($path, $value, [Text.Encoding]::ASCII) }
function Prepare([string]$name) {
    $root = Join-Path $testRoot $name
    $install = Join-Path $root 'installed'
    $package = Join-Path $root 'package'
    $payload = Join-Path $package 'payload'
    New-Item -ItemType Directory -Path $install, $payload -Force | Out-Null
    Put (Join-Path $install 'coucou.exe') 'old-app'
    Put (Join-Path $install 'coucou-hook.exe') 'old-hook'
    Put (Join-Path $install 'settings.json') 'user-settings-untouched'
    Put (Join-Path $payload 'coucou.exe') 'new-app'
    Put (Join-Path $payload 'coucou-hook.exe') 'new-hook'
    Copy-Item (Join-Path $PSScriptRoot 'Aplicar-Hotfix.ps1') (Join-Path $package 'Aplicar-Hotfix.ps1')
    $files = @('coucou.exe', 'coucou-hook.exe') | ForEach-Object {
        @{ name = $_; sha256 = (Get-FileHash (Join-Path $payload $_) -Algorithm SHA256).Hash }
    }
    @{ schema = 1; revision = 'test'; files = @($files) } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $package 'manifest.json') -Encoding UTF8
    @{ Install = $install; Package = $package; Payload = $payload; Apply = (Join-Path $package 'Aplicar-Hotfix.ps1') }
}
try {
    $test = Prepare 'success'
    & $test.Apply -InstallDir $test.Install
    Check (([IO.File]::ReadAllText((Join-Path $test.Install 'coucou.exe'))) -eq 'new-app') 'App was not updated'
    Check (([IO.File]::ReadAllText((Join-Path $test.Install 'settings.json'))) -eq 'user-settings-untouched') 'Settings changed'
    $backup = @(Get-ChildItem (Join-Path $test.Install '.coucou-hotfix-backups') -Directory)[0].FullName
    Check (([IO.File]::ReadAllText((Join-Path $backup 'coucou.exe'))) -eq 'old-app') 'Original app was not backed up'

    $test = Prepare 'bad-hash'
    Put (Join-Path $test.Payload 'coucou.exe') 'modified-after-manifest'
    $failed = $false
    try { & $test.Apply -InstallDir $test.Install } catch { $failed = $true }
    Check $failed 'Modified payload was accepted'
    Check (([IO.File]::ReadAllText((Join-Path $test.Install 'coucou.exe'))) -eq 'old-app') 'Invalid payload changed installation'

    $test = Prepare 'rollback'
    $locked = [IO.File]::Open((Join-Path $test.Install 'coucou-hook.exe'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $failed = $false
        try { & $test.Apply -InstallDir $test.Install } catch { $failed = $true }
        Check $failed 'Locked destination should have failed'
        Check (([IO.File]::ReadAllText((Join-Path $test.Install 'coucou.exe'))) -eq 'old-app') 'App was not rolled back after a later copy failed'
        Check (([IO.File]::ReadAllText((Join-Path $test.Install 'settings.json'))) -eq 'user-settings-untouched') 'Rollback changed settings'
    } finally { $locked.Dispose() }
    Write-Host 'PASS: replacement, backup, settings preservation, bad hash and rollback.'
} finally {
    Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
}
