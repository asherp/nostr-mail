#!/bin/bash

# Build and deploy release Android APK using Tauri
# Usage: ./build_and_deploy_release.sh
# 
# Note: This uses 'cargo tauri android build' which:
# - Builds the app in release mode (optimized)
# - Requires proper signing configuration (not debug keystore)
# - Installs on connected device
#
# IMPORTANT: Release builds require a signing key configured in your
# Android project. Make sure you have set up your keystore properly.

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_SDK_PATH="$HOME/Library/Android/sdk"
PLATFORM_TOOLS_PATH="$ANDROID_SDK_PATH/platform-tools"

# Add Android tools to PATH
export PATH="$PATH:$PLATFORM_TOOLS_PATH"

# Check if device is connected
echo -e "${BLUE}Checking for connected Android device...${NC}"
if ! adb devices | grep -q "device$"; then
    echo -e "${RED}Error: No Android device connected${NC}"
    echo "Please connect your device and enable USB debugging"
    exit 1
fi

DEVICE_COUNT=$(adb devices | grep -c "device$")
echo -e "${GREEN}✓ Found $DEVICE_COUNT device(s) connected${NC}"

# Show connected device info
echo -e "${BLUE}Connected device(s):${NC}"
adb devices -l | grep "device$" | while read line; do
    echo "  $line"
done
echo ""

# Check if app is already installed and uninstall if needed
# This is necessary because debug and release builds use different signing keys
PACKAGE_NAME="com.nostr.mail"
if adb shell pm list packages | grep -q "^package:$PACKAGE_NAME$"; then
    echo -e "${YELLOW}App is already installed. Uninstalling to avoid signature mismatch...${NC}"
    echo -e "${YELLOW}(Release builds use a different signing key than debug builds)${NC}"
    adb uninstall "$PACKAGE_NAME" || {
        echo -e "${YELLOW}Note: Uninstall had issues, but continuing anyway...${NC}"
    }
    echo -e "${GREEN}✓ Previous installation removed${NC}"
    echo ""
fi

# Change to script directory
cd "$SCRIPT_DIR"

# Build the release app
echo -e "${GREEN}Building release Android app...${NC}"
echo -e "${YELLOW}This will:${NC}"
echo "  1. Build the app in release mode (optimized)"
echo "  2. Sign with your configured release keystore"
echo "  3. Install on your device"
echo ""
echo -e "${YELLOW}Note: Make sure you have configured your release signing key!${NC}"
echo ""

# Build the app in release mode
cargo tauri android build

# Find the built APK - prioritize release APKs
# Look for release APKs first
APK_PATH=$(find gen/android/app/build/outputs/apk -name "*release*.apk" -type f 2>/dev/null | grep -v "unaligned" | head -1)

# If no release APK found, try universal release
if [ -z "$APK_PATH" ]; then
    APK_PATH=$(find gen/android/app/build/outputs/apk -name "*universal*release*.apk" -type f 2>/dev/null | grep -v "unaligned" | head -1)
fi

# If still not found, try any release APK
if [ -z "$APK_PATH" ]; then
    APK_PATH=$(find gen/android/app/build/outputs/apk -name "*.apk" -type f 2>/dev/null | grep -i release | grep -v "unaligned" | head -1)
fi

# If still not found, try any APK that's not debug
if [ -z "$APK_PATH" ]; then
    APK_PATH=$(find gen/android/app/build/outputs/apk -name "*.apk" -type f 2>/dev/null | grep -v debug | grep -v "unaligned" | head -1)
fi

if [ -z "$APK_PATH" ]; then
    echo -e "${RED}Error: Could not find built release APK${NC}"
    echo -e "${YELLOW}Available APKs:${NC}"
    find gen/android/app/build/outputs/apk -name "*.apk" -type f 2>/dev/null || echo "  None found"
    exit 1
fi

echo -e "${GREEN}Found APK: $APK_PATH${NC}"
echo -e "${BLUE}Installing on device...${NC}"

# Install the APK
adb install -r "$APK_PATH"

echo -e "${GREEN}✓ Release app installed successfully!${NC}"
