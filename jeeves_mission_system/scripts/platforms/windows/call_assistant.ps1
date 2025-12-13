#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Call the 7-Agent Personal Assistant from the command line.

.DESCRIPTION
    This script allows you to interact with the assistant by sending messages
    and receiving responses via the API.

.PARAMETER Message
    The message to send to the assistant (required)

.PARAMETER UserId
    User ID for the request (default: "cli-user")

.PARAMETER SessionId
    Session ID to maintain conversation context (optional)

.PARAMETER Host
    API host (default: "localhost")

.PARAMETER Port
    API port (default: 8000)

.PARAMETER Verbose
    Show detailed output

.EXAMPLE
    .\call_assistant.ps1 -Message "Hey, list all the tasks I have"

.EXAMPLE
    .\call_assistant.ps1 -Message "Add a task: Review PR #42" -SessionId "my-session"

.EXAMPLE
    .\call_assistant.ps1 -Message "What's in my journal?" -UserId "alice" -Verbose
#>

param(
    [Parameter(Mandatory=$true)]
    [string]$Message,

    [Parameter(Mandatory=$false)]
    [string]$UserId = "cli-user",

    [Parameter(Mandatory=$false)]
    [string]$SessionId,

    [Parameter(Mandatory=$false)]
    [string]$Host = "localhost",

    [Parameter(Mandatory=$false)]
    [int]$Port = 8000,

    [Parameter(Mandatory=$false)]
    [switch]$Verbose
)

# Color output functions
function Write-Success {
    param([string]$Text)
    Write-Host $Text -ForegroundColor Green
}

function Write-Error {
    param([string]$Text)
    Write-Host $Text -ForegroundColor Red
}

function Write-Info {
    param([string]$Text)
    Write-Host $Text -ForegroundColor Cyan
}

function Write-Warning {
    param([string]$Text)
    Write-Host $Text -ForegroundColor Yellow
}

# Build API URL
$apiUrl = "http://${Host}:${Port}/api/v1/requests"

if ($Verbose) {
    Write-Info "API URL: $apiUrl"
    Write-Info "User ID: $UserId"
    Write-Info "Message: $Message"
    if ($SessionId) {
        Write-Info "Session ID: $SessionId"
    }
    Write-Host ""
}

# Build request body
$body = @{
    user_message = $Message
    user_id = $UserId
}

if ($SessionId) {
    $body.session_id = $SessionId
}

$jsonBody = $body | ConvertTo-Json

if ($Verbose) {
    Write-Info "Request body:"
    Write-Host $jsonBody
    Write-Host ""
}

# Check if server is healthy
try {
    $healthUrl = "http://${Host}:${Port}/health"
    $healthResponse = Invoke-RestMethod -Uri $healthUrl -Method Get -TimeoutSec 5
    if ($Verbose) {
        Write-Success "Server is healthy"
    }
} catch {
    Write-Error "Error: Cannot connect to assistant server at http://${Host}:${Port}"
    Write-Error "Make sure the server is running with: uvicorn api.server:app --reload"
    Write-Host ""
    Write-Host "Error details: $_"
    exit 1
}

# Send request to assistant
try {
    Write-Info "Sending message to assistant..."
    Write-Host ""

    $response = Invoke-RestMethod -Uri $apiUrl -Method Post -Body $jsonBody -ContentType "application/json" -TimeoutSec 30

    if ($Verbose) {
        Write-Info "Response received:"
        Write-Host ($response | ConvertTo-Json -Depth 10)
        Write-Host ""
    }

    # Display response
    Write-Success "=== Assistant Response ==="
    Write-Host ""

    if ($response.response_text) {
        Write-Host $response.response_text
    } else {
        Write-Warning "No response text received"
    }

    Write-Host ""

    # Display status and metadata
    if ($Verbose) {
        Write-Info "Status: $($response.status)"
        Write-Info "Request ID: $($response.request_id)"
    }

    # Handle clarification if needed
    if ($response.clarification_needed -and $response.clarification_question) {
        Write-Host ""
        Write-Warning "=== Clarification Needed ==="
        Write-Host $response.clarification_question
        Write-Host ""
        Write-Info "Hint: Use -SessionId '$($response.request_id)' to continue this conversation"
    }

} catch {
    Write-Error "Error communicating with assistant:"
    Write-Host $_.Exception.Message
    Write-Host ""

    if ($Verbose) {
        Write-Host "Full error:"
        Write-Host $_
    }

    exit 1
}
