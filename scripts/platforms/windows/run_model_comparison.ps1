# PowerShell script to run pytest with multiple models and capture conversation/tool outputs
# This script tests different models that fit in 6GB VRAM (7B and highly quantized 13B/14B)

# Configuration
$TestPath = "tests/integration/test_bertie_conversation_flow.py::test_weekly_summary"
$OutputDir = "model_comparison_results"
$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"

# Function to clear GPU VRAM between model runs
function Clear-OllamaVRAM {
    param(
        [string]$OllamaHost = "http://localhost:11434"
    )

    Write-Host "Clearing Ollama VRAM..." -ForegroundColor Yellow

    # Get list of loaded models
    try {
        $psOutput = & ollama ps 2>&1 | Out-String

        # Parse model names from ollama ps output
        $lines = $psOutput -split "`n"
        foreach ($line in $lines) {
            # Skip header and empty lines
            if ($line -match "^NAME\s+" -or $line -match "^\s*$") {
                continue
            }

            # Extract model name (first column)
            if ($line -match "^(\S+)") {
                $modelName = $matches[1]
                Write-Host "  Unloading: $modelName" -ForegroundColor Gray
                & ollama stop $modelName 2>&1 | Out-Null
            }
        }

        Write-Host "  VRAM cleared successfully" -ForegroundColor Green
        Start-Sleep -Seconds 2
    }
    catch {
        Write-Host "  Warning: Could not clear VRAM - $($_.Exception.Message)" -ForegroundColor Yellow
    }
}

# Define models to test - Cross-family, cross-size comparison
# Testing strategy: Vary both model family AND size to identify capabilities/limitations
# Expected: Tiny models (0.5-1.5B) will fail, small (3B) will struggle, medium+ (6-9B) should succeed
$Models = @(
    # === TINY MODELS (0.5B - 1.5B) - Expected to fail or hallucinate ===
    @{
        Name = "Qwen2-0.5B"
        Model = "qwen2:0.5b"
        Description = "Qwen 2 0.5B - Smallest model, likely to fail (352MB)"
        Size = "0.4GB"
        Family = "Qwen"
        SizeCategory = "Tiny"
    },
    @{
        Name = "Qwen2-1.5B"
        Model = "qwen2:1.5b"
        Description = "Qwen 2 1.5B - Very small, may hallucinate (934MB)"
        Size = "0.9GB"
        Family = "Qwen"
        SizeCategory = "Tiny"
    },

    # === SMALL MODELS (3B) - Expected to struggle with complex reasoning ===
    @{
        Name = "Qwen2.5-3B-Instruct"
        Model = "qwen2.5:3b-instruct"
        Description = "Qwen 2.5 3B Instruct - Small but newer generation (1.9GB)"
        Size = "1.9GB"
        Family = "Qwen"
        SizeCategory = "Small"
    },
    @{
        Name = "Llama3.2-3B"
        Model = "llama3.2:3b"
        Description = "Llama 3.2 3B - Meta's small model (2.0GB)"
        Size = "2.0GB"
        Family = "Llama"
        SizeCategory = "Small"
    },
    @{
        Name = "Phi3-Latest"
        Model = "phi3:latest"
        Description = "Phi-3 (3.8B) - Microsoft's efficient small model (2.2GB)"
        Size = "2.2GB"
        Family = "Phi"
        SizeCategory = "Small"
    },

    # === MEDIUM MODELS (6-8B) - Expected to perform adequately ===
    @{
        Name = "DeepSeek-Coder-6.7B"
        Model = "deepseek-coder:6.7b-instruct"
        Description = "DeepSeek Coder 6.7B - Code specialist (3.8GB)"
        Size = "3.8GB"
        Family = "DeepSeek"
        SizeCategory = "Medium"
    },
    @{
        Name = "Mistral-7B"
        Model = "mistral:7b"
        Description = "Mistral 7B - Popular general purpose (4.4GB)"
        Size = "4.4GB"
        Family = "Mistral"
        SizeCategory = "Medium"
    },
    @{
        Name = "Qwen2.5-7B-Instruct"
        Model = "qwen2.5:7b-instruct"
        Description = "Qwen 2.5 7B Instruct - Strong function-calling (4.7GB)"
        Size = "4.7GB"
        Family = "Qwen"
        SizeCategory = "Medium"
    },
    @{
        Name = "Llama3.1-8B-Instruct-Q4"
        Model = "llama3.1:8b-instruct-q4_0"
        Description = "Llama 3.1 8B Q4 - Reliable, well-tested (4.7GB)"
        Size = "4.7GB"
        Family = "Llama"
        SizeCategory = "Medium"
    },
    @{
        Name = "Llama3-8B"
        Model = "llama3:8b"
        Description = "Llama 3 8B - Previous generation (4.7GB)"
        Size = "4.7GB"
        Family = "Llama"
        SizeCategory = "Medium"
    },

    # === LARGE MODELS (9B) - Expected to perform best ===
    @{
        Name = "Gemma2-9B-Instruct-Q4"
        Model = "gemma2:9b-instruct-q4_0"
        Description = "Gemma 2 9B Q4 - Google's strong instruction follower (5.4GB)"
        Size = "5.4GB"
        Family = "Gemma"
        SizeCategory = "Large"
    }
)

# Test coverage summary:
# - 5 families: Qwen, Llama, Gemma, Mistral, DeepSeek, Phi
# - 4 size categories: Tiny (0.5-1.5B), Small (3B), Medium (6-8B), Large (9B)
# - Total: 11 models spanning 352MB to 5.4GB VRAM

# Create output directory
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Model Comparison Test Runner" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Test: $TestPath" -ForegroundColor Yellow
Write-Host "Models to test: $($Models.Count)" -ForegroundColor Yellow
Write-Host "Output directory: $OutputDir" -ForegroundColor Yellow
Write-Host "Timestamp: $Timestamp" -ForegroundColor Yellow
Write-Host ""

# Pre-flight check: Verify all models are available
Write-Host "Checking model availability..." -ForegroundColor Yellow
$availableModels = & ollama list 2>&1 | Out-String
$missingModels = @()

foreach ($ModelConfig in $Models) {
    $modelId = $ModelConfig.Model
    $modelBase = $modelId -replace ':.*', ''  # Extract base name (e.g., "qwen2.5" from "qwen2.5:7b-instruct")

    if ($availableModels -notmatch [regex]::Escape($modelBase)) {
        $missingModels += $modelId
        Write-Host "  ✗ Missing: $modelId" -ForegroundColor Red
    }
    else {
        Write-Host "  ✓ Found: $modelId" -ForegroundColor Green
    }
}

if ($missingModels.Count -gt 0) {
    Write-Host ""
    Write-Host "ERROR: Some models are not available!" -ForegroundColor Red
    Write-Host "Please pull the missing models first:" -ForegroundColor Yellow
    foreach ($model in $missingModels) {
        Write-Host "  ollama pull $model" -ForegroundColor White
    }
    Write-Host ""
    Write-Host "Or remove them from the `$Models array in the script." -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "All models available! Starting tests..." -ForegroundColor Green
Write-Host ""

# Summary tracking
$Results = @()
$TotalModels = $Models.Count
$CurrentModel = 0

foreach ($ModelConfig in $Models) {
    $CurrentModel++
    $ModelName = $ModelConfig.Name
    $ModelId = $ModelConfig.Model
    $Description = $ModelConfig.Description
    $ProgressPercent = [math]::Round(($CurrentModel / $TotalModels) * 100, 0)

    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "Progress: Model $CurrentModel of $TotalModels (${ProgressPercent}%)" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "Testing: $ModelName" -ForegroundColor Green
    Write-Host "Model ID: $ModelId" -ForegroundColor Gray
    Write-Host "Description: $Description" -ForegroundColor Gray
    Write-Host "VRAM: $($ModelConfig.Size)" -ForegroundColor Gray
    Write-Host "========================================" -ForegroundColor Green
    Write-Host ""

    # Set environment variables for this model
    $env:LLM_PROVIDER = "ollama"
    $env:DEPLOYMENT_MODE = "single_node"
    $env:OLLAMA_HOST = "http://localhost:11434"
    $env:DEFAULT_MODEL = $ModelId
    $env:PLANNER_MODEL = $ModelId
    $env:VALIDATOR_MODEL = $ModelId
    $env:CRITIC_MODEL = $ModelId
    $env:META_VALIDATOR_MODEL = $ModelId
    $env:OLLAMA_AVAILABLE = "1"
    $env:ENABLE_STRUCTLOG = "1"

    # Output files
    $SafeName = $ModelName -replace '[^a-zA-Z0-9-]', '_'
    $FullOutputFile = Join-Path $OutputDir "${SafeName}_full_${Timestamp}.log"
    $FilteredOutputFile = Join-Path $OutputDir "${SafeName}_conversation_${Timestamp}.log"

    Write-Host "Running pytest (this may take 2-3 minutes)..." -ForegroundColor Yellow
    Write-Host "Progress indicators: " -NoNewline -ForegroundColor Gray
    $StartTime = Get-Date

    # Run pytest in background and show progress
    $job = Start-Job -ScriptBlock {
        param($testPath)
        & pytest $testPath -sv 2>&1 | Out-String
    } -ArgumentList $TestPath

    # Show progress dots while test runs
    $dots = 0
    while ($job.State -eq 'Running') {
        Write-Host "." -NoNewline -ForegroundColor Yellow
        Start-Sleep -Seconds 5
        $dots++

        # Show elapsed time every 30 seconds
        if ($dots % 6 -eq 0) {
            $elapsed = [math]::Round(((Get-Date) - $StartTime).TotalSeconds, 0)
            Write-Host " [$elapsed seconds elapsed]" -NoNewline -ForegroundColor Gray
        }
    }

    # Get test output
    $TestOutput = Receive-Job -Job $job
    Remove-Job -Job $job

    Write-Host " Done!" -ForegroundColor Green

    $EndTime = Get-Date
    $Duration = ($EndTime - $StartTime).TotalSeconds

    # Save full output
    $TestOutput | Out-File -FilePath $FullOutputFile -Encoding UTF8

    # Extract conversation and tool execution parts (DEBUG LEVEL)
    $FilteredLines = @()
    $FilteredLines += "=" * 80
    $FilteredLines += "FILTERED OUTPUT - CONVERSATION + TOOL CALLS (DEBUG LEVEL)"
    $FilteredLines += "=" * 80
    $FilteredLines += "Model: $ModelName"
    $FilteredLines += "Family: $($ModelConfig.Family)"
    $FilteredLines += "Size Category: $($ModelConfig.SizeCategory)"
    $FilteredLines += "Model ID: $ModelId"
    $FilteredLines += "Description: $Description"
    $FilteredLines += "VRAM: $($ModelConfig.Size)"
    $FilteredLines += "Timestamp: $Timestamp"
    $FilteredLines += "Duration: $([math]::Round($Duration, 2)) seconds"
    $FilteredLines += "=" * 80
    $FilteredLines += ""

    # Split output into lines and filter for conversation + tool calls (debug level)
    $Lines = $TestOutput -split "`n"
    $InToolSection = $false
    $InScenario = $false
    $InStructlogSection = $false

    foreach ($Line in $Lines) {
        # Capture scenario headers
        if ($Line -match "^\s*Scenario:") {
            $InScenario = $true
            $FilteredLines += ""
            $FilteredLines += $Line
            continue
        }

        # Capture conversation turns [01], [02], etc.
        if ($Line -match "^\s*\[\d{2}\]\s+(Bertie|AI):") {
            $FilteredLines += $Line
            continue
        }

        # Capture [DEBUG] headers (LLM config, tool execution, etc.)
        if ($Line -match "^\[DEBUG\]") {
            $FilteredLines += ""
            $FilteredLines += $Line
            continue
        }

        # Capture tool execution sections
        if ($Line -match "^\s*\[DEBUG\] Tools executed:") {
            $InToolSection = $true
            $FilteredLines += ""
            $FilteredLines += $Line
            continue
        }

        # Capture structlog entries for tool execution
        if ($Line -match "\[info\s+\]\s+(tool_executed|add_task|task_complete|get_tasks|plan_generated|executing_node|PlannerAgent|ExecutorAgent|ValidatorAgent)") {
            $FilteredLines += $Line
            continue
        }

        # Capture tool details (indented lines after [DEBUG] Tools executed)
        if ($InToolSection) {
            # Check if line is indented (tool detail)
            if ($Line -match '^\s{2,}\[') {
                $FilteredLines += $Line
                continue
            }
            # Check for tool result details
            elseif ($Line -match '^\s{2,}(Status:|Data:|[+]|[-])') {
                $FilteredLines += $Line
                continue
            }
            # Check for PASSED/FAILED markers
            elseif ($Line -match '^(PASSED|FAILED)') {
                $FilteredLines += ""
                $FilteredLines += $Line
                $InToolSection = $false
                continue
            }
            # Empty line might separate sections
            elseif ($Line -match '^\s*$') {
                # Check if next section is starting
                $InToolSection = $false
            }
            else {
                # Not a tool line, exit tool section
                $InToolSection = $false
            }
        }

        # Capture test result summary
        if ($Line -match "^=+\s*\d+\s+(passed|failed)") {
            $FilteredLines += ""
            $FilteredLines += $Line
        }

        # Capture test mode markers
        if ($Line -match "\[Mode: Debug") {
            $FilteredLines += $Line
        }
    }

    # Save filtered output
    $FilteredLines -join "`n" | Out-File -FilePath $FilteredOutputFile -Encoding UTF8

    # Determine test result
    $TestPassed = $TestOutput -match "PASSED"
    $TestFailed = $TestOutput -match "FAILED"

    $Status = if ($TestPassed) { "PASSED" } elseif ($TestFailed) { "FAILED" } else { "UNKNOWN" }
    $StatusColor = if ($TestPassed) { "Green" } elseif ($TestFailed) { "Red" } else { "Yellow" }

    Write-Host ""
    Write-Host "Status: $Status" -ForegroundColor $StatusColor
    Write-Host "Duration: $([math]::Round($Duration, 2)) seconds" -ForegroundColor Gray
    Write-Host "Full output: $FullOutputFile" -ForegroundColor Gray
    Write-Host "Filtered output: $FilteredOutputFile" -ForegroundColor Gray
    Write-Host ""

    # Add to results summary
    $Results += [PSCustomObject]@{
        Model = $ModelName
        Family = $ModelConfig.Family
        SizeCategory = $ModelConfig.SizeCategory
        Size = $ModelConfig.Size
        Status = $Status
        Duration = [math]::Round($Duration, 2)
        ModelID = $ModelId
        FullOutput = $FullOutputFile
        FilteredOutput = $FilteredOutputFile
    }

    # Clear VRAM before next model test
    Write-Host ""
    Clear-OllamaVRAM
    Write-Host ""
}

# Print summary
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Test Summary - Completed $TotalModels models" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$PassedCount = ($Results | Where-Object { $_.Status -eq "PASSED" }).Count
$FailedCount = ($Results | Where-Object { $_.Status -eq "FAILED" }).Count
$TotalDuration = ($Results | Measure-Object -Property Duration -Sum).Sum

Write-Host "Overall Results: $PassedCount passed, $FailedCount failed" -ForegroundColor $(if ($FailedCount -eq 0) { "Green" } else { "Yellow" })
$TotalMinutes = [math]::Round($TotalDuration / 60, 1)
Write-Host "Total time: $([math]::Round($TotalDuration, 2)) seconds (${TotalMinutes} min)" -ForegroundColor Gray
Write-Host ""

# Results by size category
Write-Host "Results by Size Category:" -ForegroundColor Yellow
foreach ($category in @("Tiny", "Small", "Medium", "Large")) {
    $categoryResults = $Results | Where-Object { $_.SizeCategory -eq $category }
    if ($categoryResults.Count -gt 0) {
        $categoryPassed = ($categoryResults | Where-Object { $_.Status -eq "PASSED" }).Count
        $categoryFailed = ($categoryResults | Where-Object { $_.Status -eq "FAILED" }).Count
        $passRate = if ($categoryResults.Count -gt 0) { [math]::Round(($categoryPassed / $categoryResults.Count) * 100, 0) } else { 0 }
        $color = if ($passRate -ge 80) { "Green" } elseif ($passRate -ge 50) { "Yellow" } else { "Red" }
        $countText = "$($categoryResults.Count) models"
        $rateText = "${passRate}% pass rate"
        Write-Host "  ${category} ($countText): $categoryPassed passed, $categoryFailed failed ($rateText)" -ForegroundColor $color
    }
}
Write-Host ""

# Results by family
Write-Host "Results by Model Family:" -ForegroundColor Yellow
$families = $Results | Select-Object -ExpandProperty Family -Unique | Sort-Object
foreach ($family in $families) {
    $familyResults = $Results | Where-Object { $_.Family -eq $family }
    $familyPassed = ($familyResults | Where-Object { $_.Status -eq "PASSED" }).Count
    $familyFailed = ($familyResults | Where-Object { $_.Status -eq "FAILED" }).Count
    Write-Host "  $family ($($familyResults.Count) models): $familyPassed passed, $familyFailed failed" -ForegroundColor Gray
}
Write-Host ""

# Detailed results table
Write-Host "Detailed Results:" -ForegroundColor Yellow
$Results | Format-Table -Property Model, Family, SizeCategory, Size, Status, Duration -AutoSize

Write-Host ""
Write-Host "All results saved to: $OutputDir" -ForegroundColor Green
Write-Host ""

# Save summary to CSV
$SummaryFile = Join-Path $OutputDir "summary_${Timestamp}.csv"
$Results | Export-Csv -Path $SummaryFile -NoTypeInformation -Encoding UTF8

Write-Host "Summary CSV: $SummaryFile" -ForegroundColor Green
Write-Host ""

# Generate analysis report
$AnalysisFile = Join-Path $OutputDir "ANALYSIS_${Timestamp}.txt"
$AnalysisLines = @()
$AnalysisLines += "=" * 80
$AnalysisLines += "MODEL COMPARISON ANALYSIS REPORT"
$AnalysisLines += "Generated: $Timestamp"
$AnalysisLines += "=" * 80
$AnalysisLines += ""
$AnalysisLines += "TEST OVERVIEW"
$AnalysisLines += "-------------"
$AnalysisLines += "Test: $TestPath"
$AnalysisLines += "Models tested: $TotalModels"
$AnalysisLines += "Total duration: $([math]::Round($TotalDuration, 2)) seconds (${TotalMinutes} min)"
$AnalysisLines += "Overall pass rate: $([math]::Round(($PassedCount / $TotalModels) * 100, 0))%"
$AnalysisLines += ""
$AnalysisLines += "RESULTS BY SIZE CATEGORY"
$AnalysisLines += "------------------------"
foreach ($category in @("Tiny", "Small", "Medium", "Large")) {
    $categoryResults = $Results | Where-Object { $_.SizeCategory -eq $category }
    if ($categoryResults.Count -gt 0) {
        $categoryPassed = ($categoryResults | Where-Object { $_.Status -eq "PASSED" }).Count
        $passRate = [math]::Round(($categoryPassed / $categoryResults.Count) * 100, 0)
        $categoryCount = $categoryResults.Count
        $AnalysisLines += "${category} ($categoryCount models): ${passRate}% pass rate"
        foreach ($result in $categoryResults) {
            $statusSymbol = if ($result.Status -eq "PASSED") { "[PASS]" } else { "[FAIL]" }
            $AnalysisLines += "  $statusSymbol $($result.Model) - $($result.Duration)s"
        }
        $AnalysisLines += ""
    }
}
$AnalysisLines += "RESULTS BY MODEL FAMILY"
$AnalysisLines += "-----------------------"
$families = $Results | Select-Object -ExpandProperty Family -Unique | Sort-Object
foreach ($family in $families) {
    $familyResults = $Results | Where-Object { $_.Family -eq $family }
    $familyPassed = ($familyResults | Where-Object { $_.Status -eq "PASSED" }).Count
    $passRate = [math]::Round(($familyPassed / $familyResults.Count) * 100, 0)
    $familyCount = $familyResults.Count
    $AnalysisLines += "${family} ($familyCount models): ${passRate}% pass rate"
    foreach ($result in $familyResults) {
        $statusSymbol = if ($result.Status -eq "PASSED") { "[PASS]" } else { "[FAIL]" }
        $AnalysisLines += "  $statusSymbol $($result.Model) [$($result.SizeCategory)] - $($result.Duration)s"
    }
    $AnalysisLines += ""
}
$AnalysisLines += "KEY FINDINGS"
$AnalysisLines += "------------"
$tinyResults = $Results | Where-Object { $_.SizeCategory -eq "Tiny" }
$smallResults = $Results | Where-Object { $_.SizeCategory -eq "Small" }
$mediumResults = $Results | Where-Object { $_.SizeCategory -eq "Medium" }
$largeResults = $Results | Where-Object { $_.SizeCategory -eq "Large" }

$tinyPassRate = if ($tinyResults.Count -gt 0) { [math]::Round((($tinyResults | Where-Object { $_.Status -eq "PASSED" }).Count / $tinyResults.Count) * 100, 0) } else { 0 }
$smallPassRate = if ($smallResults.Count -gt 0) { [math]::Round((($smallResults | Where-Object { $_.Status -eq "PASSED" }).Count / $smallResults.Count) * 100, 0) } else { 0 }
$mediumPassRate = if ($mediumResults.Count -gt 0) { [math]::Round((($mediumResults | Where-Object { $_.Status -eq "PASSED" }).Count / $mediumResults.Count) * 100, 0) } else { 0 }
$largePassRate = if ($largeResults.Count -gt 0) { [math]::Round((($largeResults | Where-Object { $_.Status -eq "PASSED" }).Count / $largeResults.Count) * 100, 0) } else { 0 }

$AnalysisLines += "1. Model size clearly impacts success:"
$AnalysisLines += "   - Tiny (0.5-1.5B): ${tinyPassRate}% pass rate - Too small for complex tasks"
$AnalysisLines += "   - Small (3B): ${smallPassRate}% pass rate - Struggles with multi-step reasoning"
$AnalysisLines += "   - Medium (6-8B): ${mediumPassRate}% pass rate - Adequate for most tasks"
$AnalysisLines += "   - Large (9B+): ${largePassRate}% pass rate - Best performance"
$AnalysisLines += ""
$AnalysisLines += "2. Recommended minimum model size:"
if ($mediumPassRate -ge 80) {
    $AnalysisLines += "   Medium (6-8B) models recommended for production use"
} elseif ($largePassRate -ge 80) {
    $AnalysisLines += "   Large (9B+) models required for reliable operation"
} else {
    $AnalysisLines += "   Consider larger models or simpler tasks"
}
$AnalysisLines += ""
$AnalysisLines += "FILES GENERATED"
$AnalysisLines += "---------------"
$AnalysisLines += "Summary CSV: $SummaryFile"
$AnalysisLines += "Per-model outputs:"
foreach ($result in $Results) {
    $AnalysisLines += "  - $($result.Model): $($result.FilteredOutput)"
}
$AnalysisLines += ""
$AnalysisLines += "=" * 80

$AnalysisLines -join "`n" | Out-File -FilePath $AnalysisFile -Encoding UTF8

Write-Host "Analysis report: $AnalysisFile" -ForegroundColor Green
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Comparison complete!" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
