#Requires -Version 7.0
<#
.SYNOPSIS
    Binary smoke gate: launch the built RepoSync binary and assert it survives startup.

.DESCRIPTION
    Closes BL-NI-88 (no gate ever launches the built binary). Every other gate in this
    repo reads or exercises code WITHOUT running the app: cargo test calls library
    functions, vitest renders components, clippy and fmt read source. None of them can
    reach reposync_lib::run, which has exactly one caller - the app's own startup. That
    is how BL-NI-87 (an unreachable! in logging::init that was always reachable) survived
    19 days of green gates and two installers: only launching the built binary could
    catch it.

    This script is that launch. It is deliberately paranoid about the one failure shape
    the app actually has:

      * [profile.release] panic = "abort" (Cargo.toml) means a panic on the startup path
        kills the process WITHOUT unwinding, so the WorkerGuard Drop that flushes
        tracing-appender's non-blocking queue never runs. The rolling appender creates
        today's file when logging::init builds it, so a crash leaves a log file that
        EXISTS and is ZERO BYTES. "The log file exists" is therefore not an assertion at
        all - "the log file is non-empty AND carries the startup line" is.
      * The binary is built with windows_subsystem = "windows", so a release build has no
        console and a panic message goes nowhere a human can see it. We hand the process
        a real stderr handle via Start-Process -RedirectStandardError, which is the exact
        technique that root-caused BL-NI-87, and print that file on failure.

    WINDOWS ONLY. The isolation mechanism is %LOCALAPPDATA%: resolve_data_dir() in
    crates/reposync-core/src/paths.rs reads that variable on Windows and appends
    "RepoSync", so pointing it at a scratch directory gives the launched app its own
    database, its own logs/, and no contact with the developer's real data. No code
    change was needed to make the app testable this way.

    NOTE on isolation scope: the scratch LOCALAPPDATA isolates RepoSync's own data dir.
    It does not isolate WebView2's browser profile, which WebView2 places relative to the
    executable. That is harmless here (an ephemeral runner, or a build output directory
    locally) and is called out so nobody later mistakes this for full sandboxing.

.PARAMETER ExePath
    Path to the built binary. Defaults to target/release/reposync.exe relative to the
    repository root (the Cargo workspace root IS the repo root, so pnpm tauri build and
    cargo build --release -p reposync both land it there).

.PARAMETER StartupTimeoutSeconds
    How long to poll for the startup line before giving up. See the timing note below.

.PARAMETER SettleSeconds
    How long to keep watching the process AFTER the startup line appears. See below.

.PARAMETER KeepScratch
    Leave the scratch data directory in place instead of deleting it (for debugging).

.NOTES
    TIMING, and why these two numbers:

    The startup line is emitted by logging::init, which is the FIRST statement of run() -
    before the Tauri builder, before the window, before the database. So the line
    appearing proves only that logging came up. Everything that can actually kill the app
    happens AFTER it: the WebView2 window build, init_pool_with_recovery (whose .expect
    aborts), the activity-log sweep, the settings read, the autostart reconcile, the
    scheduler spawn, the tray build, and windows::init. Asserting liveness at the moment
    the line appears would assert almost nothing.

    Hence two phases rather than one fixed sleep:

      Phase 1 - poll every 500 ms for up to StartupTimeoutSeconds (default 30) for the
      log file to exist, be non-empty, and contain the startup line, failing IMMEDIATELY
      if the process dies first. Locally this resolves in well under a second; 30 s is
      headroom for a cold CI runner, and because it polls, the common case costs about a
      second rather than the whole budget.

      Phase 2 - keep polling liveness for SettleSeconds (default 10) after the line
      appears, then assert the process is still alive. This is the window that covers the
      synchronous setup closure listed above. Ten seconds is comfortably longer than that
      work takes (sub-second locally) and also spans WebView2's first-run initialization,
      which is the slowest part of a Tauri startup on a cold machine.

    Typical cost is about 11 seconds; worst case about 40. The build job it runs in
    already spends minutes compiling and bundling, so this is not a meaningful tax on a
    pull request. Both values are parameters so a slow runner can be accommodated without
    editing logic.
#>
[CmdletBinding()]
param(
    [string]$ExePath,
    [int]$StartupTimeoutSeconds = 30,
    [int]$SettleSeconds = 10,
    [switch]$KeepScratch
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# The line asserted on. Emitted unconditionally at the end of logging::init
# (src-tauri/src/logging.rs) once the subscriber is installed, at INFO, which is
# DEFAULT_LEVEL. It is the only unconditional log line on the startup path: every other
# tracing call in src-tauri is a warn/error on a failure branch or is gated by a
# condition. If init returns Err instead, the app deliberately continues WITHOUT logging,
# so there is no log file at all and this gate fails on the missing file.
$StartupLine = 'RepoSync starting'

# Daily rolling files named LOG_FILE_PREFIX.YYYY-MM-DD.LOG_FILE_SUFFIX
# (crates/reposync-core/src/logging.rs), written into logs/ under the data dir.
$LogGlob = 'reposync.*.log'

$PollMs = 500

function Write-Section {
    param([Parameter(Mandatory)][string]$Title)
    Write-Host ''
    Write-Host "=== $Title ==="
}

function Format-ExitCode {
    param([Parameter(Mandatory)][int]$Code)
    # Windows fatal-error codes surface as negative Int32. The X8 format shows the
    # two's-complement hex, which is the form they are recognizable in: 0xC0000409 is the
    # fastfail/abort signature BL-NI-87 died with.
    '{0} (0x{1:X8})' -f $Code, $Code
}

function Get-LogFiles {
    param([Parameter(Mandatory)][string]$LogDir)
    # Callers wrap this in @(...) because PowerShell unrolls a returned array: without
    # that wrapper an empty result arrives as $null and .Count throws under StrictMode.
    if (-not (Test-Path -LiteralPath $LogDir)) { return }
    Get-ChildItem -LiteralPath $LogDir -Filter $LogGlob -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending
}

# Read the log by opening it, not by trusting its directory entry.
#
# This matters more than it looks. Windows does not update a file's cached directory-entry
# size while another process holds the handle open and is appending to it, so
# Get-ChildItem reported a healthy, fully written log as "1 bytes" during development.
# Basing the non-empty assertion on that number would make this gate flaky in the one
# direction that is unacceptable: a real launch failing red, and eventually being ignored.
# Opening with FileShare::ReadWrite reads what is actually on disk while the app holds the
# file, which is exactly the question being asked - does reading this log yield anything,
# or is it the zero bytes an aborted process leaves behind.
function Read-LogText {
    param([Parameter(Mandatory)][string]$Path)
    try {
        $fs = [System.IO.File]::Open(
            $Path,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::Read,
            [System.IO.FileShare]::ReadWrite)
        try {
            $reader = [System.IO.StreamReader]::new($fs)
            try { return $reader.ReadToEnd() }
            finally { $reader.Dispose() }
        }
        finally { $fs.Dispose() }
    }
    catch {
        return ''
    }
}

function Show-FileOrNote {
    param(
        [Parameter(Mandatory)][string]$Label,
        [string]$Path
    )
    Write-Host ''
    Write-Host "--- $Label ---"
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        Write-Host '(no such file)'
        return
    }
    $text = Read-LogText -Path $Path
    if ($text.Length -eq 0) {
        Write-Host '(EMPTY - reading it yields 0 bytes)'
        return
    }
    Write-Host $text
}

# Everything a human needs to diagnose a red gate, printed on every failure path. A CI
# failure nobody can diagnose is barely better than no gate at all.
function Write-Diagnostics {
    param(
        [System.Diagnostics.Process]$Process,
        [Parameter(Mandatory)][string]$LogDir,
        [string]$StdErrPath,
        [string]$StdOutPath,
        [string]$ScratchDir
    )

    Write-Section 'DIAGNOSTICS'

    if ($Process) {
        if ($Process.HasExited) {
            Write-Host "process: EXITED, exit code $(Format-ExitCode -Code $Process.ExitCode)"
        }
        else {
            Write-Host "process: still running (pid $($Process.Id))"
        }
    }
    else {
        Write-Host 'process: never started'
    }

    Write-Host ''
    Write-Host "--- log directory: $LogDir ---"
    $logs = @(Get-LogFiles -LogDir $LogDir)
    if ($logs.Count -eq 0) {
        Write-Host "(no log file matching $LogGlob - the app never opened one)"
    }
    else {
        foreach ($f in $logs) {
            $read = (Read-LogText -Path $f.FullName).Length
            Write-Host ('{0,10} chars read, directory entry says {1,10} bytes  {2}' -f $read, $f.Length, $f.Name)
        }
        Show-FileOrNote -Label "log contents: $($logs[0].Name)" -Path $logs[0].FullName
    }

    Show-FileOrNote -Label 'captured stderr' -Path $StdErrPath
    Show-FileOrNote -Label 'captured stdout' -Path $StdOutPath

    if ($ScratchDir -and (Test-Path -LiteralPath $ScratchDir)) {
        Write-Host ''
        Write-Host "--- scratch data directory tree: $ScratchDir ---"
        Get-ChildItem -LiteralPath $ScratchDir -Recurse -File -ErrorAction SilentlyContinue |
            ForEach-Object { Write-Host ('{0,10} bytes  {1}' -f $_.Length, $_.FullName.Substring($ScratchDir.Length)) }
    }
}

function Get-WebView2Version {
    # The WebView2 Evergreen Runtime registers its version under this fixed client GUID.
    # Printed rather than asserted: whether a given runner image carries it is exactly the
    # question this gate answers empirically the first time it runs.
    $guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($root in @(
            "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$guid",
            "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid",
            "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid")) {
        try {
            $pv = (Get-ItemProperty -Path $root -Name 'pv' -ErrorAction Stop).pv
            if ($pv) { return "$pv (from $root)" }
        }
        catch {
            # Not registered under this root; try the next one.
        }
    }
    return $null
}

# --- Resolve inputs -----------------------------------------------------------------

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $ExePath) {
    $ExePath = Join-Path $repoRoot 'target\release\reposync.exe'
}

Write-Section 'RepoSync binary smoke gate (BL-NI-88)'
Write-Host "repository root : $repoRoot"
Write-Host "binary          : $ExePath"
Write-Host "startup line    : '$StartupLine'"
Write-Host "log file glob   : logs\$LogGlob"
Write-Host "startup timeout : $StartupTimeoutSeconds s (polled every $PollMs ms)"
Write-Host "settle window   : $SettleSeconds s after the startup line"

# A missing binary is a HARD failure, never a skip. A skip here would recreate exactly
# the problem this gate exists to fix: a green check that never launched the app. If the
# build output path ever moves, this must go red and be noticed.
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    Write-Host "::error::Smoke gate: the built binary was not found at '$ExePath'. Build it first (pnpm tauri build, or cargo build --release -p reposync). Refusing to pass without launching anything."
    exit 1
}

$exeItem = Get-Item -LiteralPath $ExePath
Write-Host "binary size     : $($exeItem.Length) bytes, built $($exeItem.LastWriteTime.ToString('u'))"

Write-Section 'Host environment'
Write-Host "OS              : $([System.Environment]::OSVersion.VersionString)"
Write-Host "user            : $([System.Environment]::UserName)"
Write-Host "interactive     : $([System.Environment]::UserInteractive)"
Write-Host "session id      : $((Get-Process -Id $PID).SessionId)"
$wv2 = Get-WebView2Version
if ($wv2) {
    Write-Host "WebView2        : $wv2"
}
else {
    Write-Host 'WebView2        : NOT registered under any EdgeUpdate client key.'
    Write-Host '                  The app builds its window through WebView2, so if the'
    Write-Host '                  launch below fails at window creation, this is the reason.'
}

# --- Scratch data directory ---------------------------------------------------------

$scratchRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('reposync-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratchRoot -Force | Out-Null
$dataDir = Join-Path $scratchRoot 'RepoSync'
$logDir = Join-Path $dataDir 'logs'
$stdErrPath = Join-Path $scratchRoot 'stderr.txt'
$stdOutPath = Join-Path $scratchRoot 'stdout.txt'

Write-Section 'Scratch isolation'
Write-Host "LOCALAPPDATA    : $scratchRoot"
Write-Host "expected data   : $dataDir"
Write-Host "expected logs   : $logDir"

$originalLocalAppData = $env:LOCALAPPDATA
$originalLogLevel = $env:REPOSYNC_LOG
$proc = $null
$failed = $false
$windowTitle = $null
$windowSeenAfterSeconds = $null
$elapsedAtEnd = 0.0

try {
    $env:LOCALAPPDATA = $scratchRoot
    # Pin the level rather than inheriting it. DEFAULT_LEVEL is INFO and the startup line
    # is INFO, so an ambient REPOSYNC_LOG=warn in a shell or on a runner would silently
    # remove the very line this gate asserts on and turn the gate into a false red.
    $env:REPOSYNC_LOG = 'info'

    Write-Section 'Launch'
    try {
        # -RedirectStandardError is the load-bearing flag: it gives a GUI-subsystem
        # process a real stderr handle, which is how BL-NI-87's abort was finally made
        # visible. Without it a panic on this path prints into nothing.
        $proc = Start-Process -FilePath $ExePath `
            -WorkingDirectory $scratchRoot `
            -RedirectStandardError $stdErrPath `
            -RedirectStandardOutput $stdOutPath `
            -PassThru
    }
    catch {
        Write-Host "::error::Smoke gate: could not start '$ExePath': $($_.Exception.Message)"
        Write-Diagnostics -Process $null -LogDir $logDir -StdErrPath $stdErrPath -StdOutPath $stdOutPath -ScratchDir $scratchRoot
        exit 1
    }

    Write-Host "started pid $($proc.Id) at $(Get-Date -Format 'HH:mm:ss.fff')"

    # --- Phase 1: poll for the startup line ------------------------------------------

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $sawStartupLine = $false

    while ($sw.Elapsed.TotalSeconds -lt $StartupTimeoutSeconds) {
        if ($proc.HasExited) {
            Write-Host "::error::Smoke gate: the app EXITED $([math]::Round($sw.Elapsed.TotalSeconds, 2)) s after launch, before the startup line was written. Exit code $(Format-ExitCode -Code $proc.ExitCode). A GUI-subsystem process that dies during startup is exactly the BL-NI-87 shape; the captured stderr below carries the panic text if there was one."
            $failed = $true
            break
        }

        $logs = @(Get-LogFiles -LogDir $logDir)
        if ($logs.Count -gt 0) {
            $content = Read-LogText -Path $logs[0].FullName
            if ($content.Length -gt 0 -and $content.Contains($StartupLine)) {
                $sawStartupLine = $true
                Write-Host "startup line seen after $([math]::Round($sw.Elapsed.TotalSeconds, 2)) s in $($logs[0].Name) ($($content.Length) chars read)"
                break
            }
        }

        Start-Sleep -Milliseconds $PollMs
    }

    if (-not $failed -and -not $sawStartupLine) {
        $logs = @(Get-LogFiles -LogDir $logDir)
        if ($logs.Count -eq 0) {
            Write-Host "::error::Smoke gate: after $StartupTimeoutSeconds s the app is still running but NO log file matching 'logs\$LogGlob' exists. logging::init either never ran or failed, and the app is running blind."
        }
        else {
            $content = Read-LogText -Path $logs[0].FullName
            if ($content.Length -eq 0) {
                Write-Host "::error::Smoke gate: after $StartupTimeoutSeconds s the log file '$($logs[0].Name)' exists but reading it yields ZERO BYTES. The appender opens that file when logging::init builds it, so an empty one means nothing was ever flushed to it - either the process aborted before the queue drained (the panic = abort shape, and what BL-NI-87 looked like), or it is still running and the startup line is no longer being emitted at all."
            }
            else {
                Write-Host "::error::Smoke gate: after $StartupTimeoutSeconds s the log file '$($logs[0].Name)' is non-empty ($($content.Length) chars) but does NOT contain the startup line '$StartupLine'. Either the app is stuck before logging::init finished, or that line changed and this gate needs updating."
            }
        }
        $failed = $true
    }

    # --- Phase 2: settle, then assert the app survived the rest of startup ------------

    if (-not $failed) {
        Write-Section "Settle ($SettleSeconds s)"
        Write-Host 'Everything that can kill this app happens AFTER the startup line: the WebView2'
        Write-Host 'window build, the database open and migrations, the activity sweep, the settings'
        Write-Host 'read, the autostart reconcile, the scheduler spawn, the tray build, and the'
        Write-Host 'window lifecycle. This window is what covers them.'

        $settle = [System.Diagnostics.Stopwatch]::StartNew()
        while ($settle.Elapsed.TotalSeconds -lt $SettleSeconds) {
            if ($proc.HasExited) {
                Write-Host "::error::Smoke gate: the app wrote its startup line and then EXITED $([math]::Round($settle.Elapsed.TotalSeconds, 2)) s later, during the rest of startup. Exit code $(Format-ExitCode -Code $proc.ExitCode)."
                $failed = $true
                break
            }

            # Observation, not an assertion. A visible top-level window titled "RepoSync"
            # means the setup closure ran all the way through windows::init, which is a
            # far stronger end-of-startup signal than liveness alone. Whether a hosted CI
            # runner can host a window at all is precisely what this reports.
            if (-not $windowTitle) {
                $live = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
                if ($live) {
                    $live.Refresh()
                    if ($live.MainWindowTitle) {
                        $windowTitle = $live.MainWindowTitle
                        $windowSeenAfterSeconds = [math]::Round($settle.Elapsed.TotalSeconds, 2)
                    }
                }
            }

            Start-Sleep -Milliseconds $PollMs
        }
    }

    # --- Final assertions -------------------------------------------------------------

    if (-not $failed) {
        $elapsedAtEnd = [math]::Round($sw.Elapsed.TotalSeconds, 1)
        Write-Section 'Assertions'

        if ($proc.HasExited) {
            Write-Host "::error::Smoke gate: the app is NOT alive at the end of the settle window. Exit code $(Format-ExitCode -Code $proc.ExitCode)."
            $failed = $true
        }
        else {
            Write-Host "PASS  process is alive $elapsedAtEnd s after launch (pid $($proc.Id))"
        }

        $logs = @(Get-LogFiles -LogDir $logDir)
        if ($logs.Count -eq 0) {
            Write-Host "::error::Smoke gate: the log file disappeared from '$logDir'."
            $failed = $true
        }
        else {
            $content = Read-LogText -Path $logs[0].FullName
            if ($content.Length -eq 0) {
                Write-Host "::error::Smoke gate: the log file '$($logs[0].Name)' reads as ZERO BYTES. Under panic = abort an empty log is what a crash looks like."
                $failed = $true
            }
            elseif ($content.Contains($StartupLine)) {
                Write-Host "PASS  log file $($logs[0].Name) is non-empty ($($content.Length) chars read)"
                Write-Host "PASS  log contains the startup line '$StartupLine'"
            }
            else {
                Write-Host "::error::Smoke gate: the log file '$($logs[0].Name)' is non-empty ($($content.Length) chars) but does NOT contain the startup line '$StartupLine'."
                $failed = $true
            }
        }

        # A panic on any thread is a defect even in the unlikely event the process
        # survives it. Under panic = abort it will not, but assert on the evidence rather
        # than on the assumption.
        if (Test-Path -LiteralPath $stdErrPath) {
            $err = Get-Content -LiteralPath $stdErrPath -Raw -ErrorAction SilentlyContinue
            if ($err -and $err.Contains('panicked at')) {
                Write-Host '::error::Smoke gate: the app printed a Rust panic on stderr during startup.'
                $failed = $true
            }
        }

        if ($windowTitle) {
            Write-Host "INFO  a top-level window titled '$windowTitle' appeared $windowSeenAfterSeconds s into the settle window, so the setup closure ran through windows::init"
        }
        else {
            Write-Host 'INFO  no top-level window was observed for this process. Reported, not'
            Write-Host '      asserted: a runner session with no interactive desktop can still run'
            Write-Host '      the process and write the log, and the assertions above stand alone.'
        }
    }

    if ($failed) {
        Write-Diagnostics -Process $proc -LogDir $logDir -StdErrPath $stdErrPath -StdOutPath $stdOutPath -ScratchDir $scratchRoot
        exit 1
    }

    Write-Section 'RESULT'
    Write-Host 'Smoke gate PASSED: the built binary launched, wrote its startup line, and was'
    Write-Host 'still alive after the settle window.'
    exit 0
}
finally {
    # Terminate by PID, never by name. A developer running this locally very likely has
    # the real RepoSync in their tray; Stop-Process -Name reposync would kill it, and
    # Get-Process -Name reposync would have let this gate pass on THEIR instance without
    # ever launching the build under test.
    if ($proc -and -not $proc.HasExited) {
        Write-Host ''
        Write-Host "Terminating pid $($proc.Id) and its WebView2 child processes."
        # /T kills the tree: a Tauri app spawns msedgewebview2.exe children that would
        # otherwise be orphaned.
        try { & taskkill.exe /PID $proc.Id /T /F 2>&1 | Out-Null } catch { }
        try { $proc.WaitForExit(5000) | Out-Null } catch { }
    }

    $env:LOCALAPPDATA = $originalLocalAppData
    $env:REPOSYNC_LOG = $originalLogLevel

    if ($KeepScratch) {
        Write-Host "Scratch directory kept at $scratchRoot"
    }
    elseif (Test-Path -LiteralPath $scratchRoot) {
        # The SQLite WAL sidecars can stay locked for a moment after the process dies.
        # Cleanup failure is reported, never fatal: it must not turn a real pass red.
        $removed = $false
        foreach ($attempt in 1..5) {
            try {
                Remove-Item -LiteralPath $scratchRoot -Recurse -Force -ErrorAction Stop
                $removed = $true
                break
            }
            catch {
                Start-Sleep -Milliseconds 400
            }
        }
        if (-not $removed) {
            Write-Host "Note: could not remove the scratch directory $scratchRoot (files still locked). Not a gate failure."
        }
    }
}
