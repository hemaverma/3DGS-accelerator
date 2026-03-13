#!/usr/bin/env bash
# azure-setup.sh - Set up Azure Storage Account for 3DGS Processor testing
#
# This script creates:
# - Azure Storage Account
# - Four blob containers (input, output, processed, error)
# - Outputs connection string for testing
#
# Usage:
#   ./scripts/azure-setup.sh [storage_account_name] [resource_group] [location]
#
# Example:
#   ./scripts/azure-setup.sh 3dgsteststore test-3dgs-rg eastus

set -euo pipefail

# Default values
STORAGE_ACCOUNT="${1:-3dgstest$(date +%s)}"  # Append timestamp for uniqueness
RESOURCE_GROUP="${2:-3dgs-test-rg}"
LOCATION="${3:-eastus}"

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

# Check if Azure CLI is installed
if ! command -v az &> /dev/null; then
    log_error "Azure CLI not found. Install from: https://docs.microsoft.com/cli/azure/install-azure-cli"
    exit 1
fi

# Check if logged in
if ! az account show &> /dev/null; then
    log_error "Not logged into Azure. Run 'az login' first."
    exit 1
fi

log_info "Azure Storage Setup for 3DGS Processor"
log_info "========================================="
log_info "Storage Account: $STORAGE_ACCOUNT"
log_info "Resource Group:  $RESOURCE_GROUP"
log_info "Location:        $LOCATION"
echo ""

# Create resource group
log_info "Creating resource group..."
if az group show --name "$RESOURCE_GROUP" &> /dev/null; then
    log_warn "Resource group '$RESOURCE_GROUP' already exists, using existing"
else
    az group create \
        --name "$RESOURCE_GROUP" \
        --location "$LOCATION" \
        --output none
    log_info "✓ Resource group created"
fi

# Create storage account
log_info "Creating storage account '$STORAGE_ACCOUNT'..."
if az storage account show --name "$STORAGE_ACCOUNT" --resource-group "$RESOURCE_GROUP" &> /dev/null; then
    log_warn "Storage account '$STORAGE_ACCOUNT' already exists, using existing"
else
    az storage account create \
        --name "$STORAGE_ACCOUNT" \
        --resource-group "$RESOURCE_GROUP" \
        --location "$LOCATION" \
        --sku Standard_LRS \
        --kind StorageV2 \
        --allow-blob-public-access false \
        --min-tls-version TLS1_2 \
        --output none
    log_info "✓ Storage account created"
fi

# Check if shared key access is allowed
log_info "Checking storage account authentication settings..."
SHARED_KEY_ALLOWED=$(az storage account show \
    --name "$STORAGE_ACCOUNT" \
    --resource-group "$RESOURCE_GROUP" \
    --query allowSharedKeyAccess \
    --output tsv)

# Default to true if not explicitly set
if [ -z "$SHARED_KEY_ALLOWED" ] || [ "$SHARED_KEY_ALLOWED" = "None" ]; then
    SHARED_KEY_ALLOWED="true"
fi

log_info "Shared key access allowed: $SHARED_KEY_ALLOWED"

# Variables for auth
CONNECTION_STRING=""
STORAGE_KEY=""
AUTH_MODE="key"

if [ "$SHARED_KEY_ALLOWED" = "true" ]; then
    # Get connection string and storage key
    log_info "Retrieving connection string and storage key..."
    CONNECTION_STRING=$(az storage account show-connection-string \
        --name "$STORAGE_ACCOUNT" \
        --resource-group "$RESOURCE_GROUP" \
        --query connectionString \
        --output tsv)

    STORAGE_KEY=$(az storage account keys list \
        --account-name "$STORAGE_ACCOUNT" \
        --resource-group "$RESOURCE_GROUP" \
        --query '[0].value' \
        --output tsv)
    AUTH_MODE="key"
else
    log_warn "Shared key authentication is DISABLED on this storage account"
    log_info "Using Azure AD authentication instead"
    AUTH_MODE="login"
    
    # Check if user has required role
    log_info "Checking Azure AD permissions..."
    CURRENT_USER=$(az ad signed-in-user show --query id --output tsv 2>/dev/null || echo "")
    
    if [ -z "$CURRENT_USER" ]; then
        log_error "Unable to get current user. Ensure you're logged in: az login"
        exit 1
    fi
    
    # Grant Storage Blob Data Contributor role if not already assigned
    log_info "Ensuring you have Storage Blob Data Contributor role..."
    ACCOUNT_SCOPE="/subscriptions/$(az account show --query id -o tsv)/resourceGroups/$RESOURCE_GROUP/providers/Microsoft.Storage/storageAccounts/$STORAGE_ACCOUNT"
    
    az role assignment create \
        --role "Storage Blob Data Contributor" \
        --assignee "$CURRENT_USER" \
        --scope "$ACCOUNT_SCOPE" \
        --output none 2>/dev/null || log_info "Role already assigned or unable to assign (may need admin)"
    
    log_info "✓ Using Azure AD authentication"
fi

# Create blob containers
log_info "Creating blob containers..."
for container in input output processed error; do
    if [ "$AUTH_MODE" = "key" ]; then
        if az storage container show \
            --name "$container" \
            --connection-string "$CONNECTION_STRING" &> /dev/null; then
            log_warn "Container '$container' already exists, skipping"
        else
            az storage container create \
                --name "$container" \
                --connection-string "$CONNECTION_STRING" \
                --output none
            log_info "✓ Container '$container' created"
        fi
    else
        # Use Azure AD auth
        if az storage container show \
            --name "$container" \
            --account-name "$STORAGE_ACCOUNT" \
            --auth-mode login &> /dev/null; then
            log_warn "Container '$container' already exists, skipping"
        else
            az storage container create \
                --name "$container" \
                --account-name "$STORAGE_ACCOUNT" \
                --auth-mode login \
                --output none
            log_info "✓ Container '$container' created"
        fi
    fi
done

# Generate SAS token
SAS_TOKEN=""
EXPIRY=$(date -u -d '7 days' '+%Y-%m-%dT%H:%MZ' 2>/dev/null || date -u -v+7d '+%Y-%m-%dT%H:%MZ')

if [ "$AUTH_MODE" = "key" ]; then
    # Generate account SAS with storage key
    log_info "Generating Account SAS token (7 day expiry)..."
    SAS_TOKEN=$(az storage account generate-sas \
        --account-name "$STORAGE_ACCOUNT" \
        --account-key "$STORAGE_KEY" \
        --services b \
        --resource-types sco \
        --permissions acdlpruw \
        --expiry "$EXPIRY" \
        --https-only \
        --output tsv)
else
    # Generate user delegation SAS with Azure AD
    log_warn "User delegation SAS generation for individual containers..."
    log_info "Note: User delegation SAS is per-container, not account-wide"
    
    # We'll generate one for the input container as an example
    # In the test script, we'll use Azure AD auth directly instead of SAS
    SAS_TOKEN=$(az storage container generate-sas \
        --account-name "$STORAGE_ACCOUNT" \
        --name input \
        --permissions racwdl \
        --expiry "$EXPIRY" \
        --auth-mode login \
        --as-user \
        --https-only \
        --output tsv 2>/dev/null || echo "")
    
    if [ -z "$SAS_TOKEN" ]; then
        log_warn "Unable to generate user delegation SAS (requires permission)"
        log_info "Will use Azure AD authentication directly in tests"
    fi
fi

# Save configuration to file
CONFIG_FILE="azure-test-config.env"
log_info "Saving configuration to $CONFIG_FILE..."

cat > "$CONFIG_FILE" <<EOF
# Azure Storage Configuration for 3DGS Processor Testing
# Generated: $(date)
# Resource Group: $RESOURCE_GROUP
# Location: $LOCATION
# Authentication Mode: $AUTH_MODE

# Storage Account
export AZURE_STORAGE_ACCOUNT="$STORAGE_ACCOUNT"
export AZURE_RESOURCE_GROUP="$RESOURCE_GROUP"
export AZURE_AUTH_MODE="$AUTH_MODE"

EOF

if [ "$AUTH_MODE" = "key" ]; then
    cat >> "$CONFIG_FILE" <<EOF
# Shared Key Authentication (Connection String)
export AZURE_STORAGE_CONNECTION_STRING="$CONNECTION_STRING"

# SAS Token (expires: $EXPIRY)
export AZURE_STORAGE_SAS_TOKEN="$SAS_TOKEN"
EOF
else
    cat >> "$CONFIG_FILE" <<EOF
# Azure AD Authentication
# Note: Shared key authentication is DISABLED on this storage account
# Using Azure AD (requires 'az login' and Storage Blob Data Contributor role)
export AZURE_USE_AZURE_AD="true"

EOF
    if [ -n "$SAS_TOKEN" ]; then
        cat >> "$CONFIG_FILE" <<EOF
# User Delegation SAS Token (input container only, expires: $EXPIRY)
export AZURE_STORAGE_SAS_TOKEN="$SAS_TOKEN"
EOF
    else
        cat >> "$CONFIG_FILE" <<EOF
# User Delegation SAS not available
# Will use Azure AD authentication directly
EOF
    fi
fi

cat >> "$CONFIG_FILE" <<EOF

# Container names
export AZURE_BLOB_CONTAINER_INPUT="input"
export AZURE_BLOB_CONTAINER_OUTPUT="output"
export AZURE_BLOB_CONTAINER_PROCESSED="processed"
export AZURE_BLOB_CONTAINER_ERROR="error"

# Mount paths (for container)
export INPUT_PATH="/mnt/input"
export OUTPUT_PATH="/mnt/output"
export PROCESSED_PATH="/mnt/processed"
export ERROR_PATH="/mnt/error"
EOF

chmod 600 "$CONFIG_FILE"
log_info "✓ Configuration saved (permissions: 600)"

echo ""
log_info "========================================="
log_info "Setup Complete!"
log_info "========================================="
echo ""
log_info "Authentication Mode: $AUTH_MODE"
if [ "$AUTH_MODE" = "key" ]; then
    log_info "Using: Connection String + SAS Token"
    log_warn "SAS token expires on $EXPIRY"
else
    log_info "Using: Azure AD (Shared Key disabled)"
    log_warn "Ensure you stay logged in: az login"
fi
echo ""
log_info "To use this configuration:"
echo "  source $CONFIG_FILE"
echo ""
log_info "To run tests:"
echo "  ./scripts/azure-test.sh"
echo ""
log_info "To cleanup:"
echo "  ./scripts/azure-cleanup.sh $STORAGE_ACCOUNT $RESOURCE_GROUP"
echo ""
log_warn "Keep $CONFIG_FILE secure - it contains credentials"
