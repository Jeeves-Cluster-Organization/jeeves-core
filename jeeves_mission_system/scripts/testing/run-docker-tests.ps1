<#
.SYNOPSIS
    Run all tests in Docker environment

.DESCRIPTION
    Runs the full test suite assuming Docker services are already running:
    - Unit tests
    - Integration tests
    - API tests
    - E2E tests (optional)
    - Generates coverage report

.PARAMETER TestType
    Type of tests to run: 'all', 'unit', 'integration', 'api', 'e2e'

.PARAMETER Coverage
    Generate coverage report

.PARAMETER Verbose
    Show verbose test output

.EXAMPLE
    .\run-docker-tests.ps1

.EXAMPLE
    .\run-docker-tests.ps1 -TestType unit

.EXAMPLE
    .\run-docker-tests.ps1 -Coverage
#>

param(
    [ValidateSet('all', 'unit', 'integration', 'api', 'e2e')]
    [string]$TestType = 'all',
    [switch]$Coverage,
    [switch]$VerboseOutput
)

$ErrorActionPreference = "Continue"

# Colors
function Write-Step { param($msg) Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Success { param($msg) Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Warn { param($msg) Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Write-Err { param($msg) Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Write-Info { param($msg) Write-Host "    $msg" -ForegroundColor Gray }

$startTime = Get-Date

Write-Host @"

================================================================================
  7-Agent Assistant - Docker Test Runner
================================================================================
  Test Type: $TestType
  Coverage: $Coverage
================================================================================

"@ -ForegroundColor Magenta

# -----------------------------------------------------------------------------
# Step 1: Verify Services
# -----------------------------------------------------------------------------
Write-Step "Verifying services are running..."

$servicesHealthy = $true

# Check llama-server
try {
    $null = Invoke-RestMethod -Uri "http://localhost:8080/health" -TimeoutSec 5 -ErrorAction Stop
    Write-Success "llama-server: healthy"
} catch {
    Write-Err "llama-server: not responding"
    $servicesHealthy = $false
}

# Check API
try {
    $null = Invoke-RestMethod -Uri "http://localhost:8000/health" -TimeoutSec 5 -ErrorAction Stop
    Write-Success "API: healthy"
} catch {
    Write-Warn "API: not responding (some tests may fail)"
}

# Check PostgreSQL
$pgCheck = docker compose exec -T postgres pg_isready -U assistant 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Success "PostgreSQL: healthy"
} else {
    Write-Err "PostgreSQL: not responding"
    $servicesHealthy = $false
}

if (-not $servicesHealthy) {
    Write-Err "Required services not running. Run setup script first:"
    Write-Info "  .\scripts\setup\setup-docker-windows.ps1"
    exit 1
}

# -----------------------------------------------------------------------------
# Step 2: Build Test Arguments
# -----------------------------------------------------------------------------
$pytestArgs = @("-v", "--tb=short")

if ($VerboseOutput) {
    $pytestArgs += "-s"
}

if ($Coverage) {
    $pytestArgs += @("--cov=.", "--cov-report=html", "--cov-report=term")
}

# Determine test paths
$testPaths = switch ($TestType) {
    'unit' { @("tests/unit/") }
    'integration' { @("tests/integration/") }
    'api' { @("tests/api/") }
    'e2e' { @("tests/e2e/") }
    'all' { @("tests/") }
}

$pytestArgs += $testPaths

# -----------------------------------------------------------------------------
# Step 3: Run Tests
# -----------------------------------------------------------------------------
Write-Step "Running $TestType tests..."
Write-Info "pytest $($pytestArgs -join ' ')"

# Note about LLM-dependent tests
if ($TestType -eq 'all' -or $TestType -eq 'unit') {
    Write-Info ""
    Write-Info "Note: Some tests use the LLM (llama-server). With small models (3B),"
    Write-Info "LLM-dependent tests may be slower or produce different results than"
    Write-Info "larger models. These tests are marked with @pytest.mark.e2e."
    Write-Info ""
}

$testStartTime = Get-Date

docker compose run --rm test pytest $pytestArgs
$testExitCode = $LASTEXITCODE

$testDuration = (Get-Date) - $testStartTime

# -----------------------------------------------------------------------------
# Step 4: Results Summary
# -----------------------------------------------------------------------------
$totalDuration = (Get-Date) - $startTime

Write-Host "`n"
Write-Host "================================================================================" -ForegroundColor $(if ($testExitCode -eq 0) { "Green" } else { "Red" })
Write-Host "  Test Results" -ForegroundColor $(if ($testExitCode -eq 0) { "Green" } else { "Red" })
Write-Host "================================================================================" -ForegroundColor $(if ($testExitCode -eq 0) { "Green" } else { "Red" })

if ($testExitCode -eq 0) {
    Write-Host "  Status: PASSED" -ForegroundColor Green
} else {
    Write-Host "  Status: FAILED (exit code: $testExitCode)" -ForegroundColor Red
}

Write-Host "  Test Duration: $([math]::Round($testDuration.TotalSeconds, 1))s"
Write-Host "  Total Duration: $([math]::Round($totalDuration.TotalSeconds, 1))s"

if ($Coverage) {
    Write-Host "`n  Coverage report: htmlcov/index.html"
}

Write-Host "================================================================================" -ForegroundColor $(if ($testExitCode -eq 0) { "Green" } else { "Red" })

exit $testExitCode
