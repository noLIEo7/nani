# Installs nani on Windows: builds it with Rust if available, otherwise downloads the
# latest release binary with the GitHub CLI (gh auth login first).
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$dest = Join-Path $env:LOCALAPPDATA "Programs\nani"
New-Item -ItemType Directory -Force $dest | Out-Null

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    cargo build --release
    Copy-Item target\release\nani.exe $dest -Force
} elseif (Get-Command gh -ErrorAction SilentlyContinue) {
    $zip = Join-Path $env:TEMP "nani-windows-x86_64.zip"
    gh release download -p nani-windows-x86_64.zip -O $zip --clobber
    Expand-Archive -Force $zip $dest
} else {
    Write-Error "Need either Rust (cargo) or the GitHub CLI (gh). See README.md."
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dest", "User")
    Write-Host "Added $dest to your PATH - open a new terminal."
}
Write-Host "Installed: $dest\nani.exe"
