#!/usr/bin/env bash
# azure-cleanup.sh - Clean up Azure resources created for testing
#
# This script deletes:
# - Blob containers (input, output, processed, error)
# - Storage account (optional)
# - Resource group (optional)
#
# Usage:
#   ./scripts/azure-cleanup.sh [storage_account] [resource_group] [--delete-all]
#
# Examples:
#   ./scripts/azure-cleanup.sh                    # Interactive, uses azure-test-config.env
#   ./scripts/azure-cleanup.sh mystore myrg       # Specify resources
#   ./scripts/azure-cleanup.sh mystore myrg --delete-all  # Delete everything

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DELETE_ALL=false

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
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

# Check for --delete-all flag
for arg in "$@"; do
    if [ "$arg" = "--delete-all" ]; then
        DELETE_ALL=true
    fi
done

# Check if Azure CLI is installed
if ! command -v az &> /dev/null; then
    log_error "Azure CLI not found"
    exit 1
fi

# Load from config file if exists and no args provided
if [ $# -eq 0 ] || [ "${1:-}" = "--delete-all" ]; then
    CONFIG_FILE="$PROJECT_ROOT/azure-test-config.env"
    if [ -f "$CONFIG_FILE" ]; then
        log_info "Loading configuration from $CONFIG_FILE"
        source "$CONFIG_FILE"
        STORAGE_ACCOUNT="${AZURE_STORAGE_ACCOUNT:-}"
        RESOURCE_GROUP="${AZURE_RESOURCE_GROUP:-}"
    else
        log_error "No config file found and no arguments provided"
        echo "Usage: $0 [storage_account] [resource_group] [--delete-all]"
        exit 1
    fi
else
    STORAGE_ACCOUNT="${1:-}"
    RESOURCE_GROUP="${2:-}"
fi

if [ -z "$STORAGE_ACCOUNT" ] || [ -z "$RESOURCE_GROUP" ]; then
    log_error "Storage account and resource group required"
    echo "Usage: $0 [storage_account] [resource_group] [--delete-all]"
    exit 1
fi

echo ""
log_info "========================================="
log_info "Azure Cleanup"
log_info "========================================="
log_info "Storage Account: $STORAGE_ACCOUNT"
log_info "Resource Group:  $RESOURCE_GROUP"
if [ "$DELETE_ALL" = true ]; then
    log_warn "Mode: DELETE ALL (including storage account and resource group)"
else
    log_info "Mode: Clean containers only (keep storage account)"
fi
echo ""

# Get connection string if not already set
AZURE_AUTH_MODE="${AZURE_AUTH_MODE:-key}"

if [ -z "${AZURE_STORAGE_CONNECTION_STRING:-}" ] && [ "$AZURE_AUTH_MODE" != "login" ]; then
    log_info "Retrieving connection string..."
    AZURE_STORAGE_CONNECTION_STRING=$(az storage account show-connection-string \
        --name "$STORAGE_ACCOUNT" \
        --resource-group "$RESOURCE_GROUP" \
        --query connectionString \
        --output tsv 2>/dev/null || echo "")
    
    if [ -z "$AZURE_STORAGE_CONNECTION_STRING" ]; then
        log_warn "Could not retrieve connection string, trying Azure AD auth..."
        AZURE_AUTH_MODE="login"
    fi
fi

# Determine auth mode
if [ "${AZURE_USE_AZURE_AD:-false}" = "true" ] || [ "$AZURE_AUTH_MODE" = "login" ]; then
    log_info "Using Azure AD authentication for cleanup"
    USE_AZURE_AD=true
else
    USE_AZURE_AD=false
fi

# Delete blob containers
log_info "Deleting blob containers..."
for container in input output processed error; do
    if [ "$USE_AZURE_AD" = true ]; then
        # Use Azure AD auth
        if az storage container show \
            --account-name "$STORAGE_ACCOUNT" \
            --name "$container" \
            --auth-mode login &> /dev/null; then
            
            # List blobs in container
            blob_count=$(az storage blob list \
                --account-name "$STORAGE_ACCOUNT" \
                --container-name "$container" \
                --auth-mode login \
                --query "length(@)" \
                --output tsv 2>/dev/null || echo "0")
            
            log_info "Deleting container '$container' ($blob_count blobs)..."
            az storage container delete \
                --account-name "$STORAGE_ACCOUNT" \
                --name "$container" \
                --auth-mode login \
                --output none
            log_info "✓ Deleted container '$container'"
        else
            log_warn "Container '$container' not found, skipping"
        fi
    else
        # Use connection string
        if [ -n "$AZURE_STORAGE_CONNECTION_STRING" ]; then
            if az storage container show \
                --name "$container" \
                --connection-string "$AZURE_STORAGE_CONNECTION_STRING" &> /dev/null; then
                
                # List blobs in container
                blob_count=$(az storage blob list \
                    --container-name "$container" \
                    --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
                    --query "length(@)" \
                    --output tsv 2>/dev/null || echo "0")
                
                log_info "Deleting container '$container' ($blob_count blobs)..."
                az storage container delete \
                    --name "$container" \
                    --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
                    --output none
                log_info "✓ Deleted container '$container'"
            else
                log_warn "Container '$container' not found, skipping"
            fi
        else
            log_warn "No connection string available, skipping container deletion"
            break
        fi
    fi
done

# Delete storage account if --delete-all
if [ "$DELETE_ALL" = true ]; then
    log_warn "Deleting storage account '$STORAGE_ACCOUNT'..."
    read -p "Are you sure? This cannot be undone. (yes/no): " confirm
    if [ "$confirm" = "yes" ]; then
        if az storage account show \
            --name "$STORAGE_ACCOUNT" \
            --resource-group "$RESOURCE_GROUP" &> /dev/null; then
            
            az storage account delete \
                --name "$STORAGE_ACCOUNT" \
                --resource-group "$RESOURCE_GROUP" \
                --yes \
                --output none
            log_info "✓ Deleted storage account"
        else
            log_warn "Storage account not found, skipping"
        fi
        
        # Ask about resource group
        log_warn "Delete resource group '$RESOURCE_GROUP'?"
        read -p "This will delete ALL resources in the group. (yes/no): " confirm_rg
        if [ "$confirm_rg" = "yes" ]; then
            if az group show --name "$RESOURCE_GROUP" &> /dev/null; then
                az group delete \
                    --name "$RESOURCE_GROUP" \
                    --yes \
                    --output none
                log_info "✓ Deleted resource group"
            else
                log_warn "Resource group not found, skipping"
            fi
        fi
    else
        log_info "Deletion cancelled"
    fi
fi

# Remove local config file
CONFIG_FILE="$PROJECT_ROOT/azure-test-config.env"
if [ -f "$CONFIG_FILE" ]; then
    log_info "Removing local configuration file..."
    rm -f "$CONFIG_FILE"
    log_info "✓ Deleted $CONFIG_FILE"
fi

# Remove test results
RESULTS_FILE="$PROJECT_ROOT/azure-test-results.json"
if [ -f "$RESULTS_FILE" ]; then
    log_info "Removing test results file..."
    rm -f "$RESULTS_FILE"
    log_info "✓ Deleted $RESULTS_FILE"
fi

echo ""
log_info "========================================="
log_info "Cleanup Complete"
log_info "========================================="
echo ""

if [ "$DELETE_ALL" = true ]; then
    log_info "All Azure resources have been deleted"
else
    log_info "Blob containers cleaned"
    log_info "Storage account preserved: $STORAGE_ACCOUNT"
    log_info ""
    log_info "To delete storage account and resource group:"
    log_info "  ./scripts/azure-cleanup.sh $STORAGE_ACCOUNT $RESOURCE_GROUP --delete-all"
fi
