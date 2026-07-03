#!/bin/bash

# Path to the source logo
SRC_LOGO="/Users/cihan/PROJECT/google-drive-desktop/public/logo copy.png"
OUTPUT_DIR="/Users/cihan/PROJECT/google-drive-desktop/public"

# Check if source logo exists
if [ ! -f "$SRC_LOGO" ]; then
    echo "Error: Source logo not found at $SRC_LOGO"
    exit 1
fi

# Sizes to resize to
SIZES=(16 32 64 128 256)

for size in "${SIZES[@]}"; do
    output_file="${OUTPUT_DIR}/logo_${size}.png"
    echo "Resizing to ${size}x${size} -> ${output_file}"
    sips -z "$size" "$size" "$SRC_LOGO" --out "$output_file"
done

# Also overwrite logo.png with the 256x256 version to resolve Vite build errors
cp "${OUTPUT_DIR}/logo_256.png" "${OUTPUT_DIR}/logo.png"
echo "Successfully updated public/logo.png with the 256x256 version."
