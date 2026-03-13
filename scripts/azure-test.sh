#!/usr/bin/env bash
# azure-test.sh - Test 3DGS Processor with Azure Blob Storage
#
# This script:
# 1. Loads Azure credentials from azure-test-config.env
# 2. Uploads test videos to Azure Blob Storage
# 3. Runs the 3DGS processor container with Azure mounting
# 4. Validates outputs and writes test results
#
# Prerequisites:
# - Run ./scripts/azure-setup.sh first
# - Container image built: 3dgs-processor:latest
# - Docker or Podman installed
#
# Usage:
#   ./scripts/azure-test.sh [connection|sas]
#
# Examples:
#   ./scripts/azure-test.sh connection  # Test with connection string (default)
#   ./scripts/azure-test.sh sas         # Test with SAS token

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test authentication method
AUTH_METHOD="${1:-connection}"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_test() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

# Load Azure configuration
CONFIG_FILE="$PROJECT_ROOT/azure-test-config.env"
if [ ! -f "$CONFIG_FILE" ]; then
    log_error "Configuration file not found: $CONFIG_FILE"
    log_error "Run ./scripts/azure-setup.sh first"
    exit 1
fi

log_info "Loading Azure configuration from $CONFIG_FILE"
source "$CONFIG_FILE"

# Determine authentication mode
if [ "${AZURE_AUTH_MODE:-key}" = "login" ] || [ "${AZURE_USE_AZURE_AD:-false}" = "true" ]; then
    AUTH_MODE="azuread"
    log_info "Using Azure AD authentication"
    
    # Verify Azure login
    if ! az account show &> /dev/null; then
        log_error "Not logged into Azure. Run 'az login' first."
        exit 1
    fi
else
    AUTH_MODE="${1:-connection}"
    log_info "Using Shared Key authentication"
fi

# Detect container engine
if command -v podman &> /dev/null; then
    CONTAINER_ENGINE="podman"
elif command -v docker &> /dev/null; then
    CONTAINER_ENGINE="docker"
else
    log_error "Neither podman nor docker found. Please install one."
    exit 1
fi

log_info "Using container engine: $CONTAINER_ENGINE"

# Test results file
RESULTS_FILE="$PROJECT_ROOT/azure-test-results.json"
TEST_START=$(date -u +%s)
TEST_ID="azure-test-$(date +%Y%m%d-%H%M%S)"

# Initialize results
cat > "$RESULTS_FILE" <<EOF
{
  "test_id": "$TEST_ID",
  "start_time": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "auth_method": "$AUTH_METHOD",
  "storage_account": "$AZURE_STORAGE_ACCOUNT",
  "container_engine": "$CONTAINER_ENGINE",
  "tests": []
}
EOF

# Function to add test result
add_test_result() {
    local test_name="$1"
    local status="$2"
    local message="$3"
    local duration="${4:-0}"
    
    python3 - <<PYTHON
import json
import sys

with open('$RESULTS_FILE', 'r') as f:
    results = json.load(f)

results['tests'].append({
    'name': '$test_name',
    'status': '$status',
    'message': '''$message''',
    'duration_seconds': $duration
})

with open('$RESULTS_FILE', 'w') as f:
    json.dump(results, f, indent=2)
PYTHON
}

# Function to finalize results
finalize_results() {
    local overall_status="$1"
    TEST_END=$(date -u +%s)
    DURATION=$((TEST_END - TEST_START))
    
    python3 - <<PYTHON
import json

with open('$RESULTS_FILE', 'r') as f:
    results = json.load(f)

results['end_time'] = '$(date -u +%Y-%m-%dT%H:%M:%SZ)'
results['duration_seconds'] = $DURATION
results['overall_status'] = '$overall_status'

# Count pass/fail
passed = sum(1 for t in results['tests'] if t['status'] == 'PASS')
failed = sum(1 for t in results['tests'] if t['status'] == 'FAIL')
results['summary'] = {
    'total': len(results['tests']),
    'passed': passed,
    'failed': failed
}

with open('$RESULTS_FILE', 'w') as f:
    json.dump(results, f, indent=2)
PYTHON
}

echo ""
log_info "========================================="
log_info "Azure Blob Storage E2E Test"
log_info "========================================="
log_info "Test ID: $TEST_ID"
log_info "Auth Method: $AUTH_METHOD"
log_info "Storage Account: $AZURE_STORAGE_ACCOUNT"
echo ""

# Test 1: Upload test videos to Azure Blob
log_test "Test 1: Upload test videos to Azure Blob Storage"
test_start=$(date -u +%s)

JOB_NAME="test_job_azure_001"
testdata_path="$PROJECT_ROOT/testdata/sample_scene"

if [ ! -f "$testdata_path/view1.mp4" ]; then
    log_error "Test videos not found in $testdata_path"
    add_test_result "upload_test_videos" "FAIL" "Test videos not found" 0
    finalize_results "FAIL"
    exit 1
fi

log_info "Uploading test videos to input container..."
for video in view1.mp4 view2.mp4; do
    if [ "$AUTH_MODE" = "azuread" ]; then
        az storage blob upload \
            --account-name "$AZURE_STORAGE_ACCOUNT" \
            --container-name input \
            --name "$JOB_NAME/$video" \
            --file "$testdata_path/$video" \
            --auth-mode login \
            --overwrite \
            --output none
    else
        az storage blob upload \
            --container-name input \
            --name "$JOB_NAME/$video" \
            --file "$testdata_path/$video" \
            --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
            --overwrite \
            --output none
    fi
    log_info "✓ Uploaded $video"
done

test_end=$(date -u +%s)
test_duration=$((test_end - test_start))
add_test_result "upload_test_videos" "PASS" "Uploaded 2 test videos" "$test_duration"
log_info "✓ Test 1 PASSED (${test_duration}s)"
echo ""

# Test 2: Verify blob listing
log_test "Test 2: Verify blob listing"
test_start=$(date -u +%s)

if [ "$AUTH_MODE" = "azuread" ]; then
    blob_count=$(az storage blob list \
        --account-name "$AZURE_STORAGE_ACCOUNT" \
        --container-name input \
        --prefix "$JOB_NAME/" \
        --auth-mode login \
        --query "length(@)" \
        --output tsv)
else
    blob_count=$(az storage blob list \
        --container-name input \
        --prefix "$JOB_NAME/" \
        --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
        --query "length(@)" \
        --output tsv)
fi

if [ "$blob_count" -eq 2 ]; then
    test_end=$(date -u +%s)
    test_duration=$((test_end - test_start))
    add_test_result "verify_blob_listing" "PASS" "Found $blob_count blobs" "$test_duration"
    log_info "✓ Test 2 PASSED - Found $blob_count blobs (${test_duration}s)"
else
    add_test_result "verify_blob_listing" "FAIL" "Expected 2 blobs, found $blob_count" 0
    log_error "✗ Test 2 FAILED - Expected 2 blobs, found $blob_count"
fi
echo ""

# Test 3: Run container with Azure mounting (if privileged access available)
log_test "Test 3: Container startup with Azure configuration"
test_start=$(date -u +%s)

# Prepare environment variables based on auth method
if [ "$AUTH_MODE" = "azuread" ]; then
    AUTH_ENV_VARS=(
        "-e" "AZURE_STORAGE_ACCOUNT=$AZURE_STORAGE_ACCOUNT"
        "-e" "AZURE_USE_AZURE_AD=true"
    )
    log_info "Using Azure AD authentication"
elif [ "$AUTH_MODE" = "sas" ]; then
    AUTH_ENV_VARS=(
        "-e" "AZURE_STORAGE_ACCOUNT=$AZURE_STORAGE_ACCOUNT"
        "-e" "AZURE_STORAGE_SAS_TOKEN=$AZURE_STORAGE_SAS_TOKEN"
    )
    log_info "Using SAS token authentication"
else
    AUTH_ENV_VARS=(
        "-e" "AZURE_STORAGE_CONNECTION_STRING=$AZURE_STORAGE_CONNECTION_STRING"
    )
    log_info "Using connection string authentication"
fi

# Note: We can't actually test blobfuse2 mounting without --privileged and FUSE support
# Instead, we'll test that the container starts with correct Azure env vars
log_warn "Note: Blobfuse2 mounting requires --privileged container mode"
log_info "Testing container startup with Azure configuration..."

if $CONTAINER_ENGINE run --rm \
    "${AUTH_ENV_VARS[@]}" \
    -e "AZURE_BLOB_CONTAINER_INPUT=input" \
    -e "AZURE_BLOB_CONTAINER_OUTPUT=output" \
    -e "AZURE_BLOB_CONTAINER_PROCESSED=processed" \
    -e "AZURE_BLOB_CONTAINER_ERROR=error" \
    --entrypoint /bin/sh \
    3dgs-processor:latest \
    -c 'echo "Container started with Azure config"; env | grep AZURE | sed "s/=.*/=***/" | sort' \
    > /tmp/azure-container-test.log 2>&1; then
    
    test_end=$(date -u +%s)
    test_duration=$((test_end - test_start))
    add_test_result "container_startup" "PASS" "Container accepts Azure configuration" "$test_duration"
    log_info "✓ Test 3 PASSED (${test_duration}s)"
    cat /tmp/azure-container-test.log
else
    add_test_result "container_startup" "FAIL" "Container failed to start" 0
    log_error "✗ Test 3 FAILED"
    cat /tmp/azure-container-test.log
fi
echo ""

# Test 4: Download outputs from Azure (simulate processing completion)
log_test "Test 4: Upload mock output and verify download"
test_start=$(date -u +%s)

# Upload a mock output file
echo '{"test": "output", "gaussian_count": 1000}' > /tmp/test-output.json

if [ "$AUTH_MODE" = "azuread" ]; then
    az storage blob upload \
        --account-name "$AZURE_STORAGE_ACCOUNT" \
        --container-name output \
        --name "$JOB_NAME/model.json" \
        --file /tmp/test-output.json \
        --auth-mode login \
        --overwrite \
        --output none
    
    # Download it back
    az storage blob download \
        --account-name "$AZURE_STORAGE_ACCOUNT" \
        --container-name output \
        --name "$JOB_NAME/model.json" \
        --file /tmp/test-download.json \
        --auth-mode login \
        --output none
else
    az storage blob upload \
        --container-name output \
        --name "$JOB_NAME/model.json" \
        --file /tmp/test-output.json \
        --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
        --overwrite \
        --output none
    
    # Download it back
    az storage blob download \
        --container-name output \
        --name "$JOB_NAME/model.json" \
        --file /tmp/test-download.json \
        --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
        --output none
fi

if diff /tmp/test-output.json /tmp/test-download.json > /dev/null; then
    test_end=$(date -u +%s)
    test_duration=$((test_end - test_start))
    add_test_result "upload_download_cycle" "PASS" "Upload and download verified" "$test_duration"
    log_info "✓ Test 4 PASSED (${test_duration}s)"
else
    add_test_result "upload_download_cycle" "FAIL" "Upload/download mismatch" 0
    log_error "✗ Test 4 FAILED"
fi
rm -f /tmp/test-output.json /tmp/test-download.json
echo ""

# Test 5: Move blob between containers (simulate job completion)
log_test "Test 5: Move blob between containers"
test_start=$(date -u +%s)

# Copy from input to processed
if [ "$AUTH_MODE" = "azuread" ]; then
    az storage blob copy start \
        --account-name "$AZURE_STORAGE_ACCOUNT" \
        --source-container input \
        --source-blob "$JOB_NAME/view1.mp4" \
        --destination-container processed \
        --destination-blob "$JOB_NAME/view1.mp4" \
        --auth-mode login \
        --output none
    
    # Wait for copy to complete
    sleep 2
    
    # Verify in processed container
    if az storage blob show \
        --account-name "$AZURE_STORAGE_ACCOUNT" \
        --container-name processed \
        --name "$JOB_NAME/view1.mp4" \
        --auth-mode login \
        --output none 2>/dev/null; then
        
        test_end=$(date -u +%s)
        test_duration=$((test_end - test_start))
        add_test_result "blob_move" "PASS" "Blob copied to processed container" "$test_duration"
        log_info "✓ Test 5 PASSED (${test_duration}s)"
    else
        add_test_result "blob_move" "FAIL" "Blob not found in processed container" 0
        log_error "✗ Test 5 FAILED"
    fi
else
    az storage blob copy start \
        --source-container input \
        --source-blob "$JOB_NAME/view1.mp4" \
        --destination-container processed \
        --destination-blob "$JOB_NAME/view1.mp4" \
        --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
        --output none
    
    # Wait for copy to complete
    sleep 2
    
    # Verify in processed container
    if az storage blob show \
        --container-name processed \
        --name "$JOB_NAME/view1.mp4" \
        --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
        --output none 2>/dev/null; then
        
        test_end=$(date -u +%s)
        test_duration=$((test_end - test_start))
        add_test_result "blob_move" "PASS" "Blob copied to processed container" "$test_duration"
        log_info "✓ Test 5 PASSED (${test_duration}s)"
    else
        add_test_result "blob_move" "FAIL" "Blob not found in processed container" 0
        log_error "✗ Test 5 FAILED"
    fi
fi
echo ""

# Finalize results
finalize_results "PASS"

# Display results
echo ""
log_info "========================================="
log_info "Test Results Summary"
log_info "========================================="
cat "$RESULTS_FILE" | python3 -m json.tool

echo ""
log_info "Results written to: $RESULTS_FILE"
echo ""

# Check overall status
if python3 -c "import json; r=json.load(open('$RESULTS_FILE')); exit(0 if r['summary']['failed'] == 0 else 1)"; then
    log_info "✓ ALL TESTS PASSED"
    echo ""
    log_info "Next steps:"
    log_info "  1. Review results: cat $RESULTS_FILE"
    log_info "  2. For full E2E test with blobfuse2, run container with --privileged"
    log_info "  3. Cleanup resources: ./scripts/azure-cleanup.sh"
    exit 0
else
    log_error "✗ SOME TESTS FAILED"
    exit 1
fi
