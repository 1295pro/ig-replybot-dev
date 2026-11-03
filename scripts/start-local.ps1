$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $project 'target\debug\ig-reply-bot.exe'
$url = 'http://127.0.0.1:3000/'
Set-Location -LiteralPath $project

if (-not (Test-Path -LiteralPath '.env.local' -PathType Leaf)) {
    throw 'No .env.local found. Copy .env.example to .env.local and set IG_ADMIN_PASSWORD first.'
}

$listeners = @(Get-NetTCPConnection -LocalPort 3000 -State Listen -ErrorAction SilentlyContinue)
if ($listeners.Count -gt 0) {
    $ownerId = $listeners[0].OwningProcess
    $ownerPath = (Get-Process -Id $ownerId -ErrorAction Stop).Path
    if (-not [string]::Equals($ownerPath, $exe, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Port 3000 is already used by process $ownerId. Close that program before starting this bot."
    }
    Write-Host "Local bot is already running (process $ownerId). Opening $url"
    Start-Process $url
    exit 0
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Rust/Cargo was not found. Install Rust from https://rustup.rs/ and open a new terminal.'
}

Write-Host 'Building the current local version...'
& cargo build --locked
if ($LASTEXITCODE -ne 0) {
    throw 'Build failed. See the Cargo error above.'
}

Write-Host 'Starting IG Reply Bot on this computer...'
$bot = Start-Process -FilePath $exe -WorkingDirectory $project -NoNewWindow -PassThru
try {
    $ready = $false
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        if ($bot.HasExited) {
            throw "Bot exited during startup (code $($bot.ExitCode)). See the error above."
        }
        try {
            $response = Invoke-WebRequest -Uri ($url + 'healthz') -UseBasicParsing -TimeoutSec 1
            if ($response.StatusCode -eq 200) {
                $ready = $true
                break
            }
        } catch {
            Start-Sleep -Milliseconds 250
        }
    }
    if (-not $ready) {
        throw 'The bot did not answer on http://127.0.0.1:3000/ within 10 seconds.'
    }

    Start-Process $url
    Write-Host "Control room: $url"
    Write-Host 'Keep this window open while testing. Press Ctrl+C here to stop the local bot.'
    Wait-Process -Id $bot.Id
} finally {
    $bot.Refresh()
    if (-not $bot.HasExited) {
        Stop-Process -Id $bot.Id -Force -ErrorAction SilentlyContinue
    }
}
