# start-public.ps1
# Shares your local Community app through a temporary public https URL (Cloudflare quick tunnel).
# 1) Start the app first:   cargo run        (in D:\Community, leave it running)
# 2) In a second PowerShell: powershell -ExecutionPolicy Bypass -File .\start-public.ps1
# Look for the line with https://<random-words>.trycloudflare.com and open it.
# Press Ctrl+C to stop sharing. The URL changes every time you start it.

param([int]$Port = 3000)
$ErrorActionPreference = 'Stop'

# 1. The app must already be running locally.
try {
  Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/" -TimeoutSec 5 | Out-Null
} catch {
  throw "Nothing is answering on http://127.0.0.1:$Port/. Start the app with 'cargo run' in D:\Community first, then run this script again."
}

# 2. Find cloudflared, installing it with winget if it is missing.
function Find-Cloudflared {
  $cmd = Get-Command cloudflared -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  foreach ($path in @(
      'C:\Program Files (x86)\cloudflared\cloudflared.exe',
      'C:\Program Files\cloudflared\cloudflared.exe',
      (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links\cloudflared.exe'))) {
    if (Test-Path $path) { return $path }
  }
  return $null
}

$cf = Find-Cloudflared
if (-not $cf) {
  Write-Host 'Installing cloudflared with winget...'
  winget install --id Cloudflare.cloudflared -e --accept-package-agreements --accept-source-agreements
  $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
  $cf = Find-Cloudflared
}
if (-not $cf) { throw 'cloudflared was installed but could not be found. Close this window, open a new PowerShell, and run the script again.' }

Write-Host ''
Write-Host 'Starting the public tunnel. Your public URL appears below (look for trycloudflare.com).'
Write-Host 'Anyone with that link can reach your app while this window stays open.'
Write-Host 'Tip: before sharing, close new signups from the admin settings, and press Ctrl+C when you are done.'
Write-Host ''
& $cf tunnel --url "http://127.0.0.1:$Port"
