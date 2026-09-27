# Tack installer — downloads the latest release binary for Windows.
#
#   irm https://raw.githubusercontent.com/yielab/tack/main/install.ps1 | iex
#
# Options (environment variables):
#   TACK_INSTALL_DIR   target directory (default: $env:LOCALAPPDATA\Programs\tack)
#   TACK_VERSION       release tag to install, e.g. v0.1.0-beta.9 (default: newest)
#   TACK_SKIP_CHECKSUM set to 1 to skip SHA256SUMS verification (only needed
#                      for v0.1.0-beta.6 and earlier, which predate the file)
#
# Installs the single tack.exe binary (server + CLI, web UI embedded). No
# Docker, no database server — run `tack` and open http://localhost:3210.
#
# Asset names are produced by .github/workflows/release.yml as
# `tack-<tag>-windows-x86_64.zip` (e.g. tack-v0.1.0-beta.9-windows-x86_64.zip),
# each holding a single top-level folder with tack.exe inside.
#
# Windows PowerShell 5.1 and PowerShell 7 both run this — no `??`, no
# ternaries, nothing 7-only. `$ErrorActionPreference = 'Stop'` turns
# Write-Error into a terminating error, which is how Fail (below) works.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# .NET on Windows PowerShell 5.1 defaults to TLS 1.0/1.1, which
# github.com/api.github.com reject outright — set this before any network
# call or the very first request fails with a cryptic "could not create SSL/TLS
# secure channel" instead of a useful error. Harmless to set again under
# PowerShell 7, where it's already the default.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# A script run via `.\install.ps1` has a file path in $MyInvocation and can
# safely `exit 1` on failure — that's how the CI job below (and anyone who
# saved the file) gets a real exit code. Piped through `irm | iex`, the
# script body runs as a scriptblock with no path; calling `exit` there would
# close the user's whole shell, so it re-throws instead and lets iex's own
# host print the terminating error.
$IsScriptFile = $null -ne $MyInvocation.MyCommand.Path

function Fail {
    param([string]$Message)
    Write-Error "tack-install: $Message"
}

$Repo = 'yielab/tack'
$InstallDir = $env:TACK_INSTALL_DIR
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\tack' }
$Version = $env:TACK_VERSION

# Tags are published as `v0.1.0-beta.9`; accept the bare version too rather
# than failing with "no asset for 0.1.0-beta.9", which reads like the release
# is missing when only the prefix is.
if ($Version -and -not $Version.StartsWith('v')) { $Version = "v$Version" }

$Api = "https://api.github.com/repos/$Repo/releases"
$Headers = @{ 'User-Agent' = 'tack-install.ps1' }

$Tmp = $null

try {
    # ── Detect platform ─────────────────────────────────────────────────────
    # Only x86_64 Windows is published (see release.yml's matrix) — arm64
    # Windows has no prebuilt binary yet.
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -eq 'ARM64') {
        Fail "no prebuilt binary for Windows ARM64 yet. Build from source: https://github.com/$Repo"
    } elseif ($arch -ne 'AMD64') {
        Fail "unsupported architecture '$arch'. Build from source: https://github.com/$Repo"
    }

    # ── Resolve the release from the GitHub API ────────────────────────────
    # Newest releases first; this includes pre-releases (the beta tags),
    # which /releases/latest deliberately excludes.
    Write-Host "Looking up the newest tack release for windows-x86_64..."
    try {
        $releases = Invoke-RestMethod -Uri "${Api}?per_page=100" -Headers $Headers
    } catch {
        Fail "could not query the releases API"
    }

    if ($Version) {
        $release = $releases | Where-Object { $_.tag_name -eq $Version } | Select-Object -First 1
        if (-not $release) { Fail "no release found for $Version" }
    } else {
        $release = $releases | Select-Object -First 1
        if (-not $release) { Fail "could not find any published release" }
    }

    # Every release also publishes a `tack-runner-<tag>-windows-x86_64.zip`
    # archive, ending in this same suffix — excluded explicitly, same as
    # install.sh, since that archive holds tack-runner.exe, never tack.exe.
    $asset = $release.assets |
        Where-Object { $_.name -like '*-windows-x86_64.zip' -and $_.name -notlike 'tack-runner-*' } |
        Select-Object -First 1
    if (-not $asset) { Fail "no windows-x86_64 asset found for $($release.tag_name)" }

    $Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tack-install-" + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $Tmp | Out-Null
    $zipPath = Join-Path $Tmp $asset.name

    Write-Host "Downloading $($asset.name) ..."
    try {
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -UseBasicParsing
    } catch {
        Fail "download failed: $($asset.browser_download_url)"
    }

    # ── Verify the archive against the release's own SHA256SUMS ────────────
    # release.yml publishes SHA256SUMS beside the archives. Fail closed: a
    # mismatch, a missing line, an unreachable checksum file or no sums asset
    # at all all stop the install rather than running an unverified binary.
    # TACK_SKIP_CHECKSUM=1 is the deliberate way out, needed only for
    # v0.1.0-beta.6 and earlier, which predate the file.
    if ($env:TACK_SKIP_CHECKSUM -eq '1') {
        Write-Host "Skipping checksum verification (TACK_SKIP_CHECKSUM=1)."
    } else {
        $sumsAsset = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
        if (-not $sumsAsset) {
            Fail "release $($release.tag_name) has no SHA256SUMS — releases before v0.1.0-beta.7 predate the file; re-run with TACK_SKIP_CHECKSUM=1 to install without it"
        }

        $sumsPath = Join-Path $Tmp 'SHA256SUMS'
        try {
            Invoke-WebRequest -Uri $sumsAsset.browser_download_url -OutFile $sumsPath -UseBasicParsing
        } catch {
            Fail "could not fetch $($sumsAsset.browser_download_url) — re-run with TACK_SKIP_CHECKSUM=1 to install without verifying"
        }

        # sha256sum writes "<digest>  <name>"; the binary-mode form prefixes
        # the name with "*". Match the whole field rather than a substring,
        # so one asset name can never be satisfied by another's line.
        $expected = $null
        foreach ($line in Get-Content -Path $sumsPath) {
            $parts = $line -split '\s+', 2
            if ($parts.Length -lt 2) { continue }
            $name = $parts[1].TrimStart('*')
            if ($name -eq $asset.name) { $expected = $parts[0]; break }
        }
        if (-not $expected) { Fail "SHA256SUMS has no entry for $($asset.name)" }

        $actual = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash
        if ($actual.ToLowerInvariant() -ne $expected.ToLowerInvariant()) {
            Fail "checksum mismatch for $($asset.name)`n       expected $expected`n       got      $actual`n       The download does not match what the release published. Not installing."
        }

        Write-Host "Checksum verified against the release SHA256SUMS."
    }

    # ── Extract & install ────────────────────────────────────────────────────
    $extractDir = Join-Path $Tmp 'extracted'
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

    $bin = Get-ChildItem -Path $extractDir -Filter 'tack.exe' -Recurse -File | Select-Object -First 1
    if (-not $bin) { Fail "no 'tack.exe' found in archive" }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $bin.FullName -Destination (Join-Path $InstallDir 'tack.exe') -Force

    Write-Host ""
    Write-Host "Installed tack to $InstallDir\tack.exe"

    # ── PATH ─────────────────────────────────────────────────────────────────
    # Only the user PATH, never the machine one — this installer has no
    # elevation and shouldn't need it.
    $trimmedInstallDir = $InstallDir.TrimEnd('\')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $userPathEntries = @()
    if ($userPath) { $userPathEntries = $userPath -split ';' | Where-Object { $_ } }
    $alreadyOnUserPath = $userPathEntries | Where-Object { $_.TrimEnd('\') -ieq $trimmedInstallDir }

    if (-not $alreadyOnUserPath) {
        $newUserPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
        Write-Host "Added $InstallDir to your user PATH. Open a new terminal to pick it up."
    }

    # Also make it usable right away in this session, whether or not it was
    # just added to the persisted user PATH.
    $sessionPathEntries = $env:Path -split ';' | Where-Object { $_ }
    $onSessionPath = $sessionPathEntries | Where-Object { $_.TrimEnd('\') -ieq $trimmedInstallDir }
    if (-not $onSessionPath) { $env:Path = "$env:Path;$InstallDir" }

    Write-Host 'Run "tack" and open http://localhost:3210'
} catch {
    # Fail() (above) already prefixes its own messages before throwing; an
    # exception from somewhere else (a cmdlet we didn't wrap) won't have the
    # prefix yet, so add it here rather than risk two different formats.
    # `-ErrorAction Continue` keeps this Write-Error from re-triggering the
    # global 'Stop' preference and skipping the exit below — Stop would turn
    # this call itself into a new terminating error and jump straight past
    # it, the one place in this script that must not happen.
    $Message = $_.Exception.Message
    if ($Message -notlike 'tack-install: *') { $Message = "tack-install: $Message" }
    if ($IsScriptFile) {
        Write-Error $Message -ErrorAction Continue
        exit 1
    } else {
        throw $Message
    }
} finally {
    if ($Tmp -and (Test-Path $Tmp)) { Remove-Item -Path $Tmp -Recurse -Force -ErrorAction SilentlyContinue }
}
