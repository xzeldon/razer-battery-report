# Builds the release exe and the Inno Setup installer into dist/: the portable exe and the setup,
# both named after the version in Cargo.toml.
#
#     pwsh scripts/windows/build-installer.ps1 [-Notices THIRD-PARTY.md]
#
# -Notices embeds the generated licence notices (scripts/generate-notices.py) into the exe, as the
# release workflow does; without it the exe carries the placeholder.

param(
    [string]$Notices,
    [string]$Output = "dist"
)

$ErrorActionPreference = "Stop"
$package = "razer-battery-report"
$target = "x86_64-pc-windows-msvc"
$root = Resolve-Path (Join-Path $PSScriptRoot "../..")
Push-Location $root
try {
    $iscc = @(
        (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6/ISCC.exe")
        (Join-Path $env:LOCALAPPDATA "Programs/Inno Setup 6/ISCC.exe")
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) {
        throw "Inno Setup 6 is not installed"
    }

    if ($Notices) {
        $env:THIRD_PARTY_NOTICES = (Resolve-Path $Notices).Path
    }
    $messages = cargo build --release --locked --package $package --message-format=json-render-diagnostics |
        ForEach-Object { $_ | ConvertFrom-Json }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
    $exe = ($messages | Where-Object { $_.reason -eq "compiler-artifact" -and $_.executable -and $_.target.name -eq $package }).executable
    # build.rs writes the app icon as app.ico into its OUT_DIR.
    $outDir = ($messages | Where-Object { $_.reason -eq "build-script-executed" -and $_.package_id -like "*$package#*" }).out_dir
    $version = ((cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages | Where-Object { $_.name -eq $package }).version

    $dist = New-Item -ItemType Directory -Force $Output
    $name = "$package-v$version-$target"
    Copy-Item $exe (Join-Path $dist "$name.exe")

    $env:RBR_VERSION = $version
    $env:RBR_EXE = $exe
    $env:RBR_ICON = Join-Path $outDir "app.ico"
    $env:RBR_DIST = $dist.FullName
    $env:RBR_SETUP = "$name-setup"
    & $iscc /Q (Join-Path $PSScriptRoot "razer-battery-report.iss")
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}
