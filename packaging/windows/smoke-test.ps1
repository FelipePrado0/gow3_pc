# Opens dist/spartanport-windows.zip as a player gets it, on a Windows without MSYS2: no game or player
# file ships, no Python file ships, spartanport-probe.exe finds every DLL it links, the Rust launcher
# answers --version and --voice-swap --help and its window stays open (.github/workflows/windows-zip.yml).
# Usage: powershell -File smoke-test.ps1 path\to\spartanport-windows.zip
param([Parameter(Mandatory)][string]$Package)
$ErrorActionPreference = 'Stop'
# A player's PATH: a DLL found only in MSYS2 or another toolchain must not hide a missing one.
$env:PATH = "$env:SystemRoot\System32;$env:SystemRoot;$env:SystemRoot\System32\WindowsPowerShell\v1.0"
$work = Join-Path ([IO.Path]::GetTempPath()) ("spartanport-smoke-" + [guid]::NewGuid())
Expand-Archive -LiteralPath $Package -DestinationPath $work
$tree = Join-Path $work 'spartanport-windows'
$failed = @()

# A tagged build must carry the stamp the launcher's version menu reads.
if ($env:SPARTAN_RELEASE_TAG) {
    $stamp = $null
    try { $stamp = Get-Content -Raw -LiteralPath (Join-Path $tree 'release.json') | ConvertFrom-Json } catch { }
    if ($stamp -and $stamp.version) {
        Write-Host "release.json: $($stamp.version)"
    } else {
        $msg = "release.json is missing or invalid in the package (tag $env:SPARTAN_RELEASE_TAG)"
        Write-Host "FAIL: $msg"
        $failed += $msg
    }
}

# The same names package.sh refuses to ship: game files and anything a player makes.
$forbidden = 'eboot.bin', 'sce_module', 'sce_sys', 'savedata', 'user', 'cache', '*.log', 'spartanport.ini',
    'settings.json', 'game_icon.ico', 'mods.json', 'patches.json', 'fsr4_shaders', 'nvngx_dlss*.dll', '*.spv', '*.pkg',
    '*.sprx', '*.prx'
$found = Get-ChildItem -LiteralPath $tree -Recurse -Force |
    Where-Object { $name = $_.Name; $forbidden | Where-Object { $name -like $_ } }
if ($found) { $failed += "files that must not ship: $($found.FullName -join ', ')" }

# A missing DLL stops the probe before main (0xC0000135).
$probe = Start-Process -FilePath (Join-Path $tree 'bin\spartanport-probe.exe') -ArgumentList '--cpu-features' `
    -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $work 'probe.log')
$null = $probe.Handle  # without it ExitCode stays empty after WaitForExit(timeout)
if (-not $probe.WaitForExit(60000)) {
    Stop-Process -Id $probe.Id -Force
    $probe.WaitForExit()
    $failed += 'spartanport-probe.exe --cpu-features did not finish in 60 s'
} elseif ($probe.ExitCode -ne 0) {
    $failed += ('spartanport-probe.exe --cpu-features exited 0x{0:X}' -f $probe.ExitCode)
}
Get-Content (Join-Path $work 'probe.log')

$launcher = Join-Path $tree 'SpartanPort.exe'
# --version prints "SpartanPort <version>" and exits; a launcher still running after 30 s is stopped.
$version = Start-Process -FilePath $launcher -ArgumentList '--version' -NoNewWindow -PassThru `
    -RedirectStandardOutput (Join-Path $work 'version.log')
$null = $version.Handle
if (-not $version.WaitForExit(30000)) {
    Stop-Process -Id $version.Id -Force
    $version.WaitForExit()
    $failed += 'SpartanPort.exe --version did not finish in 30 s'
} elseif ($version.ExitCode -ne 0) {
    $failed += ('SpartanPort.exe --version exited 0x{0:X}' -f $version.ExitCode)
} elseif ((Get-Content -Raw (Join-Path $work 'version.log')) -notmatch '^SpartanPort \d+\.\d+\.\d+') {
    $failed += 'SpartanPort.exe --version did not print "SpartanPort <version>"'
}
Get-Content (Join-Path $work 'version.log')

$voice = Start-Process -FilePath $launcher -ArgumentList '--voice-swap', '--help' -NoNewWindow -PassThru `
    -RedirectStandardOutput (Join-Path $work 'voice.log') -RedirectStandardError (Join-Path $work 'voice.err')
$null = $voice.Handle
if (-not $voice.WaitForExit(30000)) {
    Stop-Process -Id $voice.Id -Force
    $voice.WaitForExit()
    $failed += 'SpartanPort.exe --voice-swap --help did not finish in 30 s'
} elseif ($voice.ExitCode -ne 0) {
    $failed += ('SpartanPort.exe --voice-swap --help exited 0x{0:X}' -f $voice.ExitCode)
}
Get-Content (Join-Path $work 'voice.log'), (Join-Path $work 'voice.err')

# No Python ships: the launcher is native.
$python = Get-ChildItem -LiteralPath $tree -Recurse -Force -Filter '*.py'
if ($python) { $failed += "Python files in the package: $($python.FullName -join ', ')" }

$window = Start-Process -FilePath $launcher -PassThru
Start-Sleep -Seconds 15
if ($window.HasExited) {
    $failed += "the launcher exited ($($window.ExitCode)) instead of staying open"
} else {
    Stop-Process -Id $window.Id -Force
    $window.WaitForExit()
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failed) {
    $failed | ForEach-Object { Write-Host "FAIL: $_" }
    exit 1
}
Write-Host 'OK: the package opens without MSYS2'
