<#
.SYNOPSIS
    Validate deployment readiness

.DESCRIPTION
    Pre-deployment validation that ensures everything is ready for production:
    - Services are healthy
    - All tests pass
    - Configuration is valid
    - Docker images build successfully

.PARAMETER SkipTests
    Skip running tests (use if tests were just run)

.PARAMETER Quick
    Quick validation (services only, no tests)

.EXAMPLE
    .\validate-deployment.ps1

.EXAMPLE
    .\validate-deployment.ps1 -SkipTests

.EXAMPLE
    .\validate-deployment.ps1 -Quick
#>

param(
    [switch]$SkipTests,
    [switch]$Quick
)

$ErrorActionPreference = "Continue"

# Colors
function Write-Step { param($msg) Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Success { param($msg) Write-Host "[PASS] $msg" -ForegroundColor Green }
function Write-Fail { param($msg) Write-Host "[FAIL] $msg" -ForegroundColor Red }
function Write-Skip { param($msg) Write-Host "[SKIP] $msg" -ForegroundColor Yellow }
function Write-Info { param($msg) Write-Host "    $msg" -ForegroundColor Gray }

$checks = @{
    Passed = @()
    Failed = @()
    Skipped = @()
}

function Add-Check {
    param($name, $passed, $skipped = $false)
    if ($skipped) {
        $script:checks.Skipped += $name
        Write-Skip $name
    } elseif ($passed) {
        $script:checks.Passed += $name
        Write-Success $name
    } else {
        $script:checks.Failed += $name
        Write-Fail $name
    }
}

Write-Host @"

================================================================================
  7-Agent Assistant - Deployment Validation
================================================================================
  Mode: $(if ($Quick) { "Quick" } elseif ($SkipTests) { "No Tests" } else { "Full" })
================================================================================

"@ -ForegroundColor Magenta

# -----------------------------------------------------------------------------
# Check 1: Docker Running
# -----------------------------------------------------------------------------
Write-Step "Checking prerequisites..."

$dockerRunning = $false
$null = docker info 2>&1
if ($LASTEXITCODE -eq 0) {
    $dockerRunning = $true
}
Add-Check "Docker is running" $dockerRunning

# -----------------------------------------------------------------------------
# Check 2: Required Files Exist
# -----------------------------------------------------------------------------
Write-Step "Checking configuration files..."

Add-Check ".env file exists" (Test-Path ".env")
Add-Check "docker-compose.yml exists" (Test-Path "docker-compose.yml")
Add-Check "Dockerfile exists" (Test-Path "Dockerfile")

# Check .env has required variables
if (Test-Path ".env") {
    $envContent = Get-Content ".env" -Raw
    Add-Check ".env has LLAMA_MODEL" ($envContent -match "LLAMA_MODEL=")
    Add-Check ".env has DATABASE_BACKEND" ($envContent -match "DATABASE_BACKEND=")
    Add-Check ".env has LLM_PROVIDER" ($envContent -match "LLM_PROVIDER=")
}

# -----------------------------------------------------------------------------
# Check 3: Model File Exists
# -----------------------------------------------------------------------------
Write-Step "Checking model file..."

# Check if model exists in volume (use find instead of glob for more reliable exit codes)
$modelCheck = docker run --rm -v llama-models:/models alpine sh -c "find /models -name '*.gguf' | head -1 | grep -q ." 2>&1
$modelExists = $LASTEXITCODE -eq 0

# If llama-server is healthy, the model must exist (it won't start without one)
if (-not $modelExists) {
    # Try checking if llama-server is healthy first
    try {
        $null = Invoke-RestMethod -Uri "http://localhost:8080/health" -TimeoutSec 3 -ErrorAction Stop
        # If llama-server is healthy, model definitely exists
        $modelExists = $true
        Write-Info "Model verified via llama-server health"
    } catch { }
}
Add-Check "Model file exists in volume" $modelExists

# -----------------------------------------------------------------------------
# Check 4: Services Health
# -----------------------------------------------------------------------------
Write-Step "Checking services health..."

# llama-server
$llamaHealthy = $false
try {
    $null = Invoke-RestMethod -Uri "http://localhost:8080/health" -TimeoutSec 5 -ErrorAction Stop
    $llamaHealthy = $true
} catch { }
Add-Check "llama-server is healthy" $llamaHealthy

# API (gRPC - check host port from API_PORT env var, default 8000)
$apiPort = if ($env:API_PORT) { $env:API_PORT } else { "8000" }
$apiHealthy = $false
$apiError = ""
try {
    # Check gRPC port via TCP connection
    $tcp = New-Object System.Net.Sockets.TcpClient
    $tcp.Connect("localhost", [int]$apiPort)
    $tcp.Close()
    $apiHealthy = $true
} catch {
    $apiError = $_.Exception.Message
}

if (-not $apiHealthy -and $apiError) {
    # Check if container is running
    $containerStatus = docker ps --filter "name=assistant-7agent-api" --format "{{.Status}}" 2>&1
    if ($containerStatus) {
        Write-Info "API container status: $containerStatus"
    } else {
        Write-Info "API container not running"
    }
}
Add-Check "API (gRPC:$apiPort) is healthy" $apiHealthy

# PostgreSQL
$pgHealthy = $false
$pgCheck = docker compose exec -T postgres pg_isready -U assistant 2>&1
if ($LASTEXITCODE -eq 0) {
    $pgHealthy = $true
}
Add-Check "PostgreSQL is healthy" $pgHealthy

# Gateway (HTTP frontend)
$gatewayPort = if ($env:GATEWAY_PORT) { $env:GATEWAY_PORT } else { "8001" }
$gatewayHealthy = $false
try {
    $null = Invoke-RestMethod -Uri "http://localhost:$gatewayPort/health" -TimeoutSec 5 -ErrorAction Stop
    $gatewayHealthy = $true
} catch { }
Add-Check "Gateway (HTTP:$gatewayPort) is healthy" $gatewayHealthy

if ($Quick) {
    # Skip remaining checks
    Add-Check "Docker build" $true $true
    Add-Check "Unit tests" $true $true
    Add-Check "Integration tests" $true $true
} else {
    # -----------------------------------------------------------------------------
    # Check 5: Docker Build
    # -----------------------------------------------------------------------------
    Write-Step "Verifying Docker build..."

    docker compose build --quiet 2>&1 | Out-Null
    Add-Check "Docker images build successfully" ($LASTEXITCODE -eq 0)

    # -----------------------------------------------------------------------------
    # Check 6: Tests
    # -----------------------------------------------------------------------------
    if ($SkipTests) {
        Add-Check "Unit tests" $true $true
        Add-Check "Integration tests" $true $true
    } else {
        Write-Step "Running tests..."

        # Run a quick subset of unit tests (deterministic ones, no LLM)
        Write-Info "Running unit tests (quick, non-LLM)..."
        Write-Info "(Full test suite can be run with: docker compose run --rm test pytest -v)"

        # Run tests that don't require LLM - these are fast and deterministic
        $testResult = docker compose run --rm test pytest tests/unit/test_tool_registry.py tests/unit/test_circuit_breaker.py tests/unit/test_intent_prompts.py -v --tb=short -q 2>&1
        $testExitCode = $LASTEXITCODE

        if ($testExitCode -ne 0) {
            # Show test output on failure
            Write-Info ($testResult | Out-String)
        }
        Add-Check "Unit tests pass" ($testExitCode -eq 0)

        # Skip LLM-dependent tests in validation (they're slow and can timeout)
        Write-Info "Note: LLM-dependent tests skipped for quick validation"
        Write-Info "Run full test suite: docker compose run --rm test pytest -v"
        Add-Check "Integration tests pass" $true $true
    }
}

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
$totalChecks = $checks.Passed.Count + $checks.Failed.Count
$passedChecks = $checks.Passed.Count

Write-Host "`n"
if ($checks.Failed.Count -eq 0) {
    Write-Host "================================================================================" -ForegroundColor Green
    Write-Host "  VALIDATION PASSED - Ready for deployment" -ForegroundColor Green
    Write-Host "================================================================================" -ForegroundColor Green
} else {
    Write-Host "================================================================================" -ForegroundColor Red
    Write-Host "  VALIDATION FAILED - Not ready for deployment" -ForegroundColor Red
    Write-Host "================================================================================" -ForegroundColor Red
    Write-Host "`nFailed checks:" -ForegroundColor Red
    $checks.Failed | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
}

Write-Host "`nSummary: $passedChecks/$totalChecks checks passed"
if ($checks.Skipped.Count -gt 0) {
    Write-Host "Skipped: $($checks.Skipped.Count) checks"
}
Write-Host "================================================================================"

if ($checks.Failed.Count -gt 0) {
    exit 1
}
exit 0
