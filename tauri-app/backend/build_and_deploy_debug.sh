#!/bin/bash

# Build and deploy debug Android APK using Tauri dev mode
# Usage: ./build_and_deploy_debug.sh
# 
# Note: This uses 'cargo tauri android dev' which automatically:
# - Builds the app in debug mode
# - Signs with Android's debug keystore (no password needed)
# - Installs on connected device
# - Watches for file changes and rebuilds automatically
#
# To stop watching, press Ctrl+C

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
    echo -e "${YELLOW}(Debug builds use a different signing key than release builds)${NC}"
    adb uninstall "$PACKAGE_NAME" || {
        echo -e "${YELLOW}Note: Uninstall had issues, but continuing anyway...${NC}"
    }
    echo -e "${GREEN}✓ Previous installation removed${NC}"
    echo ""
fi

# Change to script directory
cd "$SCRIPT_DIR"

# Check if we should use bundling mode (no dev server)
USE_BUNDLE=${1:-""}
if [ "$USE_BUNDLE" = "--bundle" ] || [ "$USE_BUNDLE" = "-b" ]; then
    echo -e "${GREEN}Building bundled Android app (no dev server)...${NC}"
    echo -e "${YELLOW}This will:${NC}"
    echo "  1. Build the app in debug mode with bundled assets"
    echo "  2. Sign with Android debug keystore (automatic)"
    echo "  3. Install on your device"
    echo ""
    
    # Build the app
    cargo tauri android build --debug
    
    # Find the built APK - prioritize debug APKs, then universal debug
    # Look for debug APKs first (these are signed with debug keystore)
    APK_PATH=$(find gen/android/app/build/outputs/apk -name "*debug*.apk" -type f 2>/dev/null | grep -v "unaligned" | head -1)
    
    # If no debug APK found, try universal debug
    if [ -z "$APK_PATH" ]; then
        APK_PATH=$(find gen/android/app/build/outputs/apk -name "*universal*debug*.apk" -type f 2>/dev/null | grep -v "unaligned" | head -1)
    fi
    
    # If still not found, try any debug APK
    if [ -z "$APK_PATH" ]; then
        APK_PATH=$(find gen/android/app/build/outputs/apk -name "*.apk" -type f 2>/dev/null | grep -i debug | grep -v "unaligned" | head -1)
    fi
    
    if [ -z "$APK_PATH" ]; then
        echo -e "${RED}Error: Could not find built debug APK${NC}"
        echo -e "${YELLOW}Available APKs:${NC}"
        find gen/android/app/build/outputs/apk -name "*.apk" -type f 2>/dev/null || echo "  None found"
        exit 1
    fi
    
    echo -e "${GREEN}Found APK: $APK_PATH${NC}"
    echo -e "${BLUE}Installing on device...${NC}"
    
    # Install the APK
    adb install -r "$APK_PATH"
    
    echo -e "${GREEN}✓ App installed successfully!${NC}"
else
    # Run Tauri dev mode
    echo -e "${GREEN}Starting Tauri Android dev mode...${NC}"
    echo -e "${YELLOW}This will:${NC}"
    echo "  1. Build the app in debug mode"
    echo "  2. Sign with Android debug keystore (automatic)"
    echo "  3. Install on your device"
    echo "  4. Watch for file changes and rebuild automatically"
    echo ""
    echo -e "${YELLOW}Press Ctrl+C to stop watching${NC}"
    echo ""
    echo -e "${BLUE}Tip: Use './build_and_deploy_debug.sh --bundle' for bundled build (no dev server)${NC}"
    echo ""
    
    # Run cargo tauri android dev
    # This command handles building, signing, and installing automatically
    cargo tauri android dev
fi
