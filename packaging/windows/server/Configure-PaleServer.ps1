[CmdletBinding()]
param(
    [string]$HttpAddr = "127.0.0.1:8080",
    [string]$AdminUsername = "admin",
    [string]$AdminPassword,
    [string]$DatabaseUrl = "",
    [ValidateSet("off", "udp-parser", "pjsip")]
    [string]$SipBackend = "udp-parser",
    [string]$SipExternalAddr = "",
    [string]$TurnServer = "",
    [switch]$SkipServiceStart
)

$ErrorActionPreference = "Stop"

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function New-PaleSecret {
    $bytes = [byte[]]::new(32)
    [Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
    [Convert]::ToBase64String($bytes)
}

function ConvertTo-PlainText {
    param([Security.SecureString]$SecureString)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SecureString)
    try {
        [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
}

function Write-PaleEnvFile {
    param(
        [string]$Path,
        [hashtable]$Values
    )

    $lines = @(
        "# Pale Server local configuration"
        "# Generated on $(Get-Date -Format o)"
        "# This file contains local secrets. Do not publish it."
    )

    foreach ($key in ($Values.Keys | Sort-Object)) {
        $value = [string]$Values[$key]
        $lines += "$key=$value"
    }

    $directory = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    Set-Content -Path $Path -Value $lines -Encoding UTF8
}

function Install-PaleService {
    param(
        [string]$InstallDir,
        [string]$ServiceName,
        [string]$LogDir
    )

    $serviceExe = Join-Path $InstallDir "PaleServerService.exe"
    if (-not (Test-Path $serviceExe)) {
        throw "Missing Windows service wrapper: $serviceExe"
    }

    $powershell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
    $runner = Join-Path $InstallDir "Run-PaleServer.ps1"
    $serviceXml = Join-Path $InstallDir "PaleServerService.xml"
    $escapedPowershell = [System.Security.SecurityElement]::Escape($powershell)
    $escapedRunner = [System.Security.SecurityElement]::Escape($runner)
    $escapedLogDir = [System.Security.SecurityElement]::Escape($LogDir)
    $escapedInstallDir = [System.Security.SecurityElement]::Escape($InstallDir)
    $xml = @"
<service>
  <id>PaleServer</id>
  <name>Pale Server</name>
  <description>Self-hosted Pale communications and PBX backend</description>
  <executable>$escapedPowershell</executable>
  <arguments>-NoProfile -ExecutionPolicy Bypass -File "$escapedRunner"</arguments>
  <workingdirectory>$escapedInstallDir</workingdirectory>
  <logpath>$escapedLogDir</logpath>
  <log mode="roll-by-size">
    <sizeThreshold>10485760</sizeThreshold>
    <keepFiles>5</keepFiles>
  </log>
  <onfailure action="restart" delay="10 sec" />
</service>
"@
    Set-Content -Path $serviceXml -Value $xml -Encoding UTF8

    $existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if ($existing) {
        & $serviceExe stop | Out-Null
        & $serviceExe uninstall | Out-Null
    }

    & $serviceExe install | Out-Null
}

if (-not (Test-Administrator)) {
    Start-Process powershell.exe -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    exit
}

$installDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$programData = Join-Path $env:ProgramData "Pale Server"
$envPath = Join-Path $programData "pale-server.env"
$dataDir = Join-Path $programData "data"
$logsDir = Join-Path $programData "logs"

New-Item -ItemType Directory -Force -Path $programData, $dataDir, $logsDir | Out-Null

if ([string]::IsNullOrWhiteSpace($AdminPassword)) {
    $securePassword = Read-Host "Admin password for Pale Server" -AsSecureString
    $AdminPassword = ConvertTo-PlainText -SecureString $securePassword
}

if ([string]::IsNullOrWhiteSpace($AdminPassword) -or $AdminPassword.Length -lt 24) {
    throw "Admin password must be at least 24 characters."
}

# "off" maps to pjsip on no-native builds (no registrar). Prefer udp-parser for production calling.
$sipBackendValue = if ($SipBackend -eq "off") { "pjsip" } else { $SipBackend }

$values = @{
    "PALE_ADMIN_USERNAME" = $AdminUsername
    "PALE_ADMIN_PASSWORD" = $AdminPassword
    "PALE_DATA_DIR" = $dataDir
    "PALE_HTTP_ADDR" = $HttpAddr
    "PALE_LOG_JSON" = "true"
    "PALE_SERVER_TOKEN" = New-PaleSecret
    "PALE_SIP_BACKEND" = $sipBackendValue
    "PALE_SIP_TCP" = "true"
    "PALE_SIP_UDP" = "false"
    "PALE_SIP_SRTP" = "true"
    "PALE_STORAGE_KEY" = New-PaleSecret
    "RUST_LOG" = "info"
}

if (-not [string]::IsNullOrWhiteSpace($DatabaseUrl)) {
    $values["PALE_DATABASE_URL"] = $DatabaseUrl
}

if (-not [string]::IsNullOrWhiteSpace($SipExternalAddr)) {
    $values["PALE_SIP_EXTERNAL_ADDR"] = $SipExternalAddr
}

if (-not [string]::IsNullOrWhiteSpace($TurnServer)) {
    $values["PALE_TURN_SERVER"] = $TurnServer
}

Write-PaleEnvFile -Path $envPath -Values $values
icacls $programData /inheritance:r /grant:r "Administrators:(OI)(CI)F" "SYSTEM:(OI)(CI)F" | Out-Null
Install-PaleService -InstallDir $installDir -ServiceName "PaleServer" -LogDir $logsDir

if ($HttpAddr -notmatch "^(127\.0\.0\.1|localhost):") {
    $port = ($HttpAddr -split ":")[-1]
    if ($port -match "^\d+$" -and -not (Get-NetFirewallRule -DisplayName "Pale Server HTTP" -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -DisplayName "Pale Server HTTP" -Direction Inbound -Protocol TCP -LocalPort $port -Action Allow | Out-Null
    }
}

if (-not $SkipServiceStart) {
    Start-Service -Name PaleServer
    Start-Sleep -Seconds 3

    $healthPort = ($HttpAddr -split ":")[-1]
    $healthUrl = "http://127.0.0.1:$healthPort/health"

    try {
        $response = Invoke-WebRequest -UseBasicParsing -Uri $healthUrl -TimeoutSec 10
        Write-Host "Pale Server is running: $($response.Content)"
    } catch {
        Write-Warning "Service was installed, but the health check did not answer yet. Check $logsDir\pale-server.log and Windows Event Viewer."
    }
}

Write-Host "Configuration written to $envPath"
Write-Host "Service name: PaleServer"
Write-Host "SIP backend: $sipBackendValue (use udp-parser for built-in registrar)"
Write-Host "Production: terminate TLS in front of $HttpAddr; set SIP TLS certs and TURN. See docs/deploy/windows.md"
