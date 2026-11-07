$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $project
$exe = Join-Path $project 'ig-reply-bot.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    $exe = Join-Path $project 'target\release\ig-reply-bot.exe'
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'This is a source download. For the ready-to-run app, download the Windows ZIP from https://github.com/1295pro/ig-replybot-dev/releases/latest . To build from source, install Rust from https://rustup.rs/.'
    }
    $running = @(Get-Process -Name ig-reply-bot -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe })
    if ($running.Count -eq 0) {
        Write-Host 'Building IG Reply Bot. The first build can take a few minutes...'
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Build failed. Read the error above.' }
    }
}
Write-Host 'Opening your control room...'
Write-Host 'Keep this window open and your computer awake. Press Ctrl+C to stop the bot.'
& $exe --local
if ($LASTEXITCODE -ne 0) { throw "IG Reply Bot stopped (exit code $LASTEXITCODE). Read the error above." }
