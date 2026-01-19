<#
.SYNOPSIS
    Setup script for running 7-Agent Assistant on Docker (Windows)

.DESCRIPTION
    This script sets up the full-stack Docker environment for integration testing:
    - Verifies prerequisites (Docker, NVIDIA GPU)
    - Downloads the LLM model to Docker volume
    - Builds and starts all services
    - Runs integration tests

.PARAMETER SkipModelDownload
    Skip downloading the LLM model (use if already downloaded)

.PARAMETER SkipTests
    Skip running integration tests after setup

.PARAMETER Model
    Model to download. Options: 'qwen-3b' (default, 2GB), 'llama-3.2-3b' (2GB), 'qwen-7b' (4.4GB)

.PARAMETER GpuLayers
    Number of GPU layers (default: 35, use 0 for CPU-only)

.PARAMETER CleanModels
    Remove ALL existing models and download fresh

.PARAMETER ForceDownload
    Force re-download even if model exists

.EXAMPLE
    .\setup-docker-windows.ps1

.EXAMPLE
    .\setup-docker-windows.ps1 -SkipModelDownload -SkipTests

.EXAMPLE
    .\setup-docker-windows.ps1 -CleanModels -ForceDownload

.NOTES
    Prerequisites:
    - Windows 10 21H2+ or Windows 11
    - Docker Desktop with WSL2 backend
    - NVIDIA GPU with driver 535.54+ (optional, falls back to CPU)
#>

param(
    [switch]$SkipModelDownload,
    [switch]$SkipTests,
    [ValidateSet('qwen-3b', 'llama-3.2-3b', 'qwen-7b')]
    [string]$Model = 'qwen-3b',
    [int]$GpuLayers = 35,
    [switch]$CleanModels,
    [switch]$ForceDownload
)

$ErrorActionPreference = "Continue"
$ProgressPreference = 'SilentlyContinue'

# Colors for output
function Write-Step { param($msg) Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Success { param($msg) Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Warn { param($msg) Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Write-Err { param($msg) Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Write-Info { param($msg) Write-Host "    $msg" -ForegroundColor Gray }

# Model configurations - use EXACT HuggingFace filenames for consistency
# The filename matches the URL exactly (no case conversion)
$Models = @{
    'qwen-3b' = @{
        Url = 'https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf'
        Filename = 'qwen2.5-3b-instruct-q4_k_m.gguf'  # Exact HuggingFace name (Qwen official repo)
        MinSize = 1900000000
        Size = '2GB'
    }
    'llama-3.2-3b' = @{
        Url = 'https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf'
        Filename = 'Llama-3.2-3B-Instruct-Q4_K_M.gguf'  # Exact HuggingFace name
        MinSize = 1900000000
        Size = '2GB'
    }
    'qwen-7b' = @{
        Url = 'https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf'
        Filename = 'Qwen2.5-7B-Instruct-Q4_K_M.gguf'  # Exact HuggingFace name (bartowski repo)
        MinSize = 4000000000
        Size = '4.68GB'
    }
}

$SelectedModel = $Models[$Model]
$ModelFilename = $SelectedModel.Filename

Write-Host @"

================================================================================
  7-Agent Assistant - Docker Setup for Windows
================================================================================
  Model: $Model ($($SelectedModel.Size))
  GPU Layers: $GpuLayers (0 = CPU only)
================================================================================

"@ -ForegroundColor Magenta

# -----------------------------------------------------------------------------
# Step 1: Verify Prerequisites
# -----------------------------------------------------------------------------
Write-Step "Verifying prerequisites..."

# Check Docker
try {
    $dockerVersion = docker --version 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Docker not found" }
    Write-Success "Docker installed: $dockerVersion"
} catch {
    Write-Err "Docker not found. Please install Docker Desktop."
    Write-Info "Download: https://www.docker.com/products/docker-desktop/"
    exit 1
}

# Check Docker is running
$null = docker info 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Err "Docker is not running. Please start Docker Desktop."
    exit 1
}
Write-Success "Docker is running"

# Pre-pull required images
Write-Info "Pulling required Docker images..."
$null = docker pull alpine:latest 2>&1
$null = docker pull busybox:latest 2>&1
Write-Info "  Images ready"

# Check NVIDIA GPU (optional)
$hasGpu = $false
try {
    $nvidiaSmi = nvidia-smi --query-gpu=name --format=csv,noheader 2>$null
    if ($nvidiaSmi -and $LASTEXITCODE -eq 0) {
        Write-Success "NVIDIA GPU detected: $($nvidiaSmi.Trim())"
        $hasGpu = $true
    }
} catch { }

if (-not $hasGpu) {
    Write-Warn "No NVIDIA GPU detected. Will use CPU-only mode."
    $GpuLayers = 0
}

# -----------------------------------------------------------------------------
# Step 2: Stop existing services
# -----------------------------------------------------------------------------
Write-Step "Stopping existing services..."
$null = docker compose down 2>&1
Write-Success "Services stopped"

# -----------------------------------------------------------------------------
# Step 3: Setup Docker Volume for Models
# -----------------------------------------------------------------------------
Write-Step "Setting up models volume..."

# Create volume if needed
$null = docker volume create llama-models 2>&1

# Clean models if requested
if ($CleanModels) {
    Write-Info "Cleaning all existing models..."
    $null = docker run --rm -v llama-models:/models alpine rm -rf /models/* 2>&1
    Write-Success "Models volume cleaned"
}

# -----------------------------------------------------------------------------
# Step 4: Check/Download Model
# -----------------------------------------------------------------------------
Write-Step "Checking model..."

# Check if model exists with EXACT filename
$modelValid = $false

# Show what's in the volume
Write-Info "Current volume contents:"
docker run --rm -v llama-models:/models alpine ls -la /models/ 2>&1 | ForEach-Object { Write-Info "  $_" }

$checkOutput = docker run --rm -v llama-models:/models alpine stat -c '%s' "/models/$ModelFilename" 2>&1
if ($LASTEXITCODE -eq 0) {
    $modelSize = [long]($checkOutput.Trim())
    if ($modelSize -ge $SelectedModel.MinSize) {
        $modelValid = $true
        Write-Success "Valid model found: $ModelFilename ($([math]::Round($modelSize/1GB, 2)) GB)"
    } else {
        Write-Warn "Model exists but appears corrupt ($modelSize bytes). Will re-download."
        $null = docker run --rm -v llama-models:/models alpine rm -f "/models/$ModelFilename" 2>&1
    }
} else {
    Write-Info "Expected model not found: $ModelFilename"
}

# ALWAYS clean up orphaned .gguf files (wrong case, old models, etc) - even if valid model found
Write-Info "Cleaning up orphaned model files..."
$null = docker run --rm -v llama-models:/models alpine sh -c "find /models -name '*.gguf' ! -name '$ModelFilename' -delete" 2>&1

# Download if needed
if ((-not $modelValid -or $ForceDownload) -and -not $SkipModelDownload) {
    Write-Step "Downloading model: $Model ($($SelectedModel.Size))..."
    Write-Info "URL: $($SelectedModel.Url)"
    Write-Info "This may take several minutes..."

    # ALWAYS clean up ALL .gguf files before downloading (safety - removes wrong case, old models, etc)
    Write-Info "Cleaning up any existing model files..."
    $null = docker run --rm -v llama-models:/models alpine sh -c "rm -f /models/*.gguf" 2>&1

    # Download with busybox wget
    docker run --rm -v llama-models:/models busybox wget -O "/models/$ModelFilename" "$($SelectedModel.Url)"

    if ($LASTEXITCODE -eq 0) {
        # Verify download
        $verifyOutput = docker run --rm -v llama-models:/models alpine stat -c '%s' /models/$ModelFilename 2>&1
        if ($LASTEXITCODE -eq 0) {
            $downloadedSize = [long]($verifyOutput.Trim())
            if ($downloadedSize -ge $SelectedModel.MinSize) {
                Write-Success "Model downloaded successfully ($([math]::Round($downloadedSize/1GB, 2)) GB)"
            } else {
                Write-Err "Download incomplete ($downloadedSize bytes). Please try again."
                exit 1
            }
        } else {
            Write-Err "Could not verify download. Please try again."
            exit 1
        }
    } else {
        Write-Err "Download failed. Try manual download:"
        Write-Info "  curl.exe -L -o model.gguf '$($SelectedModel.Url)'"
        Write-Info "  docker run --rm -v llama-models:/models -v `${PWD}:/src alpine cp /src/model.gguf /models/"
        exit 1
    }
} elseif ($SkipModelDownload -and -not $modelValid) {
    Write-Warn "Model not found and -SkipModelDownload specified"
    Write-Warn "Services may fail to start!"
}

# Verify final state
Write-Info "Verifying models volume contents..."
docker run --rm -v llama-models:/models alpine ls -la /models/

# -----------------------------------------------------------------------------
# Step 5: Update Environment Configuration
# -----------------------------------------------------------------------------
Write-Step "Updating environment configuration..."

$envFile = ".env"
if (Test-Path $envFile) {
    $envContent = Get-Content $envFile -Raw
    $modified = $false

    # Update LLAMA_MODEL to use our canonical name
    if ($envContent -notmatch "LLAMA_MODEL=$ModelFilename") {
        $envContent = $envContent -replace 'LLAMA_MODEL=.*', "LLAMA_MODEL=$ModelFilename"
        $modified = $true
    }

    # Update LLAMA_GPU_LAYERS
    $currentGpuLayers = if ($envContent -match 'LLAMA_GPU_LAYERS=(\d+)') { [int]$Matches[1] } else { -1 }
    if ($currentGpuLayers -ne $GpuLayers) {
        $envContent = $envContent -replace 'LLAMA_GPU_LAYERS=.*', "LLAMA_GPU_LAYERS=$GpuLayers"
        $modified = $true
    }

    if ($modified) {
        Set-Content $envFile $envContent -NoNewline
        Write-Success "Updated .env: model=$ModelFilename, gpu_layers=$GpuLayers"
    } else {
        Write-Success ".env already configured correctly"
    }
} else {
    Write-Warn ".env file not found. Creating minimal config..."
    # CRITICAL: LLAMA_PARALLEL=1 ensures full context per request
    # With parallel>1, context is divided: 8192/4=2048 tokens per slot (causes truncation!)
    @"
LLAMA_MODEL=$ModelFilename
LLAMA_GPU_LAYERS=$GpuLayers
LLAMA_CTX_SIZE=8192
LLAMA_PARALLEL=1
"@ | Set-Content $envFile
    Write-Success "Created .env file"
}

# Also update docker-compose default if needed
$composeFile = "docker-compose.yml"
if (Test-Path $composeFile) {
    $composeContent = Get-Content $composeFile -Raw
    if ($composeContent -notmatch "\`${LLAMA_MODEL:-$ModelFilename}") {
        $composeContent = $composeContent -replace '\$\{LLAMA_MODEL:-[^}]+\}', "`${LLAMA_MODEL:-$ModelFilename}"
        Set-Content $composeFile $composeContent -NoNewline
        Write-Info "Updated docker-compose.yml default model"
    }
}

# -----------------------------------------------------------------------------
# Step 6: Build Docker Images
# -----------------------------------------------------------------------------
Write-Step "Building Docker images..."
Write-Info "This may take a few minutes on first run..."

# Run build and show output directly (don't filter - user needs to see progress)
docker compose build

if ($LASTEXITCODE -ne 0) {
    Write-Err "Docker build failed"
    exit 1
}
Write-Success "Docker images built"

# -----------------------------------------------------------------------------
# Step 7: Start Services
# -----------------------------------------------------------------------------
Write-Step "Starting services..."

docker compose up -d

Write-Info "Waiting for services to become healthy..."
Write-Info "(llama-server may take 1-2 minutes to load the model)"

# Wait for llama-server specifically
$timeout = 180
$elapsed = 0
$interval = 10
$llamaHealthy = $false

while ($elapsed -lt $timeout) {
    Start-Sleep -Seconds $interval
    $elapsed += $interval

    # Check llama-server health
    try {
        $null = Invoke-RestMethod -Uri "http://localhost:8080/health" -TimeoutSec 5 -ErrorAction Stop
        $llamaHealthy = $true
        break
    } catch {
        # Check if container is running and show progress
        $status = docker compose ps llama-server --format json 2>$null | ConvertFrom-Json -ErrorAction SilentlyContinue
        if ($status) {
            Write-Info "llama-server status: $($status.State) ($elapsed/$timeout sec)"
        } else {
            Write-Info "Waiting for llama-server... ($elapsed/$timeout sec)"
        }
    }
}

if ($llamaHealthy) {
    Write-Success "llama-server is healthy!"
} else {
    Write-Warn "llama-server may still be starting. Check logs:"
    Write-Info "  docker compose logs llama-server"
}

# Check PostgreSQL
$pgHealthy = $false
$pgCheck = docker compose exec -T postgres pg_isready -U assistant 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Success "PostgreSQL is healthy!"
    $pgHealthy = $true
} else {
    Write-Warn "PostgreSQL may still be starting"
}

# Check API
Start-Sleep -Seconds 3
$apiHealthy = $false
try {
    $null = Invoke-RestMethod -Uri "http://localhost:8000/health" -TimeoutSec 5 -ErrorAction Stop
    Write-Success "API is healthy!"
    $apiHealthy = $true
} catch {
    Write-Warn "API may still be starting"
}

# Service status summary
Write-Info ""
Write-Info "Service Status Summary:"
Write-Info "  llama-server: $(if ($llamaHealthy) { 'Ready' } else { 'Starting...' })"
Write-Info "  PostgreSQL:   $(if ($pgHealthy) { 'Ready' } else { 'Starting...' })"
Write-Info "  API:          $(if ($apiHealthy) { 'Ready' } else { 'Starting...' })"

# Start dependent services now that llama-server should be up
docker compose up -d 2>&1 | Out-Null

# -----------------------------------------------------------------------------
# Step 8: Verify GPU Usage
# -----------------------------------------------------------------------------
if ($hasGpu -and $GpuLayers -gt 0 -and $llamaHealthy) {
    Write-Step "Verifying GPU usage..."
    $logs = docker compose logs llama-server 2>&1 | Select-String -Pattern "CUDA|GPU|cuda|offload|VRAM"
    if ($logs) {
        Write-Success "GPU acceleration active:"
        $logs | Select-Object -First 3 | ForEach-Object { Write-Info $_.Line.Trim() }
    }
}

# -----------------------------------------------------------------------------
# Step 9: Run Tests
# -----------------------------------------------------------------------------
$allServicesReady = $llamaHealthy -and $pgHealthy -and $apiHealthy

if (-not $SkipTests -and $allServicesReady) {
    Write-Step "Running integration tests..."
    Write-Info "Note: LLM-dependent tests may vary with small models (3B)"
    docker compose run --rm test pytest -v --tb=short
    if ($LASTEXITCODE -eq 0) {
        Write-Success "All tests passed!"
    } else {
        Write-Warn "Some tests failed (check output above)"
        Write-Info "LLM-dependent tests may fail with small models - this is expected"
    }
} elseif (-not $SkipTests) {
    Write-Warn "Skipping tests - services not ready"
    Write-Info "Run manually: docker compose run --rm test pytest -v --tb=short"
}

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
$gpuStatus = if ($hasGpu -and $GpuLayers -gt 0) { "Enabled ($GpuLayers layers)" } else { "Disabled (CPU mode)" }

Write-Host @"

================================================================================
  Setup Complete!
================================================================================

Services:
  - API:          http://localhost:8000 $(if ($apiHealthy) { "[healthy]" } else { "[starting]" })
  - llama-server: http://localhost:8080 $(if ($llamaHealthy) { "[healthy]" } else { "[starting]" })
  - PostgreSQL:   localhost:5432 $(if ($pgHealthy) { "[healthy]" } else { "[starting]" })

GPU Status: $gpuStatus
Model: $ModelFilename ($Model)

Commands:
  docker compose ps                          # Check status
  docker compose logs -f llama-server        # View logs
  docker compose run --rm test pytest -v     # Run tests
  docker compose down                        # Stop services

Troubleshooting:
  .\setup-docker-windows.ps1 -CleanModels    # Clean and re-download model
  .\setup-docker-windows.ps1 -ForceDownload  # Force re-download

================================================================================
"@ -ForegroundColor Green
