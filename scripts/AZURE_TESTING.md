# Azure Blob Storage Testing Guide

This directory contains scripts for end-to-end testing of the 3DGS processor with Azure Blob Storage.

## Overview

The Azure testing workflow consists of three phases:

1. **Setup** - Create Azure resources (storage account, containers)
2. **Test** - Upload test data, run processor, validate outputs
3. **Cleanup** - Remove Azure resources

## Prerequisites

### Required Tools

- **Azure CLI** - Install from [docs.microsoft.com/cli/azure/install-azure-cli](https://docs.microsoft.com/cli/azure/install-azure-cli)
- **Docker or Podman** - Container runtime
- **Python 3** - For test result processing
- **Container image built** - Run `./docker-build-podman.sh` first

### Azure Account

- Active Azure subscription (free tier works)
- Logged in: `az login`
- Sufficient permissions to create storage accounts

## Quick Start

### 1. Setup Azure Resources

```bash
# Create storage account with auto-generated name
./scripts/azure-setup.sh

# Or specify custom names
./scripts/azure-setup.sh mystorageaccount my-resource-group eastus
```

This creates:
- ✅ Azure Storage Account (Standard_LRS)
- ✅ Four blob containers: `input`, `output`, `processed`, `error`
- ✅ Connection string and SAS token
- ✅ Configuration file: `azure-test-config.env`

**Output:**
```
Azure Storage Setup for 3DGS Processor
=========================================
Storage Account: 3dgstest1709051234
Resource Group:  3dgs-test-rg
Location:        eastus

✓ Resource group created
✓ Storage account created
✓ Container 'input' created
✓ Container 'output' created
✓ Container 'processed' created
✓ Container 'error' created
✓ Configuration saved (permissions: 600)

To use this configuration:
  source azure-test-config.env

To run tests:
  ./scripts/azure-test.sh
```

### 2. Run Tests

```bash
# Load configuration
source azure-test-config.env

# Test with connection string (default)
./scripts/azure-test.sh

# Or test with SAS token
./scripts/azure-test.sh sas
```

**What gets tested:**
1. ✅ Upload test videos to Azure Blob Storage
2. ✅ Verify blob listing
3. ✅ Container startup with Azure configuration
4. ✅ Upload/download cycle
5. ✅ Blob movement between containers

**Test Results:**
```json
{
  "test_id": "azure-test-20260227-143022",
  "start_time": "2026-02-27T14:30:22Z",
  "end_time": "2026-02-27T14:30:45Z",
  "duration_seconds": 23,
  "auth_method": "connection",
  "storage_account": "3dgstest1709051234",
  "container_engine": "podman",
  "overall_status": "PASS",
  "summary": {
    "total": 5,
    "passed": 5,
    "failed": 0
  },
  "tests": [
    {
      "name": "upload_test_videos",
      "status": "PASS",
      "message": "Uploaded 2 test videos",
      "duration_seconds": 5
    },
    ...
  ]
}
```

Results saved to: `azure-test-results.json`

### 3. Cleanup

```bash
# Clean blob containers only (keep storage account)
./scripts/azure-cleanup.sh

# Or delete everything (storage account + resource group)
./scripts/azure-cleanup.sh --delete-all
```

## Detailed Usage

### azure-setup.sh

Creates Azure infrastructure for testing.

```bash
./scripts/azure-setup.sh [storage_account] [resource_group] [location]
```

**Parameters:**
- `storage_account` - Storage account name (default: auto-generated with timestamp)
- `resource_group` - Resource group name (default: `<your-resource-group>`)
- `location` - Azure region (default: `eastus`)

**Output files:**
- `azure-test-config.env` - Environment variables for testing (⚠️ contains credentials)

**Environment Variables:**
```bash
export AZURE_STORAGE_ACCOUNT="..."
export AZURE_STORAGE_CONNECTION_STRING="..."
export AZURE_STORAGE_SAS_TOKEN="..."
export AZURE_BLOB_CONTAINER_INPUT="input"
export AZURE_BLOB_CONTAINER_OUTPUT="output"
export AZURE_BLOB_CONTAINER_PROCESSED="processed"
export AZURE_BLOB_CONTAINER_ERROR="error"
```

### azure-test.sh

Runs end-to-end tests with Azure Blob Storage.

```bash
./scripts/azure-test.sh [connection|sas]
```

**Parameters:**
- `connection` - Use connection string authentication (default)
- `sas` - Use SAS token authentication

**Prerequisites:**
- `azure-test-config.env` exists (created by `azure-setup.sh`)
- Test videos in `testdata/sample_scene/` (view1.mp4, view2.mp4)
- Container image built: `3dgs-processor:latest`

**Test Flow:**
1. Load Azure credentials from config
2. Upload test videos to `input` container
3. Verify blobs are accessible
4. Test container startup with Azure env vars
5. Simulate processing (upload/download outputs)
6. Test blob movement (input → processed)
7. Write results to `azure-test-results.json`

### azure-cleanup.sh

Removes Azure resources created during testing.

```bash
./scripts/azure-cleanup.sh [storage_account] [resource_group] [--delete-all]
```

**Modes:**

**Default** - Clean containers only:
```bash
./scripts/azure-cleanup.sh
```
- Deletes blob containers (input, output, processed, error)
- Keeps storage account (reusable for next test)
- Removes local config files

**Delete All** - Remove everything:
```bash
./scripts/azure-cleanup.sh --delete-all
```
- Deletes blob containers
- Deletes storage account (⚠️ cannot be undone)
- Optionally deletes resource group
- Removes local config files

## Authentication Methods

### 1. Connection String (Recommended for Testing)

Full access to storage account.

```bash
export AZURE_STORAGE_CONNECTION_STRING="DefaultEndpointsProtocol=https;AccountName=..."
```

**Pros:** ✅ Simple, full access, easy to test  
**Cons:** ⚠️ Highest privilege, must protect credentials  
**Availability:** ❌ Not available if shared key access is disabled

### 2. SAS Token (Limited Access)

Time-limited, scoped permissions.

```bash
export AZURE_STORAGE_ACCOUNT="mystorageaccount"
export AZURE_STORAGE_SAS_TOKEN="sv=2021-06-08&ss=b&srt=sco..."
```

**Pros:** ✅ Time-limited (7 day expiry), scoped permissions  
**Cons:** ⚠️ Requires storage account name + token  
**Availability:** ⚠️ Account SAS requires shared key access; User delegation SAS works with Azure AD

### 3. Azure AD Authentication (Enterprise/Production) ⭐ NEW

Azure Active Directory authentication with role-based access control.

```bash
export AZURE_STORAGE_ACCOUNT="mystorageaccount"
export AZURE_USE_AZURE_AD="true"
# Requires: az login + Storage Blob Data Contributor role
```

**Pros:** ✅ No shared keys, enterprise-compliant, audit trail, identity-based  
**Cons:** ⚠️ Requires Azure AD login, role assignment  
**Availability:** ✅ **Works when shared key access is disabled**

### 4. Managed Identity (Production)

Azure-managed credentials (no secrets in env vars).

```bash
export AZURE_STORAGE_ACCOUNT="mystorageaccount"
export AZURE_USE_MANAGED_IDENTITY="true"
```

**Pros:** ✅ No credentials, Azure-managed, most secure  
**Cons:** ⚠️ Only works in Azure (VM/ACI), not testable locally

**Note:** Managed Identity requires Azure Container Instances or VM deployment. Not covered by these scripts.

## Enterprise Environments with Shared Key Disabled

If your organization has disabled shared key access (error: `Key based authentication is not permitted on this storage account`), the scripts automatically detect this and use **Azure AD authentication** instead.

### What Happens Automatically

1. **Setup script** detects shared key is disabled
2. Grants you **Storage Blob Data Contributor** role
3. Uses `--auth-mode login` for all operations
4. Generates **user delegation SAS** (Azure AD-based) instead of account SAS

### Requirements

- Azure CLI installed and logged in: `az login`
- Sufficient permissions to assign roles (or admin pre-assigns the role)
- Storage Blob Data Contributor role on the storage account

### Testing with Azure AD

```bash
# 1. Login to Azure
az login

# 2. Setup will automatically use Azure AD
./scripts/azure-setup.sh

# Output will show:
# Shared key access allowed: false
# Using: Azure AD (Shared Key disabled)

# 3. Run tests (works the same)
source azure-test-config.env
./scripts/azure-test.sh

# 4. Cleanup (works the same)
./scripts/azure-cleanup.sh
```

### Differences from Shared Key Mode

| Feature | Shared Key Mode | Azure AD Mode |
|---------|-----------------|---------------|
| Connection string | ✅ Available | ❌ Not available |
| Account SAS | ✅ Works | ❌ Blocked |
| User delegation SAS | ⚠️ Optional | ✅ Required for SAS |
| Authentication | Account key | Azure AD identity |
| Audit trail | Limited | Full Azure AD audit |
| Setup command | Same | Same (auto-detects) |
| Test command | Same | Same (auto-detects) |
| Cleanup command | Same | Same (auto-detects) |

## Cost Estimation

### Storage Account (Standard_LRS)

- **Storage:** $0.0184/GB/month
- **Transactions:** $0.004 per 10,000 operations
- **Data transfer:** Egress after 5GB charged

### Test Run Costs

Typical test run (5-10 minutes):
- Storage: ~0.5 GB = **$0.001/month**
- Transactions: ~500 operations = **$0.0002**
- **Total per test:** < $0.01

### Free Tier

Azure Free Tier includes:
- ✅ 5 GB storage for 12 months
- ✅ 20,000 read operations
- ✅ 10,000 write operations

**Recommendation:** Use free tier for development testing.

## Limitations

### What These Scripts Test

✅ Azure authentication methods (connection string, SAS token)  
✅ Blob upload/download operations  
✅ Container listing and navigation  
✅ Container startup with Azure configuration  
✅ Blob movement between containers  

### What's NOT Tested (Requires --privileged Container)

❌ **Blobfuse2 FUSE mounting** - Requires privileged container mode  
❌ **File watching on mounted blobs** - Requires FUSE  
❌ **Direct filesystem operations on blobs** - Requires FUSE  
❌ **Real 3DGS processing with Azure storage** - Requires GPU + FUSE  

### Full E2E Testing (Manual)

To test blobfuse2 mounting and real processing:

```bash
# Load Azure config
source azure-test-config.env

# Run container with privileged mode (required for FUSE)
podman run --rm --privileged \
  -e AZURE_STORAGE_CONNECTION_STRING="$AZURE_STORAGE_CONNECTION_STRING" \
  -e AZURE_BLOB_CONTAINER_INPUT="input" \
  -e AZURE_BLOB_CONTAINER_OUTPUT="output" \
  -e AZURE_BLOB_CONTAINER_PROCESSED="processed" \
  -e AZURE_BLOB_CONTAINER_ERROR="error" \
  -e INPUT_PATH="/mnt/input" \
  -e OUTPUT_PATH="/mnt/output" \
  -e PROCESSED_PATH="/mnt/processed" \
  -e ERROR_PATH="/mnt/error" \
  -e BACKEND="mock" \
  -e LOG_LEVEL="debug" \
  3dgs-processor:latest
```

**Requirements:**
- Linux host or VM (macOS containers don't support FUSE)
- `--privileged` flag
- Blobfuse2 installed in container
- Azure credentials

## Troubleshooting

### Azure CLI Issues

**Error:** `az: command not found`

```bash
# Install Azure CLI
# macOS
brew install azure-cli

# Ubuntu/Debian
curl -sL https://aka.ms/InstallAzureCLIDeb | sudo bash
```

**Error:** `Please run 'az login' to setup account`

```bash
az login
# Follow browser authentication flow
```

### Storage Account Issues

**Error:** `The storage account name 'xyz' is already taken`

Storage account names must be globally unique. The setup script auto-generates unique names with timestamps.

```bash
# Use custom name with your initials/org
./scripts/azure-setup.sh myorg3dgs$(date +%s) my-rg eastus
```

**Error:** `StorageAccountNotFound`

Storage account may have been deleted. Re-run setup:

```bash
./scripts/azure-setup.sh
```

**Error:** `Key based authentication is not permitted on this storage account` ⭐ NEW

Your organization has disabled shared key access for security compliance. This is **automatically handled** by the scripts.

**Solution:** The scripts detect this and switch to Azure AD authentication automatically. No action needed!

```bash
# Just run setup normally
./scripts/azure-setup.sh

# Output will show:
# Shared key access allowed: false
# Using Azure AD authentication instead
# ✓ Using Azure AD authentication
```

If you see this and the script fails, ensure:
1. You're logged in: `az login`
2. You have proper role: Ask admin to grant "Storage Blob Data Contributor" role
3. The role assignment propagates (can take 5-10 minutes)

**Manual role assignment (if needed):**
```bash
az role assignment create \
  --role "Storage Blob Data Contributor" \
  --assignee $(az ad signed-in-user show --query id -o tsv) \
  --scope "/subscriptions/<sub-id>/resourceGroups/<rg>/providers/Microsoft.Storage/storageAccounts/<account>"
```

### Permission Issues

**Error:** `AuthorizationFailed`

Your Azure account lacks permissions. Need `Storage Account Contributor` role:

```bash
# Check current roles
az role assignment list --assignee $(az account show --query user.name -o tsv)

# Request access from Azure admin
```

### Container Issues

**Error:** `Container image '3dgs-processor:latest' not found`

Build the container first:

```bash
./docker-build-podman.sh
```

**Error:** `Cannot connect to container engine`

Start Docker/Podman daemon:

```bash
# macOS
open -a Docker

# Linux (Podman)
sudo systemctl start podman
```

## Security Best Practices

### 1. Protect Credentials

```bash
# Config file has restricted permissions (600)
ls -l azure-test-config.env
# -rw------- 1 user user 1234 Feb 27 14:30 azure-test-config.env

# Never commit to git
echo "azure-test-config.env" >> .gitignore
echo "azure-test-results.json" >> .gitignore
```

### 2. Use Time-Limited Tokens

SAS tokens expire after 7 days (configurable in setup script).

```bash
# Check expiry
grep EXPIRY azure-test-config.env
```

### 3. Cleanup After Testing

```bash
# Delete test data
./scripts/azure-cleanup.sh

# Or delete everything
./scripts/azure-cleanup.sh --delete-all
```

### 4. Rotate Credentials

```bash
# Regenerate storage account keys periodically
az storage account keys renew \
  --account-name mystorageaccount \
  --resource-group my-rg \
  --key primary
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Azure E2E Tests

on: [push, pull_request]

jobs:
  azure-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Azure Login
        uses: azure/login@v1
        with:
          creds: ${{ secrets.AZURE_CREDENTIALS }}
      
      - name: Setup Azure Resources
        run: ./scripts/azure-setup.sh
      
      - name: Build Container
        run: ./docker-build-podman.sh
      
      - name: Run Tests
        run: |
          source azure-test-config.env
          ./scripts/azure-test.sh
      
      - name: Upload Results
        uses: actions/upload-artifact@v3
        with:
          name: azure-test-results
          path: azure-test-results.json
      
      - name: Cleanup
        if: always()
        run: ./scripts/azure-cleanup.sh --delete-all
```

**Required Secrets:**
- `AZURE_CREDENTIALS` - Service principal JSON

## Next Steps

After running these tests successfully:

1. ✅ **Update TODO.md** - Mark Azure integration tests as complete
2. ✅ **Document in DEPLOYMENT.md** - Add real Azure deployment examples
3. ✅ **Production testing** - Deploy to Azure Container Instances
4. ✅ **Add to CI/CD** - Automate Azure testing in GitHub Actions

## Support

Issues or questions:
- Check [TROUBLESHOOTING.md](../docs/TROUBLESHOOTING.md)
- Review Azure CLI docs: [docs.microsoft.com/cli/azure](https://docs.microsoft.com/cli/azure)
- File issue: [GitHub Issues](https://github.com/azure-samples/3DGS-accelerator/issues)
