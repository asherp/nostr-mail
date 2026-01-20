#!/usr/bin/env python3
"""
Script to update version numbers across all files in the nostr-mail project.

Usage:
    python update_version.py <new_version>
    python update_version.py --help

Example:
    python update_version.py 1.0.1-beta
    python update_version.py 1.1.0-beta
"""

import re
import sys
import os
from pathlib import Path

# Files to update with their patterns
# Each entry: (file_path, pattern_function)
# The pattern function takes content and returns (pattern, replacement) tuple

def get_setup_cfg_pattern(content, new_version):
    """Update version in setup.cfg - only the one in [metadata] section."""
    # Match version line that comes after [metadata] and before [options]
    pattern = r'(^\[metadata\]\n(?:[^\[]*\n)*?version\s*=\s*)[^\n]+'
    def replacement(match):
        return match.group(1) + new_version
    return pattern, replacement

def get_cargo_toml_pattern(content, new_version):
    """Update version in Cargo.toml - only the one in [package] section."""
    # Match version line that comes right after [package] section header
    # This ensures we only match the package version, not dependency versions
    pattern = r'(^\[package\]\n(?:[^\[]*\n)*?version\s*=\s*")[^"]+(")'
    def replacement(match):
        return match.group(1) + new_version + match.group(2)
    return pattern, replacement

def get_tauri_conf_pattern(content, new_version):
    """Update version in tauri.conf.json."""
    pattern = r'("version":\s*")[^"]+(")'
    def replacement(match):
        return match.group(1) + new_version + match.group(2)
    return pattern, replacement

def get_app_js_pattern(content, new_version):
    """Update version in app.js console log."""
    pattern = r"(console\.log\('🌐 Version:\s*)[^']+('\))"
    def replacement(match):
        return match.group(1) + new_version + match.group(2)
    return pattern, replacement

def get_index_html_pattern(content, new_version):
    """Update version in index.html."""
    pattern = r'(<span class="version-text">v)[^<]+(</span>)'
    def replacement(match):
        return match.group(1) + new_version + match.group(2)
    return pattern, replacement

def get_release_notes_pattern(content, new_version):
    """Update version in RELEASE_NOTES.md."""
    pattern = r'(# Release Notes - v)[^\n]+'
    def replacement(match):
        return match.group(1) + new_version
    return pattern, replacement

def calculate_version_code(version):
    """Calculate Android versionCode from version string.
    
    Converts version like "1.0.3-beta" to versionCode like 1000003.
    Format: MAJOR * 1000000 + MINOR * 1000 + PATCH
    """
    # Extract numeric version (remove suffix like -beta)
    version_parts = version.split('-')[0].split('.')
    
    major = int(version_parts[0]) if len(version_parts) > 0 else 0
    minor = int(version_parts[1]) if len(version_parts) > 1 else 0
    patch = int(version_parts[2]) if len(version_parts) > 2 else 0
    
    version_code = major * 1000000 + minor * 1000 + patch
    return version_code

def get_tauri_properties_pattern(content, new_version):
    """Update version in tauri.properties (Android build config)."""
    # Store version_code in closure for use in replacement
    version_code = calculate_version_code(new_version)
    
    # Pattern for versionName
    pattern = r'(tauri\.android\.versionName=)[^\n]+'
    def replacement(match):
        return match.group(1) + new_version
    
    # Store version_code and pattern2 in the replacement function's closure
    pattern2 = r'(tauri\.android\.versionCode=)[^\n]+'
    def replacement2(match):
        return match.group(1) + str(version_code)
    
    # Return a special marker and a function that updates both
    return ("TAURI_PROPERTIES", (pattern, replacement, pattern2, replacement2))

VERSION_FILES = [
    # (file_path, pattern_function)
    ("setup.cfg", get_setup_cfg_pattern),
    ("tauri-app/backend/Cargo.toml", get_cargo_toml_pattern),
    ("tauri-app/backend/src-tauri/Cargo.toml", get_cargo_toml_pattern),
    ("tauri-app/backend/src-tauri/tauri.conf.json", get_tauri_conf_pattern),
    ("tauri-app/backend/tauri.conf.json", get_tauri_conf_pattern),
    ("tauri-app/frontend/js/app.js", get_app_js_pattern),
    ("tauri-app/frontend/index.html", get_index_html_pattern),
    ("RELEASE_NOTES.md", get_release_notes_pattern),
    ("tauri-app/backend/gen/android/app/tauri.properties", get_tauri_properties_pattern),
]


def update_file_version(file_path, pattern_func, new_version):
    """Update version in a single file."""
    file_path = Path(file_path)
    
    if not file_path.exists():
        print(f"⚠️  Warning: File not found: {file_path}")
        return False
    
    try:
        # Read file content
        content = file_path.read_text(encoding='utf-8')
        
        # Get pattern and replacement function from the pattern function
        result = pattern_func(content, new_version)
        
        # Special handling for tauri.properties (needs to update both versionName and versionCode)
        if isinstance(result, tuple) and result[0] == "TAURI_PROPERTIES":
            _, (pattern1, replacement1, pattern2, replacement2) = result
            flags = re.MULTILINE | re.DOTALL
            new_content = re.sub(pattern1, replacement1, content, flags=flags)
            new_content = re.sub(pattern2, replacement2, new_content, flags=flags)
        else:
            # Normal case - pattern and replacement function
            pattern, replacement_func = result
            flags = re.MULTILINE | re.DOTALL
            new_content = re.sub(pattern, replacement_func, content, flags=flags)
        
        # Check if anything changed
        if content == new_content:
            print(f"⚠️  No changes made to {file_path}")
            return False
        
        # Write updated content
        file_path.write_text(new_content, encoding='utf-8')
        print(f"✅ Updated {file_path}")
        return True
        
    except Exception as e:
        print(f"❌ Error updating {file_path}: {e}")
        import traceback
        traceback.print_exc()
        return False


def validate_version(version):
    """Basic version format validation."""
    if not version:
        return False
    
    # Check for common invalid patterns
    invalid_patterns = ['--', 'help', 'version']
    version_lower = version.lower()
    for pattern in invalid_patterns:
        if pattern in version_lower:
            return False
    
    # Version should contain at least one digit or letter
    if not re.search(r'[\d\w]', version):
        return False
    
    return True


def main():
    # Handle --help flag
    if len(sys.argv) == 2 and sys.argv[1] in ['--help', '-h', 'help']:
        print(__doc__)
        sys.exit(0)
    
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(1)
    
    new_version = sys.argv[1].strip()
    
    if not new_version:
        print("❌ Error: Version cannot be empty")
        sys.exit(1)
    
    # Validate version format
    if not validate_version(new_version):
        print(f"❌ Error: Invalid version format: '{new_version}'")
        print("   Version should not contain '--help', 'help', or 'version'")
        print("   Example valid versions: 1.0.1-beta, 1.1.0, 2.0.0-beta")
        sys.exit(1)
    
    # Get the project root directory
    script_dir = Path(__file__).parent
    os.chdir(script_dir)
    
    print(f"🔄 Updating version to: {new_version}\n")
    
    updated_count = 0
    failed_count = 0
    
    for file_path, pattern_func in VERSION_FILES:
        if update_file_version(file_path, pattern_func, new_version):
            updated_count += 1
        else:
            failed_count += 1
    
    print(f"\n📊 Summary:")
    print(f"   ✅ Updated: {updated_count} files")
    if failed_count > 0:
        print(f"   ⚠️  Warnings: {failed_count} files")
    
    if updated_count > 0:
        print(f"\n✨ Version updated successfully to: {new_version}")
        print(f"💡 Don't forget to:")
        print(f"   - Review the changes")
        print(f"   - Commit the version update")
        print(f"   - Create a git tag if needed")
    else:
        print(f"\n❌ No files were updated. Please check the version format.")
        sys.exit(1)


if __name__ == "__main__":
    main()
