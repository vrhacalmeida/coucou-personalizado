param([string]$InstallDir)
$ErrorActionPreference = 'Stop'
$manifestPath = Join-Path $PSScriptRoot 'manifest.json'
if (!(Test-Path -LiteralPath $manifestPath)) { throw 'Extraia o ZIP completo do hotfix antes de executar.' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$allowed = @('coucou.exe', 'coucou-hook.exe')
if ($manifest.schema -ne 1 -or @($manifest.files).Count -eq 0) { throw 'Manifesto de hotfix invalido.' }
$names = @()
foreach ($file in $manifest.files) {
    if ($file.name -cnotin $allowed -or $file.name -in $names) { throw 'Arquivo inesperado ou duplicado no hotfix.' }
    $names += $file.name
    $source = Join-Path (Join-Path $PSScriptRoot 'payload') $file.name
    if (!(Test-Path -LiteralPath $source -PathType Leaf)) { throw "Arquivo ausente: $($file.name)" }
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $file.sha256) { throw "Verificacao SHA256 falhou: $($file.name)" }
}
if (!$InstallDir) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Coucou'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Coucou')
    )
    foreach ($proc in @(Get-Process -Name coucou -ErrorAction SilentlyContinue)) {
        if ($proc.Path) { $candidates += Split-Path -Parent $proc.Path }
    }
    foreach ($key in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*')) {
        foreach ($app in @(Get-ItemProperty $key -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq 'Coucou' })) {
            if ($app.InstallLocation) { $candidates += $app.InstallLocation }
        }
    }
    # Uninstall registry entries can surround InstallLocation with quotes.
    # Do not pass untrusted candidate strings directly to PowerShell drive parsing.
    $found = @(@(foreach ($candidate in $candidates) {
        if (!$candidate) { continue }
        $folder = [Environment]::ExpandEnvironmentVariables(([string]$candidate).Trim().Trim('"'))
        try {
            if (![IO.Path]::IsPathRooted($folder)) { continue }
            if (![IO.File]::Exists([IO.Path]::Combine($folder, 'coucou.exe'))) { continue }
            (Get-Item -LiteralPath $folder -ErrorAction Stop).FullName
        } catch { continue }
    }) | Sort-Object -Unique)
    if ($found.Count -ne 1) { throw 'Nao foi possivel escolher a instalacao. Execute com -InstallDir "C:\caminho\do\Coucou".' }
    $InstallDir = $found[0]
}
$InstallDir = (Resolve-Path -LiteralPath $InstallDir).Path
$exe = Join-Path $InstallDir 'coucou.exe'
if (!(Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'A pasta indicada nao contem coucou.exe.' }
if (@(Get-Process -Name coucou -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }).Count) {
    Write-Host 'Feche o Coucou pelo icone perto do relogio (Sair/Quit).'
    Read-Host 'Depois pressione Enter para continuar' | Out-Null
    if (@(Get-Process -Name coucou -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }).Count) { throw 'O Coucou ainda esta aberto. Nenhum arquivo foi alterado.' }
}
$stamp = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$backup = Join-Path $InstallDir ('.coucou-hotfix-backups\' + $stamp)
$stage = Join-Path $InstallDir ('.coucou-hotfix-stage-' + $stamp)
New-Item -ItemType Directory -Path $backup, $stage -Force | Out-Null
$changed = @()
$existed = @{}
try {
    # Stage and back up every file before replacing anything. User data is outside this list.
    foreach ($file in $manifest.files) {
        $target = Join-Path $InstallDir $file.name
        if ((Test-Path -LiteralPath $target) -and !(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Destino nao e um arquivo: $($file.name)" }
        $existed[$file.name] = Test-Path -LiteralPath $target
        if ($existed[$file.name]) { Copy-Item -LiteralPath $target -Destination (Join-Path $backup $file.name) }
        Copy-Item -LiteralPath (Join-Path (Join-Path $PSScriptRoot 'payload') $file.name) -Destination (Join-Path $stage $file.name)
        if ((Get-FileHash -LiteralPath (Join-Path $stage $file.name) -Algorithm SHA256).Hash -ne $file.sha256) { throw 'Falha na verificacao dos arquivos preparados.' }
    }
    foreach ($file in $manifest.files) {
        $changed += $file.name
        Copy-Item -LiteralPath (Join-Path $stage $file.name) -Destination (Join-Path $InstallDir $file.name) -Force
    }
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $backup 'manifest-aplicado.json')
    Write-Host "Hotfix $($manifest.revision) aplicado. Backup: $backup"
    Write-Host 'Abra o Coucou novamente pelo menu Iniciar.'
} catch {
    $original = $_
    $rollbackErrors = @()
    foreach ($name in $changed) {
        try {
            if ($existed[$name]) { Copy-Item -LiteralPath (Join-Path $backup $name) -Destination (Join-Path $InstallDir $name) -Force }
            else { Remove-Item -LiteralPath (Join-Path $InstallDir $name) -Force -ErrorAction SilentlyContinue }
        } catch { $rollbackErrors += $name }
    }
    if ($rollbackErrors.Count) { Write-Warning "Restaure manualmente do backup: $backup. Arquivos: $($rollbackErrors -join ', ')" }
    throw $original
} finally {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}
