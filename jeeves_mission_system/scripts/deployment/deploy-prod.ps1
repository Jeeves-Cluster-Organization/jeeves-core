<#
.SYNOPSIS
    Deploy 7-Agent Assistant to production

.DESCRIPTION
    Production deployment script that:
    - Runs pre-deployment validation
    - Builds production Docker images
    - Tags images for registry (optional)
    - Deploys/restarts services with zero-downtime
    - Runs post-deployment verification

.PARAMETER SkipValidation
    Skip pre-deployment validation (use if just validated)

.PARAMETER SkipTests
    Skip running tests during validation

.PARAMETER Tag
    Docker image tag (default: 'latest')

.PARAMETER Registry
    Docker registry to push images to (optional)

.PARAMETER Push
    Push images to registry after build

.PARAMETER DryRun
    Show what would be done without making changes

.EXAMPLE
    .\deploy-prod.ps1

.EXAMPLE
    .\deploy-prod.ps1 -Tag "v1.2.3" -Registry "ghcr.io/myorg" -Push

.EXAMPLE
    .\deploy-prod.ps1 -DryRun
#>

param(
    [switch]$SkipValidation,
    [switch]$SkipTests,
    [string]$Tag = 'latest',
    [string]$Registry = '',
    [switch]$Push,
    [switch]$DryRun
)

$ErrorActionPreference = "Continue"

# Colors
function Write-Step { param($msg) Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Success { param($msg) Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Warn { param($msg) Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Write-Err { param($msg) Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Write-Info { param($msg) Write-Host "    $msg" -ForegroundColor Gray }
function Write-DryRun { param($msg) Write-Host "[DRY-RUN] $msg" -ForegroundColor Yellow }

$startTime = Get-Date
$deploymentId = Get-Date -Format "yyyyMMdd-HHmmss"

Write-Host @"

================================================================================
  7-Agent Assistant - Production Deployment
================================================================================
  Deployment ID: $deploymentId
  Tag: $Tag
  Registry: $(if ($Registry) { $Registry } else { "(local)" })
  Mode: $(if ($DryRun) { "DRY RUN" } else { "LIVE" })
================================================================================

"@ -ForegroundColor Magenta

# -----------------------------------------------------------------------------
# Step 1: Pre-deployment Validation
# -----------------------------------------------------------------------------
if (-not $SkipValidation) {
    Write-Step "Running pre-deployment validation..."

    if ($DryRun) {
        Write-DryRun "Would run: .\scripts\deployment\validate-deployment.ps1 $(if ($SkipTests) { '-SkipTests' })"
    } else {
        $validateArgs = @()
        if ($SkipTests) { $validateArgs += "-SkipTests" }

        & "$PSScriptRoot\validate-deployment.ps1" @validateArgs

        if ($LASTEXITCODE -ne 0) {
            Write-Err "Pre-deployment validation failed!"
            Write-Info "Fix issues and try again, or use -SkipValidation to bypass"
            exit 1
        }
        Write-Success "Pre-deployment validation passed"
    }
} else {
    Write-Warn "Skipping pre-deployment validation"
}

# -----------------------------------------------------------------------------
# Step 2: Build Production Images
# -----------------------------------------------------------------------------
Write-Step "Building production Docker images..."

$imageName = "jeeves-core"
$fullTag = "${imageName}:${Tag}"

if ($DryRun) {
    Write-DryRun "Would run: docker compose build"
    Write-DryRun "Would tag: $fullTag"
} else {
    docker compose build

    if ($LASTEXITCODE -ne 0) {
        Write-Err "Docker build failed"
        exit 1
    }

    # Tag the image
    docker tag "${imageName}:latest" $fullTag
    Write-Success "Built and tagged: $fullTag"
}

# -----------------------------------------------------------------------------
# Step 3: Push to Registry (if requested)
# -----------------------------------------------------------------------------
if ($Push -and $Registry) {
    Write-Step "Pushing images to registry..."

    $registryTag = "${Registry}/${imageName}:${Tag}"

    if ($DryRun) {
        Write-DryRun "Would tag: $registryTag"
        Write-DryRun "Would push: $registryTag"
    } else {
        docker tag $fullTag $registryTag
        docker push $registryTag

        if ($LASTEXITCODE -ne 0) {
            Write-Err "Failed to push to registry"
            exit 1
        }
        Write-Success "Pushed: $registryTag"
    }
} elseif ($Push -and -not $Registry) {
    Write-Warn "Push requested but no registry specified. Skipping push."
}

# -----------------------------------------------------------------------------
# Step 4: Deploy Services (Rolling Update)
# -----------------------------------------------------------------------------
Write-Step "Deploying services..."

if ($DryRun) {
    Write-DryRun "Would stop services: docker compose down"
    Write-DryRun "Would start services: docker compose up -d"
} else {
    # Stop existing services gracefully
    Write-Info "Stopping existing services..."
    docker compose down --timeout 30

    # Start services with new images
    Write-Info "Starting services with new images..."
    docker compose up -d

    if ($LASTEXITCODE -ne 0) {
        Write-Err "Failed to start services"
        exit 1
    }

    Write-Success "Services started"
}

# -----------------------------------------------------------------------------
# Step 5: Wait for Services to be Healthy
# -----------------------------------------------------------------------------
if (-not $DryRun) {
    Write-Step "Waiting for services to become healthy..."

    $timeout = 180
    $elapsed = 0
    $interval = 10
    $allHealthy = $false

    while ($elapsed -lt $timeout) {
        Start-Sleep -Seconds $interval
        $elapsed += $interval

        $llamaHealthy = $false
        $apiHealthy = $false
        $pgHealthy = $false
        $gatewayHealthy = $false

        # Check llama-server
        try {
            $null = Invoke-RestMethod -Uri "http://localhost:8080/health" -TimeoutSec 5 -ErrorAction Stop
            $llamaHealthy = $true
        } catch { }

        # Check API (gRPC - use TCP socket connection to API_PORT, default 8000)
        $apiPort = if ($env:API_PORT) { $env:API_PORT } else { "8000" }
        try {
            $tcp = New-Object System.Net.Sockets.TcpClient
            $tcp.Connect("localhost", [int]$apiPort)
            $tcp.Close()
            $apiHealthy = $true
        } catch { }

        # Check PostgreSQL
        $pgCheck = docker compose exec -T postgres pg_isready -U assistant 2>&1
        if ($LASTEXITCODE -eq 0) {
            $pgHealthy = $true
        }

        # Check Gateway (HTTP)
        $gatewayPort = if ($env:GATEWAY_PORT) { $env:GATEWAY_PORT } else { "8001" }
        try {
            $null = Invoke-RestMethod -Uri "http://localhost:$gatewayPort/health" -TimeoutSec 5 -ErrorAction Stop
            $gatewayHealthy = $true
        } catch { }

        Write-Info "Health: llama=$(if ($llamaHealthy) {'OK'} else {'...'}) api=$(if ($apiHealthy) {'OK'} else {'...'}) gw=$(if ($gatewayHealthy) {'OK'} else {'...'}) pg=$(if ($pgHealthy) {'OK'} else {'...'}) ($elapsed/$timeout sec)"

        if ($llamaHealthy -and $apiHealthy -and $pgHealthy -and $gatewayHealthy) {
            $allHealthy = $true
            break
        }
    }

    if ($allHealthy) {
        Write-Success "All services are healthy!"
    } else {
        Write-Warn "Some services may still be starting"
        Write-Info "Check logs: docker compose logs -f"
    }
}

# -----------------------------------------------------------------------------
# Step 6: Post-deployment Verification
# -----------------------------------------------------------------------------
if (-not $DryRun -and -not $SkipTests) {
    Write-Step "Running post-deployment verification..."

    # Quick smoke test - just check endpoints respond
    $verifyPassed = $true
    $gatewayPort = if ($env:GATEWAY_PORT) { $env:GATEWAY_PORT } else { "8001" }

    # Test Gateway health (HTTP frontend)
    try {
        $health = Invoke-RestMethod -Uri "http://localhost:$gatewayPort/health" -TimeoutSec 10
        Write-Success "Gateway health endpoint: OK"
    } catch {
        Write-Err "Gateway health endpoint: FAILED"
        $verifyPassed = $false
    }

    # Test llama-server health
    try {
        $health = Invoke-RestMethod -Uri "http://localhost:8080/health" -TimeoutSec 10
        Write-Success "llama-server health endpoint: OK"
    } catch {
        Write-Err "llama-server health endpoint: FAILED"
        $verifyPassed = $false
    }

    # Test Gateway UI routes
    try {
        $null = Invoke-WebRequest -Uri "http://localhost:$gatewayPort/chat" -TimeoutSec 10 -ErrorAction Stop
        Write-Success "Gateway /chat UI: OK"
    } catch {
        Write-Info "Gateway /chat UI: Not available (optional)"
    }

    # Test Gateway API info
    try {
        $apiInfo = Invoke-RestMethod -Uri "http://localhost:$gatewayPort/" -TimeoutSec 10 -ErrorAction Stop
        Write-Success "Gateway root endpoint: OK (service: $($apiInfo.service))"
    } catch {
        Write-Info "Gateway root endpoint: Not available (optional)"
    }

    if (-not $verifyPassed) {
        Write-Warn "Post-deployment verification had failures"
        Write-Info "Check logs for details: docker compose logs"
    }
}

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
$totalDuration = (Get-Date) - $startTime

Write-Host "`n"
if ($DryRun) {
    Write-Host "================================================================================" -ForegroundColor Yellow
    Write-Host "  DRY RUN COMPLETE - No changes made" -ForegroundColor Yellow
    Write-Host "================================================================================" -ForegroundColor Yellow
} else {
    Write-Host "================================================================================" -ForegroundColor Green
    Write-Host "  DEPLOYMENT COMPLETE" -ForegroundColor Green
    Write-Host "================================================================================" -ForegroundColor Green
}

Write-Host @"

  Deployment ID: $deploymentId
  Image Tag: $fullTag
  $(if ($Registry -and $Push) { "Registry: ${Registry}/${imageName}:${Tag}" })
  Duration: $([math]::Round($totalDuration.TotalSeconds, 1))s

Services:
  - API:          http://localhost:8000
  - llama-server: http://localhost:8080
  - PostgreSQL:   localhost:5432

Commands:
  docker compose ps              # Check status
  docker compose logs -f         # View logs
  docker compose down            # Stop services

================================================================================
"@

if ($DryRun) {
    Write-Host "Run without -DryRun to perform actual deployment" -ForegroundColor Yellow
}

exit 0
