//! Azure Blob Storage integration
//!
//! This module handles Azure Blob Storage for the 3DGS processor via two mechanisms:
//!
//! - **BlobFuse2** (watch mode): Mounts Azure Blob containers as local filesystem paths
//! - **SDK** (batch mode): Direct Azure Blob Storage API access via the official Rust SDK
//!
//! Authentication methods supported:
//! - Connection string (BlobFuse2 only)
//! - SAS token (BlobFuse2 only)
//! - Managed Identity (both)
//! - DefaultAzureCredential / Azure CLI (SDK only)

mod auth;
mod mount;
pub mod sdk;

pub use auth::{AzureAuth, AzureAuthMethod};
pub use mount::{mount_all_containers, unmount_all_containers, verify_mounts, MountConfig};
pub use sdk::BlobStorageClient;
