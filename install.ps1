# mcp-hub installer for Windows PowerShell
# Usage: iwr -useb https://raw.githubusercontent.com/jiale-cheng-ning/mcp-hub/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$REPO = "jiale-cheng-ning/mcp-hub"
$BINARY = "mcp-hub.exe"
$INSTALL_DIR = "$env:USERPROFILE\.local\bin"

Write-Host "Downloading mcp-hub for Windows..." -ForegroundColor Cyan

# Create install directory
if (-not (Test-Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
}

# Get latest release URL
$URL = "https://github.com/$REPO/releases/latest/download/mcp-hub-windows-amd64.exe"
$DEST = Join-Path $INSTALL_DIR $BINARY

try {
    Invoke-WebRequest -Uri $URL -OutFile $DEST -UseBasicParsing
} catch {
    Write-Host "Download failed: $_" -ForegroundColor Red
    Write-Host "Download manually from: https://github.com/$REPO/releases" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "Installed mcp-hub to $DEST" -ForegroundColor Green
Write-Host ""

# Check if in PATH
$PATH_DIRS = $env:PATH -split ";"
if ($PATH_DIRS -contains $INSTALL_DIR) {
    Write-Host "Run 'mcp-hub --help' to get started." -ForegroundColor Cyan
} else {
    Write-Host "NOTE: Add $INSTALL_DIR to your PATH:" -ForegroundColor Yellow
    Write-Host '  [Environment]::SetEnvironmentVariable("Path", $env:Path + ";' + $INSTALL_DIR + '", "User")'
    Write-Host ""
    Write-Host "Then restart your terminal and run 'mcp-hub --help'." -ForegroundColor Cyan
}
