# -------------------------------
# Ollama full setup script
# Run as Administrator
# Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
# -------------------------------

# Ensure script is running as Administrator
if (-not ([Security.Principal.WindowsPrincipal] `
    [Security.Principal.WindowsIdentity]::GetCurrent()
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error "This script must be run as Administrator."
    exit 1
}

Write-Host "=== Configuring Ollama ===" -ForegroundColor Cyan

# 1. Open firewall for Ollama (TCP 11434)
$ruleName = "Ollama API 11434"
if (-not (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue)) {
    Write-Host "Opening Windows Firewall port 11434..."
    New-NetFirewallRule `
        -DisplayName $ruleName `
        -Direction Inbound `
        -Protocol TCP `
        -LocalPort 11434 `
        -Action Allow `
        -Profile Private,Domain
} else {
    Write-Host "Firewall rule already exists."
}

# 2. Set environment variables (persistent, user scope)
Write-Host "Setting environment variables..."

[Environment]::SetEnvironmentVariable(
    "OLLAMA_API",
    "openai",
    [EnvironmentVariableTarget]::User
)

[Environment]::SetEnvironmentVariable(
    "OLLAMA_HOST",
    "0.0.0.0:11434",
    [EnvironmentVariableTarget]::User
)

# 3. Install Ollama via winget (if not already installed)
if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Ollama via winget..."
    winget install -e --id Ollama.Ollama
} else {
    Write-Host "Ollama is already installed."
}

# 4. Start Ollama server with correct environment
Write-Host "Starting Ollama server..."
$env:OLLAMA_API  = "openai"
$env:OLLAMA_HOST = "0.0.0.0:11434"

$ollamaProcess = Start-Process -NoNewWindow -FilePath "ollama" -ArgumentList "serve" -PassThru

Write-Host "✅ Ollama is now running on http://0.0.0.0:11434" -ForegroundColor Green
Write-Host "✅ OpenAI-compatible API available at /v1/*"

# Give the server a moment to come up
Start-Sleep -Seconds 3

# 5. Pull required models
Write-Host "Pulling models..."
ollama pull phi3:latest
ollama pull phi3.5:latest
ollama pull llama3.1:8b

Write-Host ""
Write-Host "Press Ctrl+C to exit and stop Ollama..." -ForegroundColor Yellow

try {
    while ($true) {
        Start-Sleep -Seconds 1
    }
} catch [System.Management.Automation.PipelineStoppedException] {
    Write-Host "`nCtrl+C pressed. Shutting down Ollama..." -ForegroundColor Yellow
} finally {
    if ($ollamaProcess -and -not $ollamaProcess.HasExited) {
        Stop-Process -Id $ollamaProcess.Id -Force
        Write-Host "Ollama stopped." -ForegroundColor Green
    }
}