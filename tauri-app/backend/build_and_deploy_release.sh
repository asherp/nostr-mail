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

PACKAGE_NAME="com.nostr.mail"

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
echo -e "${BLUE}Installing on device (preserving app data)...${NC}"

# Try to install in-place first. -r preserves /data/data/<pkg>/ (your SQLite DB).
# Only fall back to uninstall+install if the signature mismatch makes it impossible —
# and require explicit confirmation, since uninstall destroys all local data.
set +e
INSTALL_OUTPUT=$(adb install -r "$APK_PATH" 2>&1)
INSTALL_RC=$?
set -e

if [ $INSTALL_RC -eq 0 ]; then
    echo -e "${GREEN}✓ Release app installed successfully (data preserved)!${NC}"
    exit 0
fi

echo -e "${RED}Install failed:${NC}"
echo "$INSTALL_OUTPUT"

if echo "$INSTALL_OUTPUT" | grep -qE "INSTALL_FAILED_UPDATE_INCOMPATIBLE|signatures do not match|INCONSISTENT_CERTIFICATES"; then
    echo ""
    echo -e "${RED}===========================================================${NC}"
    echo -e "${RED}SIGNATURE MISMATCH${NC}"
    echo -e "${RED}===========================================================${NC}"
    echo -e "${YELLOW}The installed APK was signed with a different key than this build.${NC}"
    echo -e "${YELLOW}The only way to install this APK is to UNINSTALL the existing app first.${NC}"
    echo -e "${RED}UNINSTALLING WILL DELETE ALL LOCAL APP DATA, INCLUDING YOUR SQLite DATABASE${NC}"
    echo -e "${RED}at /data/data/$PACKAGE_NAME/files/nostr-mail/nostr_mail.db.${NC}"
    echo ""
    echo -e "${YELLOW}Recommended: rebuild with the same signing key as the installed version.${NC}"
    echo ""
    read -rp "Type 'DELETE MY DATA' to uninstall and reinstall, or anything else to abort: " confirm
    if [ "$confirm" = "DELETE MY DATA" ]; then
        echo -e "${YELLOW}Uninstalling $PACKAGE_NAME...${NC}"
        adb uninstall "$PACKAGE_NAME"
        echo -e "${BLUE}Installing fresh APK...${NC}"
        adb install "$APK_PATH"
        echo -e "${GREEN}✓ Release app installed (data was wiped).${NC}"
    else
        echo -e "${GREEN}Aborted. Your existing app data is preserved.${NC}"
        exit 1
    fi
else
    exit $INSTALL_RC
fi
