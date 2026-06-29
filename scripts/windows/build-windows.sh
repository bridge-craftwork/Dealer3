#!/bin/bash
# Build script for Windows 64-bit version of dealer3

set -e

echo "Building dealer3 for Windows (x86_64)..."

# Check if mingw-w64 is installed
if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
    echo "Error: mingw-w64 not found. Install with:"
    echo "  brew install mingw-w64"
    exit 1
fi

# Check if Windows target is installed
if ! rustup target list --installed | grep -q x86_64-pc-windows-gnu; then
    echo "Installing Windows target..."
    rustup target add x86_64-pc-windows-gnu
fi

# Build for Windows
echo "Compiling..."
cargo build --release --target x86_64-pc-windows-gnu

# Create distribution directory
echo "Creating distribution package..."
rm -rf dist/dealer3-windows-x64
mkdir -p dist/dealer3-windows-x64

# Copy files
cp target/x86_64-pc-windows-gnu/release/dealer.exe dist/dealer3-windows-x64/
cp LICENSE dist/dealer3-windows-x64/
cp CHANGELOG.md dist/dealer3-windows-x64/

# Create README for Windows
cat > dist/dealer3-windows-x64/README.txt << 'EOF'
dealer3 - Windows 64-bit Version
================================

Bridge hand generator with constraint evaluation

This is a Rust implementation of the classic dealer program by Thomas Andrews,
compatible with both dealer.exe and DealerV2_4.

QUICK START
-----------

1. Open Command Prompt (cmd.exe) or PowerShell
2. Navigate to the directory containing dealer.exe
3. Run dealer with input from a file or stdin

EXAMPLES
--------

Generate 10 deals where North has 15+ HCP:
  echo hcp(north) >= 15 | dealer.exe -p 10

Use a constraint file:
  dealer.exe -p 10 < constraint.dl

Predeal specific cards (DealerV2_4 format):
  echo hcp(north) >= 0 | dealer.exe -E S8743,HA9,D642,CQT64 -W SQ965,HK63,DAQJT,CA5 -p 5

For full documentation, see: https://github.com/bridge-craftwork/Dealer3

VERSION
-------

Built from dealer3 version 0.1.0
Compatible with dealer.exe and DealerV2_4
EOF

# Create zip file
cd dist
rm -f dealer3-windows-x64.zip
zip -r dealer3-windows-x64.zip dealer3-windows-x64/
cd ..

echo ""
echo "Build complete!"
echo "Binary size: $(ls -lh target/x86_64-pc-windows-gnu/release/dealer.exe | awk '{print $5}')"
echo "Package: dist/dealer3-windows-x64.zip ($(ls -lh dist/dealer3-windows-x64.zip | awk '{print $5}'))"
echo ""
echo "Distribution contents:"
unzip -l dist/dealer3-windows-x64.zip
