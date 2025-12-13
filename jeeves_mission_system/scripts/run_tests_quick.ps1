# Quick test runner for Phase 0 verification
# Runs only essential tests without hanging

param(
    [switch]$Verbose,
    [switch]$Help
)

function Show-Usage {
    Write-Host @"
Usage: .\run_tests_quick.ps1 [-Verbose] [-Help]

Quick test runner that avoids potentially hanging operations.

OPTIONS:
    -Verbose    Show detailed test output
    -Help       Show this help message

EXAMPLES:
    # Run quick tests
    .\run_tests_quick.ps1

    # With verbose output
    .\run_tests_quick.ps1 -Verbose
"@
}

if ($Help) {
    Show-Usage
    exit 0
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Quick Test Runner" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $ProjectRoot

try {
    # Test 1: Import feature flags
    Write-Host "Testing feature flags import... " -NoNewline
    $result = python -c "from config.feature_flags import FeatureFlags; print('OK')" 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "OK" -ForegroundColor Green
    } else {
        Write-Host "FAIL" -ForegroundColor Red
        Write-Host "  Error: $result"
        exit 1
    }

    # Test 2: Import settings
    Write-Host "Testing settings import... " -NoNewline
    $result = python -c "from config.settings import settings; print('OK')" 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "OK" -ForegroundColor Green
    } else {
        Write-Host "FAIL" -ForegroundColor Red
        Write-Host "  Error: $result"
        exit 1
    }

    # Test 3: Feature flag validation
    Write-Host "Testing feature flag validation... " -NoNewline
    $result = python -c "from config.feature_flags import feature_flags; errors = feature_flags.validate_dependencies(); exit(0 if not errors else 1)" 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "OK" -ForegroundColor Green
    } else {
        Write-Host "FAIL" -ForegroundColor Red
        Write-Host "  Error: $result"
        exit 1
    }

    # Test 4: Run unit tests (without coverage to avoid hang)
    Write-Host "Running unit tests (subset)... " -NoNewline

    $testArgs = @(
        "-m", "pytest",
        "tests/unit/test_health.py",
        "-v",
        "--tb=short",
        "-x"
    )

    if ($Verbose) {
        Write-Host ""
        & python @testArgs
    } else {
        $null = & python @testArgs 2>&1
    }

    if ($LASTEXITCODE -eq 0) {
        if (-not $Verbose) {
            Write-Host "OK" -ForegroundColor Green
        }
    } else {
        Write-Host "FAIL" -ForegroundColor Red
        Write-Host "  Run with -Verbose to see details"
        exit 1
    }

    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "Quick tests passed!" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "To run full test suite:"
    Write-Host "  pytest -v"
    Write-Host ""
    Write-Host "To run with coverage:"
    Write-Host "  pytest -v --cov=. --cov-report=html"
    Write-Host ""

} finally {
    Pop-Location
}
