@echo off
REM Nuitka Build Script for TimeLeapMemo
REM This script builds a standalone executable with all dependencies

echo ========================================
echo TimeLeapMemo Nuitka Build Script
echo ========================================
echo.

REM Check if Nuitka is installed
python -m nuitka --version >nul 2>&1
if errorlevel 1 (
    echo Nuitka is not installed. Installing...
    pip install nuitka
    if errorlevel 1 (
        echo Failed to install Nuitka. Please install manually: pip install nuitka
        pause
        exit /b 1
    )
)

echo Building executable...
echo.

python -m nuitka ^
    --standalone ^
    --onefile ^
    --windows-icon-from-ico=Icon\timeLeapMemoIcon_v2.ico ^
    --include-data-file=config.json=config.json ^
    --enable-plugin=pyqt6 ^
    --windows-console-mode=disable ^
    --output-dir=Releas ^
    --output-filename=TimeLeapMemo.exe ^
    --company-name="TimeLeap" ^
    --product-name="TimeLeapMemo" ^
    --file-version=1.0.0.0 ^
    --product-version=1.0.0.0 ^
    --file-description="Homeostatic Forget Drawing Application" ^
    --assume-yes-for-downloads ^
    TimeLeapMemo.py

if errorlevel 1 (
    echo.
    echo Build failed! Please check the error messages above.
    pause
    exit /b 1
)

echo.
echo ========================================
echo Build completed successfully!
echo ========================================
echo Executable location: Releas\TimeLeapMemo.exe
echo.
pause
