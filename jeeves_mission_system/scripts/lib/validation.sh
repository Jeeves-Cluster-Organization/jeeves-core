#!/bin/bash
# Validation utilities library
# Source common.sh before using this library

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common functions if not already loaded
if [ -z "$(type -t print_info)" ]; then
    source "$SCRIPT_DIR/common.sh"
fi

# Validate Python dependencies
validate_python_deps() {
    print_step "Validating Python dependencies..."

    activate_venv || return 1

    if [ -f "requirements.txt" ]; then
        python3 -m pip check
        if [ $? -eq 0 ]; then
            print_success "Python dependencies are valid"
            return 0
        else
            print_error "Python dependencies have conflicts"
            return 1
        fi
    else
        print_error "requirements.txt not found"
        return 1
    fi
}

# Validate environment configuration
validate_env_config() {
    print_step "Validating environment configuration..."

    local required_vars=("LLM_PROVIDER")
    local missing_vars=()

    for var in "${required_vars[@]}"; do
        if [ -z "${!var}" ]; then
            missing_vars+=("$var")
        fi
    done

    if [ ${#missing_vars[@]} -eq 0 ]; then
        print_success "Environment configuration is valid"
        print_info "LLM_PROVIDER=$LLM_PROVIDER"
        return 0
    else
        print_error "Missing required environment variables:"
        for var in "${missing_vars[@]}"; do
            print_error "  - $var"
        done
        return 1
    fi
}

# Validate project structure
validate_project_structure() {
    print_step "Validating project structure..."

    local required_dirs=(
        "agents"
        "api"
        "common"
        "config"
        "database"
        "llm"
        "memory"
        "tools"
        "tests"
        "scripts"
        "data"
    )

    local missing_dirs=()

    for dir in "${required_dirs[@]}"; do
        if [ ! -d "$dir" ]; then
            missing_dirs+=("$dir")
        fi
    done

    local required_files=(
        "init_db.py"
        "start_server.py"
        "requirements.txt"
        ".env"
    )

    local missing_files=()

    for file in "${required_files[@]}"; do
        if [ ! -f "$file" ]; then
            missing_files+=("$file")
        fi
    done

    if [ ${#missing_dirs[@]} -eq 0 ] && [ ${#missing_files[@]} -eq 0 ]; then
        print_success "Project structure is valid"
        return 0
    else
        print_error "Project structure validation failed:"
        if [ ${#missing_dirs[@]} -gt 0 ]; then
            print_error "Missing directories:"
            for dir in "${missing_dirs[@]}"; do
                print_error "  - $dir/"
            done
        fi
        if [ ${#missing_files[@]} -gt 0 ]; then
            print_error "Missing files:"
            for file in "${missing_files[@]}"; do
                print_error "  - $file"
            done
        fi
        return 1
    fi
}

# Validate imports (check if Python modules can be imported)
validate_imports() {
    print_step "Validating Python imports..."

    activate_venv || return 1

    python3 << 'EOF'
import sys

modules_to_test = [
    "fastapi",
    "uvicorn",
    "pytest",
    "sqlalchemy",
    "pydantic",
]

failed = []

for module in modules_to_test:
    try:
        __import__(module)
    except ImportError as e:
        failed.append(f"{module}: {e}")

if failed:
    print("Failed to import:")
    for f in failed:
        print(f"  ✗ {f}")
    sys.exit(1)
else:
    print("✓ All required modules can be imported")
    sys.exit(0)
EOF

    if [ $? -eq 0 ]; then
        print_success "Import validation passed"
        return 0
    else
        print_error "Import validation failed"
        return 1
    fi
}

# Run full validation suite
run_full_validation() {
    local failed=0

    print_step "Running full validation suite..."
    echo ""

    # Validate project structure
    validate_project_structure
    [ $? -ne 0 ] && failed=$((failed + 1))
    echo ""

    # Validate Python
    check_python
    [ $? -ne 0 ] && failed=$((failed + 1))
    echo ""

    # Validate venv
    check_venv
    [ $? -ne 0 ] && failed=$((failed + 1))
    echo ""

    # Validate imports
    validate_imports
    [ $? -ne 0 ] && failed=$((failed + 1))
    echo ""

    # Validate dependencies
    validate_python_deps
    [ $? -ne 0 ] && failed=$((failed + 1))
    echo ""

    # Validate environment
    load_env
    validate_env_config
    [ $? -ne 0 ] && failed=$((failed + 1))
    echo ""

    # Validate database if it exists
    if [ -f "data/memory.db" ]; then
        source "$SCRIPT_DIR/database.sh"
        validate_database
        [ $? -ne 0 ] && failed=$((failed + 1))
        echo ""
    fi

    # Summary
    if [ $failed -eq 0 ]; then
        print_success "All validation checks passed!"
        return 0
    else
        print_error "$failed validation check(s) failed"
        return 1
    fi
}

# Export functions
export -f validate_python_deps
export -f validate_env_config
export -f validate_project_structure
export -f validate_imports
export -f run_full_validation
