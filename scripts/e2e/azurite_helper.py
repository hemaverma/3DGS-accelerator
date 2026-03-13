#!/usr/bin/env python3
"""Azurite helper for E2E batch mode testing.

Manages Azurite blob storage for batch mode E2E tests:
- Creates containers (input, output, processed, error)
- Uploads test videos to the input container
- Generates SAS tokens for the Rust processor
- Verifies output blobs after processing
- Cleans up containers

Usage:
    python3 azurite_helper.py setup <video_dir> <prefix>
    python3 azurite_helper.py verify <prefix>
    python3 azurite_helper.py cleanup
    python3 azurite_helper.py sas
"""

import os
import sys
from datetime import datetime, timedelta, timezone

from azure.storage.blob import (
    BlobServiceClient,
    generate_account_sas,
    ResourceTypes,
    AccountSasPermissions,
)

ACCOUNT_NAME = "devstoreaccount1"
ACCOUNT_KEY = "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw=="
BLOB_ENDPOINT = "http://127.0.0.1:10000/devstoreaccount1"
CONN_STR = f"DefaultEndpointsProtocol=http;AccountName={ACCOUNT_NAME};AccountKey={ACCOUNT_KEY};BlobEndpoint={BLOB_ENDPOINT};"

CONTAINERS = ["input", "output", "processed", "error"]


def get_client() -> BlobServiceClient:
    return BlobServiceClient.from_connection_string(CONN_STR)


def generate_sas() -> str:
    return generate_account_sas(
        account_name=ACCOUNT_NAME,
        account_key=ACCOUNT_KEY,
        resource_types=ResourceTypes(service=True, container=True, object=True),
        permission=AccountSasPermissions(
            read=True, write=True, delete=True, list=True, add=True, create=True
        ),
        expiry=datetime.now(timezone.utc) + timedelta(hours=24),
    )


def cmd_setup(video_dir: str, prefix: str):
    """Create containers and upload test videos."""
    client = get_client()

    # Create containers
    for name in CONTAINERS:
        try:
            client.create_container(name)
        except Exception as e:
            if "ContainerAlreadyExists" not in str(e):
                raise

    # Upload videos
    container = client.get_container_client("input")
    count = 0
    for fname in sorted(os.listdir(video_dir)):
        if fname.endswith(".mp4"):
            blob_name = f"{prefix}{fname}"
            fpath = os.path.join(video_dir, fname)
            with open(fpath, "rb") as f:
                container.upload_blob(blob_name, f, overwrite=True)
            size_mb = os.path.getsize(fpath) / (1024 * 1024)
            print(f"  Uploaded {blob_name} ({size_mb:.1f} MB)")
            count += 1

    print(f"  Total: {count} videos uploaded to input/{prefix}")


def cmd_verify(prefix: str) -> bool:
    """Verify outputs exist and inputs were moved to processed."""
    client = get_client()
    ok = True

    # Check output container
    output_container = client.get_container_client("output")
    output_blobs = list(output_container.list_blobs(name_starts_with=prefix))
    output_names = [b.name for b in output_blobs]

    has_ply = any(n.endswith(".ply") for n in output_names)
    has_splat = any(n.endswith(".splat") for n in output_names)
    has_manifest = any("manifest.json" in n for n in output_names)

    if has_ply:
        ply_blob = next(b for b in output_blobs if b.name.endswith(".ply"))
        print(f"  ✅ PLY:          {ply_blob.name} ({ply_blob.size} bytes)")
    else:
        print("  ❌ PLY:          MISSING")
        ok = False

    if has_splat:
        splat_blob = next(b for b in output_blobs if b.name.endswith(".splat"))
        print(f"  ✅ SPLAT:        {splat_blob.name} ({splat_blob.size} bytes)")
    else:
        print("  ❌ SPLAT:        MISSING")
        ok = False

    if has_manifest:
        print("  ✅ manifest:     present")
    else:
        print("  ❌ manifest:     MISSING")
        ok = False

    # Check processed container (inputs should have been moved here)
    processed_container = client.get_container_client("processed")
    processed_blobs = list(processed_container.list_blobs(name_starts_with=prefix))
    processed_mp4 = [b for b in processed_blobs if b.name.endswith(".mp4")]

    if processed_mp4:
        print(f"  ✅ processed:    {len(processed_mp4)} input video(s) archived")
    else:
        print("  ❌ processed:    no input videos moved to processed container")
        ok = False

    # Check input container is empty (videos should have been deleted)
    input_container = client.get_container_client("input")
    remaining = list(input_container.list_blobs(name_starts_with=prefix))
    if not remaining:
        print("  ✅ input:        cleaned (all blobs moved)")
    else:
        print(f"  ⚠️  input:        {len(remaining)} blob(s) still remain")

    # Check error container is empty
    error_container = client.get_container_client("error")
    errors = list(error_container.list_blobs(name_starts_with=prefix))
    if not errors:
        print("  ✅ error:        empty (no failures)")
    else:
        print(f"  ❌ error:        {len(errors)} blob(s) in error container")
        ok = False

    return ok


def cmd_cleanup():
    """Delete all containers."""
    client = get_client()
    for name in CONTAINERS:
        try:
            client.delete_container(name)
        except Exception:
            pass


def cmd_sas():
    """Print a SAS token to stdout."""
    print(generate_sas())


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    command = sys.argv[1]

    if command == "setup":
        if len(sys.argv) != 4:
            print("Usage: azurite_helper.py setup <video_dir> <prefix>")
            sys.exit(1)
        cmd_setup(sys.argv[2], sys.argv[3])
    elif command == "verify":
        if len(sys.argv) != 3:
            print("Usage: azurite_helper.py verify <prefix>")
            sys.exit(1)
        ok = cmd_verify(sys.argv[2])
        sys.exit(0 if ok else 1)
    elif command == "cleanup":
        cmd_cleanup()
    elif command == "sas":
        cmd_sas()
    else:
        print(f"Unknown command: {command}")
        print(__doc__)
        sys.exit(1)


if __name__ == "__main__":
    main()
