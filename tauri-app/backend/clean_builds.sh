#!/bin/bash

# Clean all build files for Tauri Android project
# Usage: ./clean_builds.sh [--all]
#   --all: Also clean Gradle cache and Rust target directories

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

CLEAN_ALL=false
if [ "$1" == "--all" ]; then
    CLEAN_ALL=true
fi

echo -e "${BLUE}Cleaning build files...${NC}"
echo ""

# Clean Android build directories
echo -e "${YELLOW}Cleaning Android build directories...${NC}"
if [ -d "gen/android/app/build" ]; then
    rm -rf gen/android/app/build
    echo -e "${GREEN}✓ Removed gen/android/app/build${NC}"
fi

if [ -d "gen/android/build" ]; then
    rm -rf gen/android/build
    echo -e "${GREEN}✓ Removed gen/android/build${NC}"
fi

if [ -d "gen/android/buildSrc/build" ]; then
    rm -rf gen/android/buildSrc/build
    echo -e "${GREEN}✓ Removed gen/android/buildSrc/build${NC}"
fi

# Clean APK and AAB files
echo -e "${YELLOW}Cleaning APK and AAB files...${NC}"
find gen/android -name "*.apk" -type f -delete 2>/dev/null && echo -e "${GREEN}✓ Removed APK files${NC}" || true
find gen/android -name "*.aab" -type f -delete 2>/dev/null && echo -e "${GREEN}✓ Removed AAB files${NC}" || true

# Clean Gradle cache (optional)
if [ "$CLEAN_ALL" = true ]; then
    echo -e "${YELLOW}Cleaning Gradle cache...${NC}"
    if [ -d "gen/android/.gradle" ]; then
        rm -rf gen/android/.gradle
        echo -e "${GREEN}✓ Removed gen/android/.gradle${NC}"
    fi
fi

# Clean Rust target directories (optional)
if [ "$CLEAN_ALL" = true ]; then
    echo -e "${YELLOW}Cleaning Rust target directories...${NC}"
    if [ -d "target" ]; then
        rm -rf target
        echo -e "${GREEN}✓ Removed target/ directory${NC}"
    fi
    if [ -d "src-tauri/target" ]; then
        rm -rf src-tauri/target
        echo -e "${GREEN}✓ Removed src-tauri/target/ directory${NC}"
    fi
fi

echo ""
echo -e "${GREEN}✓ Build cleanup complete!${NC}"
if [ "$CLEAN_ALL" != true ]; then
    echo -e "${YELLOW}Note: Use './clean_builds.sh --all' to also clean Gradle cache and Rust targets${NC}"
fi
