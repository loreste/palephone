$ErrorActionPreference = "Stop"

$installDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$programData = Join-Path $env:ProgramData "Pale Server"
$envPath = Join-Path $programData "pale-server.env"
$logDir = Join-Path $programData "logs"
$logPath = Join-Path $logDir "pale-server.log"
$exePath = Join-Path $installDir "pale-server.exe"

New-Item -ItemType Directory -Force -Path $logDir | Out-Null

if (-not (Test-Path $envPath)) {
    throw "Missing Pale Server environment file: $envPath. Run Configure Pale Server first."
}

Get-Content $envPath | ForEach-Object {
    $line = $_.Trim()
    if ($line.Length -eq 0 -or $line.StartsWith("#")) {
        return
    }

    $parts = $line -split "=", 2
    if ($parts.Count -eq 2) {
        [Environment]::SetEnvironmentVariable(($parts[0]).Trim(), $parts[1], "Process")
    }
}

"[$(Get-Date -Format o)] Starting Pale Server from $exePath" | Add-Content -Path $logPath
& $exePath *>> $logPath
