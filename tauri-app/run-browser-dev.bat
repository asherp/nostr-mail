@echo off
REM Script to run the backend HTTP server and serve the frontend for browser development on Windows

echo 🚀 Starting NostrMail Browser Development Mode
echo ================================================
echo.

REM Get the directory where this script is located
set SCRIPT_DIR=%~dp0
set BACKEND_DIR=%SCRIPT_DIR%backend
set FRONTEND_DIR=%SCRIPT_DIR%frontend

REM Check if we're in the right directory
if not exist "%BACKEND_DIR%" (
    echo ❌ Error: backend directory not found
    exit /b 1
)
if not exist "%FRONTEND_DIR%" (
    echo ❌ Error: frontend directory not found
    exit /b 1
)

REM Build the HTTP server if needed
echo.
echo 📦 Building HTTP server...
cd /d "%BACKEND_DIR%"
cargo build --bin http-server --release

REM Start the HTTP server in a new window
echo.
echo 🌐 Starting HTTP server on http://127.0.0.1:1420...
start "NostrMail HTTP Server" cmd /k "cargo run --bin http-server --release"

REM Wait a moment for the server to start
timeout /t 3 /nobreak >nul

REM Start frontend server
echo.
echo 📂 Serving frontend on http://127.0.0.1:8080...
echo    Open http://127.0.0.1:8080 in your browser
echo.
echo Press any key to stop the frontend server (HTTP server will continue running)

cd /d "%FRONTEND_DIR%"
python -m http.server 8080

pause
