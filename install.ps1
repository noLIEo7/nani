# Installs nani on Windows.
#
#   Without cloning:  irm https://raw.githubusercontent.com/noLIEo7/nani/main/install.ps1 | iex
#   From a clone:     powershell -ExecutionPolicy Bypass -File install.ps1
#
# Builds with Rust when run from a clone with cargo installed, otherwise downloads the latest
# release. Installs to %LOCALAPPDATA%\Programs\nani and adds that folder to your PATH.
& {
    $ErrorActionPreference = "Stop"
    $repo = "noLIEo7/nani"
    $dest = Join-Path $env:LOCALAPPDATA "Programs\nani"
    New-Item -ItemType Directory -Force $dest | Out-Null

    $src = $PSScriptRoot
    if ($src -and (Test-Path (Join-Path $src "Cargo.toml")) -and (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Push-Location $src
        try {
            cargo build --release --locked
            if ($LASTEXITCODE) { throw "cargo build failed" }
        } finally {
            Pop-Location
        }
        Copy-Item (Join-Path $src "target\release\nani.exe") $dest -Force
    } else {
        $url = "https://github.com/$repo/releases/latest/download/nani-windows-x86_64.zip"
        $zip = Join-Path $env:TEMP "nani-windows-x86_64.zip"
        Write-Host "Downloading $url"
        # Windows PowerShell 5.1 may not enable TLS 1.2 by default
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
        Expand-Archive -Force $zip $dest
        Remove-Item $zip
    }

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (($userPath -split ";") -notcontains $dest) {
        $newPath = if ($userPath) { $userPath.TrimEnd(";") + ";" + $dest } else { $dest }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "Added $dest to your PATH."
    }
    if (($env:Path -split ";") -notcontains $dest) {
        $env:Path += ";$dest"
    }
    Write-Host "Installed: $dest\nani.exe ($(& "$dest\nani.exe" --version))"
}
