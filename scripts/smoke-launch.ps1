#Requires -Version 7.0
<#
.SYNOPSIS
    Binary smoke gate: launch the built RepoSync binary, assert it survives startup, then
    launch it a SECOND time and assert the single-instance guard turns that one away.

.NOTES
    RUNNING THIS LOCALLY: CLOSE REPOSYNC FIRST.

    Since BL-NI-73 (nothing stops two RepoSync instances sharing one database) the app
    carries a startup singleton keyed on its BUNDLE IDENTIFIER (com.reposync.app), not on
    its data directory. The scratch %LOCALAPPDATA% below therefore does NOT isolate this
    gate from an installed RepoSync sitting in your tray: the guard sees the same
    identifier and turns THIS gate's first launch away, which shows up as an immediate
    exit with code 0 before the startup line. Phase 1 says so explicitly when it sees that
    shape. The same applies to `pnpm tauri dev`, for the same reason and by design.

.DESCRIPTION
    Closes BL-NI-88 (no gate ever launches the built binary). Every other gate in this
    repo reads or exercises code WITHOUT running the app: cargo test calls library
    functions, vitest renders components, clippy and fmt read source. None of them can
    reach reposync_lib::run, which has exactly one caller - the app's own startup. That
    is how BL-NI-87 (an unreachable! in logging::init that was always reachable) survived
    19 days of green gates and two installers: only launching the built binary could
    catch it.

    It also carries the machine-checkable half of BL-NI-73 (nothing stops two RepoSync
    instances sharing one database). That guard is a property of the PROCESS - a named
    mutex claimed during plugin setup - so nothing that reads source or calls a library
    function can see whether it works. Only launching the binary twice can. Phase 4 does
    exactly that; see the section below it for what it asserts and why each assertion is
    there.

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

    NOTE on isolation scope, checked rather than assumed. The scratch LOCALAPPDATA isolates
    RepoSync's own data dir completely: across a full local run of this gate the developer's
    real %LOCALAPPDATA%\RepoSync was untouched, its newest file older than the run. It does
    NOT isolate WebView2's browser profile. Tauri points that at app_local_data_dir(), which
    resolves through the Windows known-folder API and not through the environment variable, so
    it lands in the REAL %LOCALAPPDATA%\com.reposync.app\EBWebView regardless of what this
    script sets. Harmless - it is a browser cache, not app state - and stated here so nobody
    later mistakes this for full sandboxing.

.PARAMETER ExePath
    Path to the built binary. Defaults to target/release/reposync.exe relative to the
    repository root (the Cargo workspace root IS the repo root, so pnpm tauri build and
    cargo build --release -p reposync both land it there).

.PARAMETER StartupTimeoutSeconds
    How long to poll for the startup line before giving up. See the timing note below.

.PARAMETER ReadyTimeoutSeconds
    How long to poll for the readiness marker after the startup line appears. This is the
    hang budget: a process still running with no readiness marker after this fails.

.PARAMETER SettleSeconds
    How long to keep watching the process AFTER the startup line appears. See below.

.PARAMETER SecondLaunchTimeoutSeconds
    How long to wait for the SECOND launch to exit on its own, and then how long to poll
    for the running instance's deferral line. A second instance that is still alive after
    this is the failure BL-NI-73 describes.

.PARAMETER KeepScratch
    Leave the scratch data directory in place instead of deleting it (for debugging).

.PARAMETER SelfTest
    Run this script's own log-reading assertions against synthesized fixtures and exit,
    launching nothing. Covers the daily-rollover boundary: an app started just before UTC
    midnight writes its startup line to one file and its readiness marker to the next, so
    a gate that read only the newest file would reject a healthy binary once a day. A
    required check that goes red on a good build is how a gate gets ignored and then
    switched off, so that case is worth a test rather than a comment.

.NOTES
    TIMING, and why these two numbers:

    The startup line is emitted by logging::init, which is the FIRST statement of run() -
    before the Tauri builder, before the window, before the database. So the line
    appearing proves only that logging came up. Everything that can actually kill the app
    happens AFTER it: the WebView2 window build, init_pool_with_recovery (whose .expect
    aborts), the activity-log sweep, the settings read, the autostart reconcile, the
    scheduler spawn, the tray build, and windows::init. Asserting liveness at the moment
    the line appears would assert almost nothing.

    Hence three phases rather than one fixed sleep:

      Phase 1 - poll every 500 ms for up to StartupTimeoutSeconds (default 30) for the
      log file to exist, be non-empty, and contain the startup line, failing IMMEDIATELY
      if the process dies first. Locally this resolves in well under a second; 30 s is
      headroom for a cold CI runner, and because it polls, the common case costs about a
      second rather than the whole budget.

      Phase 2 - poll for up to ReadyTimeoutSeconds (default 30) for the readiness marker,
      which is emitted after windows::init returns and therefore after every item in that
      list. This is the hang budget, and the phase that makes the gate mean anything: a
      deadlock in the setup closure leaves a process that is alive, has written its
      startup line, and will never write this one. Thirty seconds is generous against a
      path that completes in 0.5 to 3 s locally and 2.55 s on a GitHub windows-latest
      runner, because the cost of being wrong here is a flaky required check, and
      because polling means a healthy run pays none of it. That the observed spread is
      already six-fold on one machine is exactly why the headroom is large.

      Phase 3 - keep polling liveness for SettleSeconds (default 10) after readiness, then
      assert the process is still alive. Startup is complete by then, so this covers a
      death just after it: the detached auto-update check, the first scheduler cycle.

    Typical cost is about 11 seconds; worst case about 70. The build job it runs in
    already spends minutes compiling and bundling, so this is not a meaningful tax on a
    pull request. Both values are parameters so a slow runner can be accommodated without
    editing logic. Measured on a GitHub windows-latest runner: the startup line appeared at
    0.46 s, the readiness marker 2.55 s after it, and the whole step took 13 to 18 seconds
    across runs.
#>
[CmdletBinding()]
param(
    [string]$ExePath,
    [int]$StartupTimeoutSeconds = 30,
    [int]$ReadyTimeoutSeconds = 30,
    [int]$SettleSeconds = 10,
    [int]$SecondLaunchTimeoutSeconds = 20,
    [switch]$KeepScratch,
    [switch]$SelfTest
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

# The readiness marker, and the assertion that makes this gate mean something.
#
# Emitted once, after windows::init returns at the end of the setup closure in
# src-tauri/src/lib.rs, from the event vocabulary in crates/reposync-core/src/logging.rs.
# The startup line above cannot stand in for it: logging::init runs before the Tauri
# builder, so "RepoSync starting" is written by a process that has not yet opened the
# database, built the window, resolved the tray, or wired the window lifecycle. Neither
# can liveness: a process that DEADLOCKS in any of that work stays alive forever, and a
# gate that only asks "is it running" passes it. The absence of this line from a process
# that is still running is the only way a startup hang is visible from outside.
#
# It is also CONDITIONAL. The app emits it only when windows::init reports a window the
# user can actually see, and logs $WindowFailedEvent below instead when it does not. So
# its absence covers two defects, not one: a startup that never finished, and a startup
# that finished with nothing on screen. The app itself is unchanged in both cases - it
# keeps running exactly as it always has - and this gate is what turns that silence into
# a red check.
$ReadyEvent = 'app.startup_completed'

# The event the app logs INSTEAD of the readiness marker when startup finished but left
# no usable main window. The gate needs no separate assertion for it - the readiness
# marker is already absent, which already fails - but it changes what the failure MEANS,
# and a maintainer reading a red CI log should not have to guess. Without this the one
# message would have to cover both a deadlock and a window that never appeared, and
# "never became ready" reads as a hang.
$WindowFailedEvent = 'app.window_setup_failed'

# The line the RUNNING instance logs when a second launch is handed to it (BL-NI-73 -
# nothing stops two RepoSync instances sharing one database). Emitted from
# `crate::single_instance::on_second_launch`, which is where the plugin's callback runs.
#
# Asserted because "the second process exited" is NOT evidence that the guard worked: a
# process that crashes on startup also exits, and would satisfy an exit-only assertion
# perfectly. This line is the POSITIVE half - the running instance saying it was handed a
# second launch - and it comes from the process whose log appender is alive and flushing.
# The doomed second process leaves through std::process::exit(0) inside the plugin's own
# setup hook, which unwinds nothing, so ITS worker guard never drops and anything it
# queued may never reach the file. Nothing it writes can be relied on.
$DeferralEvent = 'app.second_instance_deferred'

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

# Read EVERY log file in the scratch directory as one text, oldest first.
#
# Not just the newest, and the difference is a real failure mode rather than tidiness.
# The appender rolls DAILY, so a launch that straddles UTC midnight writes its startup
# line into one file and its readiness marker into the next. Asserting against only the
# newest would then fail a perfectly healthy binary, once a day, on a required check, and
# a blocking gate that goes red on a good build is how a gate gets ignored and then
# switched off. Concatenating is safe precisely because the data directory is created
# fresh for every run: every file under it belongs to THIS launch, so no assertion here
# can be satisfied by a marker an earlier run left behind.
function Read-AllLogText {
    param([Parameter(Mandatory)][string]$LogDir)
    $files = @(Get-LogFiles -LogDir $LogDir)
    if ($files.Count -eq 0) { return '' }
    # Get-LogFiles hands back newest first; read oldest first so the text reads in order.
    [array]::Reverse($files)
    return (($files | ForEach-Object { Read-LogText -Path $_.FullName }) -join [System.Environment]::NewLine)
}

# Count occurrences of a marker across the combined log text.
#
# `-split` and regex counting are both wrong here: the markers are literal event names
# containing a dot, and a regex would quietly treat it as "any character". Ordinal
# IndexOf is literal, culture-independent, and counts non-overlapping matches, which is
# what "how many processes logged this" means.
#
# Needed because the BL-NI-73 assertion is about a COUNT rather than presence. A run of
# this gate launches the binary twice, so presence proves nothing about the second
# launch: the readiness marker being there is equally consistent with one instance
# starting and with two. Exactly one is the claim.
function Measure-Occurrence {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$Text,
        [Parameter(Mandatory)][string]$Needle
    )
    if ($Text.Length -eq 0) { return 0 }
    $count = 0
    $at = $Text.IndexOf($Needle, [System.StringComparison]::Ordinal)
    while ($at -ge 0) {
        $count++
        $at = $Text.IndexOf($Needle, $at + $Needle.Length, [System.StringComparison]::Ordinal)
    }
    return $count
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
        # Every file, oldest first: a run that crossed the daily rollover has its
        # evidence split across two of them, and printing one would hide half of it.
        $ordered = @($logs)
        [array]::Reverse($ordered)
        foreach ($f in $ordered) {
            Show-FileOrNote -Label "log contents: $($f.Name)" -Path $f.FullName
        }
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

# --- Self-test ----------------------------------------------------------------------
#
# The gate's own log-reading logic, checked against synthesized fixtures, launching
# nothing. It exists for one case that is easy to reason past and impossible to notice
# until it bites: the appender rolls DAILY, on UTC midnight, so a launch that straddles
# that instant writes its startup line into yesterday's file and its readiness marker
# into today's. A gate reading only the newest file would fail a healthy binary, once a
# day, on a REQUIRED check. That is the most corrosive kind of flake, because the cost
# lands on a maintainer who did nothing wrong and the cheapest response is to stop
# trusting the gate.
#
# Runs in CI as its own step, so the assertions this gate is built on are themselves
# gated rather than taken on faith.

function New-LogFixture {
    param(
        [Parameter(Mandatory)][string]$LogDir,
        [Parameter(Mandatory)][string]$Date,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Content,
        [Parameter(Mandatory)][datetime]$WrittenUtc
    )
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
    $path = Join-Path $LogDir "reposync.$Date.log"
    [System.IO.File]::WriteAllText($path, $Content)
    (Get-Item -LiteralPath $path).LastWriteTimeUtc = $WrittenUtc
    return $path
}

function Invoke-SelfTest {
    $failures = 0
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ('reposync-smoke-selftest-' + [guid]::NewGuid().ToString('N'))

    function Assert-True {
        param([Parameter(Mandatory)][bool]$Condition, [Parameter(Mandatory)][string]$What)
        if ($Condition) {
            Write-Host "PASS  $What"
        }
        else {
            Write-Host "::error::Smoke gate self-test FAILED: $What"
            $script:selfTestFailures++
        }
    }

    $script:selfTestFailures = 0

    try {
        # 1. The rollover case. A launch just before UTC midnight leaves the startup line
        #    in yesterday's file and the readiness marker in today's. Both must be found.
        $split = Join-Path $root 'split\logs'
        New-LogFixture -LogDir $split -Date '2026-09-04' -WrittenUtc ([datetime]::new(2026, 9, 4, 23, 59, 59, [System.DateTimeKind]::Utc)) `
            -Content "2026-09-04T23:59:59Z  INFO reposync_lib::logging: $StartupLine version=`"0.9.0`"" | Out-Null
        New-LogFixture -LogDir $split -Date '2026-09-05' -WrittenUtc ([datetime]::new(2026, 9, 5, 0, 0, 1, [System.DateTimeKind]::Utc)) `
            -Content "2026-09-05T00:00:01Z  INFO reposync_lib: startup finished event=`"$ReadyEvent`"" | Out-Null

        $text = Read-AllLogText -LogDir $split
        Assert-True -Condition ($text.Contains($StartupLine)) -What 'rollover: the startup line is found in the OLDER file'
        Assert-True -Condition ($text.Contains($ReadyEvent)) -What 'rollover: the readiness marker is found in the NEWER file'
        Assert-True -Condition ($text.Length -gt 0) -What 'rollover: the combined content is non-empty'
        Assert-True -Condition ($text.IndexOf($StartupLine) -ge 0 -and $text.IndexOf($StartupLine) -lt $text.IndexOf($ReadyEvent)) -What 'rollover: files are concatenated oldest first, so the text reads in order'

        # 2. The ordinary case: one file carrying both markers still works.
        $single = Join-Path $root 'single\logs'
        New-LogFixture -LogDir $single -Date '2026-09-05' -WrittenUtc ([datetime]::new(2026, 9, 5, 12, 0, 0, [System.DateTimeKind]::Utc)) `
            -Content "$StartupLine`nevent=`"$ReadyEvent`"" | Out-Null
        $text = Read-AllLogText -LogDir $single
        Assert-True -Condition ($text.Contains($StartupLine) -and $text.Contains($ReadyEvent)) -What 'single file: both markers are found'

        # 3. Non-empty must mean CONTENT. A directory of empty files is still empty, which
        #    is exactly the shape an aborted process leaves behind under panic = abort,
        #    and reading across several files must not blur that into "some file exists".
        $empty = Join-Path $root 'empty\logs'
        New-LogFixture -LogDir $empty -Date '2026-09-04' -WrittenUtc ([datetime]::new(2026, 9, 4, 23, 59, 59, [System.DateTimeKind]::Utc)) -Content '' | Out-Null
        New-LogFixture -LogDir $empty -Date '2026-09-05' -WrittenUtc ([datetime]::new(2026, 9, 5, 0, 0, 1, [System.DateTimeKind]::Utc)) -Content '' | Out-Null
        $text = Read-AllLogText -LogDir $empty
        Assert-True -Condition ($text.Trim().Length -eq 0) -What 'two empty files still read as empty, so the crash shape is not masked'
        Assert-True -Condition (@(Get-LogFiles -LogDir $empty).Count -eq 2) -What 'two empty files are still FOUND, so the failure is reported as empty rather than as missing'

        # 4. A directory that does not exist yet reads as empty rather than throwing.
        $missing = Join-Path $root 'missing\logs'
        Assert-True -Condition ((Read-AllLogText -LogDir $missing).Length -eq 0) -What 'a log directory that does not exist reads as empty'
        Assert-True -Condition (@(Get-LogFiles -LogDir $missing).Count -eq 0) -What 'a log directory that does not exist lists no files'

        # 5. Counting, which is what the BL-NI-73 (nothing stops two RepoSync instances
        #    sharing one database) assertion rests on. Presence is not enough there: this
        #    gate launches the binary TWICE, so "the readiness marker is present" is
        #    equally true whether one instance started or two. Exactly one is the claim,
        #    and it must survive the marker landing in two different daily files.
        Assert-True -Condition ((Measure-Occurrence -Text "a $ReadyEvent b" -Needle $ReadyEvent) -eq 1) -What 'counting: one occurrence is counted once'
        Assert-True -Condition ((Measure-Occurrence -Text "$ReadyEvent`n$ReadyEvent" -Needle $ReadyEvent) -eq 2) -What 'counting: two occurrences are counted twice, which is the guard failure this gate exists to catch'
        Assert-True -Condition ((Measure-Occurrence -Text 'nothing here' -Needle $ReadyEvent) -eq 0) -What 'counting: an absent marker counts zero'
        Assert-True -Condition ((Measure-Occurrence -Text '' -Needle $ReadyEvent) -eq 0) -What 'counting: empty text counts zero rather than throwing'
        # The event names contain dots. A regex-based count would read those as "any
        # character" and match a line that merely resembles the event, so the count is
        # ordinal and literal. This fixture would match under a regex and must not here.
        Assert-True -Condition ((Measure-Occurrence -Text 'appXstartupXcompleted' -Needle $ReadyEvent) -eq 0) -What 'counting: the dots in the event name are literal, not regex wildcards'

        $counted = Join-Path $root 'counted\logs'
        New-LogFixture -LogDir $counted -Date '2026-09-04' -WrittenUtc ([datetime]::new(2026, 9, 4, 23, 59, 59, [System.DateTimeKind]::Utc)) -Content "event=`"$ReadyEvent`"" | Out-Null
        New-LogFixture -LogDir $counted -Date '2026-09-05' -WrittenUtc ([datetime]::new(2026, 9, 5, 0, 0, 1, [System.DateTimeKind]::Utc)) -Content "event=`"$ReadyEvent`"" | Out-Null
        Assert-True -Condition ((Measure-Occurrence -Text (Read-AllLogText -LogDir $counted) -Needle $ReadyEvent) -eq 2) -What 'counting: markers in two daily files are counted across both, so a rollover cannot hide a second instance'

        # 6. Only OUR files count. The appender names every file reposync.<date>.log, and
        #    anything else in that directory is not ours to assert on.
        $mixed = Join-Path $root 'mixed\logs'
        New-LogFixture -LogDir $mixed -Date '2026-09-05' -WrittenUtc ([datetime]::new(2026, 9, 5, 1, 0, 0, [System.DateTimeKind]::Utc)) -Content $StartupLine | Out-Null
        [System.IO.File]::WriteAllText((Join-Path $mixed 'notes.txt'), 'not a log file')
        Assert-True -Condition (@(Get-LogFiles -LogDir $mixed).Count -eq 1) -What 'a non-matching file in the log directory is ignored'
    }
    finally {
        Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
    }

    $failures = $script:selfTestFailures
    Write-Host ''
    if ($failures -eq 0) {
        Write-Host 'Smoke gate self-test PASSED.'
        return 0
    }
    Write-Host "Smoke gate self-test FAILED with $failures assertion failure(s)."
    return 1
}

if ($SelfTest) {
    Write-Section 'Smoke gate self-test (log reading, launching nothing)'
    exit (Invoke-SelfTest)
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
Write-Host "readiness marker: event=$ReadyEvent"
Write-Host "window failure  : event=$WindowFailedEvent"
Write-Host "log file glob   : logs\$LogGlob"
Write-Host "startup timeout : $StartupTimeoutSeconds s (polled every $PollMs ms)"
Write-Host "ready timeout   : $ReadyTimeoutSeconds s after the startup line"
Write-Host "settle window   : $SettleSeconds s after the readiness marker"

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
# The second launch gets its OWN capture files. The first process still holds handles on
# the pair above for its whole life, so reusing them would fail the second Start-Process.
$stdErrPath2 = Join-Path $scratchRoot 'stderr-second.txt'
$stdOutPath2 = Join-Path $scratchRoot 'stdout-second.txt'

Write-Section 'Scratch isolation'
Write-Host "LOCALAPPDATA    : $scratchRoot"
Write-Host "expected data   : $dataDir"
Write-Host "expected logs   : $logDir"

$originalLocalAppData = $env:LOCALAPPDATA
$originalLogLevel = $env:REPOSYNC_LOG
$proc = $null
$proc2 = $null
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
            # A clean exit code 0 at this point has one overwhelmingly likely cause, and
            # it is not a defect in the build. It is the signature of the BL-NI-73
            # single-instance guard turning THIS launch away, which happens when RepoSync
            # is already running in this logon session. The guard keys on the bundle
            # identifier, so the scratch %LOCALAPPDATA% above does not isolate the gate
            # from an installed copy. Saying so here costs one line and saves a
            # maintainer from debugging a healthy binary.
            if ($proc.ExitCode -eq 0) {
                Write-Host '::error::Smoke gate: that exit was CLEAN (code 0), which is what the single-instance guard (BL-NI-73) does to a second launch. A RepoSync instance is almost certainly already running in this logon session - close it (tray icon -> Quit) and run this again. The guard keys on the bundle identifier com.reposync.app, so pointing LOCALAPPDATA at a scratch directory does not hide this gate from an installed copy.'
            }
            $failed = $true
            break
        }

        $logs = @(Get-LogFiles -LogDir $logDir)
        if ($logs.Count -gt 0) {
            $content = Read-AllLogText -LogDir $logDir
            if ($content.Length -gt 0 -and $content.Contains($StartupLine)) {
                $sawStartupLine = $true
                Write-Host "startup line seen after $([math]::Round($sw.Elapsed.TotalSeconds, 2)) s ($($content.Length) chars across $($logs.Count) file(s))"
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
            $content = Read-AllLogText -LogDir $logDir
            if ($content.Length -eq 0) {
                Write-Host "::error::Smoke gate: after $StartupTimeoutSeconds s the log file(s) under '$logDir' exist but reading them yields ZERO BYTES. The appender opens that file when logging::init builds it, so an empty one means nothing was ever flushed to it - either the process aborted before the queue drained (the panic = abort shape, and what BL-NI-87 looked like), or it is still running and the startup line is no longer being emitted at all."
            }
            else {
                Write-Host "::error::Smoke gate: after $StartupTimeoutSeconds s the log is non-empty ($($content.Length) chars across $($logs.Count) file(s)) but does NOT contain the startup line '$StartupLine'. Either the app is stuck before logging::init finished, or that line changed and this gate needs updating."
            }
        }
        $failed = $true
    }

    # --- Phase 2: poll for the readiness marker ---------------------------------------
    #
    # This is the phase the first version of this gate did not have, and the one that
    # catches a HANG. A process wedged anywhere in the setup closure stays alive and keeps
    # its startup line on disk, so phase 1 and a liveness check both pass it. Only the
    # absence of this marker, from a process that is still running, says "it started and
    # never finished starting".

    $sawReady = $false

    if (-not $failed) {
        Write-Section "Readiness (up to $ReadyTimeoutSeconds s)"
        Write-Host "Waiting for event=$ReadyEvent, emitted after windows::init returns. Everything"
        Write-Host 'that can hang lies between the startup line and this marker: the WebView2 window'
        Write-Host 'build, the database open and migrations, the activity sweep, the settings read,'
        Write-Host 'the autostart reconcile, the scheduler spawn, the tray build, and the window'
        Write-Host 'lifecycle.'

        $ready = [System.Diagnostics.Stopwatch]::StartNew()
        while ($ready.Elapsed.TotalSeconds -lt $ReadyTimeoutSeconds) {
            if ($proc.HasExited) {
                Write-Host "::error::Smoke gate: DIED DURING STARTUP. The app wrote its startup line and then EXITED $([math]::Round($ready.Elapsed.TotalSeconds, 2)) s later, before reaching readiness (event=$ReadyEvent). Exit code $(Format-ExitCode -Code $proc.ExitCode). The captured stderr below carries the panic text if there was one."
                $failed = $true
                break
            }

            $logs = @(Get-LogFiles -LogDir $logDir)
            if ($logs.Count -gt 0) {
                $content = Read-AllLogText -LogDir $logDir
                if ($content.Contains($ReadyEvent)) {
                    $sawReady = $true
                    Write-Host "readiness marker seen after $([math]::Round($ready.Elapsed.TotalSeconds, 2)) s"
                    break
                }
            }

            Start-Sleep -Milliseconds $PollMs
        }

        if (-not $failed -and -not $sawReady) {
            # Two different defects reach this point, and they want different words. The
            # app either never got to the end of startup (a hang), or it got there and
            # deliberately withheld the marker because the window it produced was not
            # usable. The log says which, so read it rather than guessing.
            $tail = Read-AllLogText -LogDir $logDir

            if ($tail.Contains($WindowFailedEvent)) {
                Write-Host "::error::Smoke gate: STARTED BUT THE WINDOW NEVER CAME UP. The app finished its startup path and then logged event=$WindowFailedEvent instead of event=$ReadyEvent, which it does when there is no usable main window - either no 'main' window at all, or a launch that could not show the one it had. The process is alive and nothing crashed, so it would have passed a liveness check while presenting the user with nothing on screen. This is NOT a hang; the log line below names which part failed and carries the OS error."
            }
            else {
                Write-Host "::error::Smoke gate: STARTED BUT NEVER BECAME READY. The app is STILL RUNNING $ReadyTimeoutSeconds s after writing its startup line, and logged neither event=$ReadyEvent nor event=$WindowFailedEvent. Reaching the end of startup produces one or the other, so it never got there: it is wedged somewhere in the setup closure - the window build, the database open and migrations, the activity sweep, the settings read, the autostart reconcile, the scheduler spawn, the tray build, or the window lifecycle. This is a startup HANG. Nothing crashed, so a liveness-only check would have passed it. The log below is everything it managed to write."
            }
            $failed = $true
        }
    }

    # --- Phase 3: settle, then assert it did not die immediately after becoming ready --

    if (-not $failed) {
        Write-Section "Settle ($SettleSeconds s)"
        Write-Host 'The Rust startup path is complete at this point. This window catches a death'
        Write-Host 'just AFTER readiness: the detached auto-update check, the first scheduler'
        Write-Host 'cycle, or anything else that only runs once the app is up.'

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

        # Assert across EVERY file this run produced, not just the newest, so a launch
        # that crossed the daily UTC rollover is not failed for having written its two
        # markers either side of the boundary. "Non-empty" stays a claim about CONTENT:
        # it is the length of what was actually read, so a directory of empty files
        # still fails.
        $logs = @(Get-LogFiles -LogDir $logDir)
        $content = Read-AllLogText -LogDir $logDir
        if ($logs.Count -eq 0) {
            Write-Host "::error::Smoke gate: the log file disappeared from '$logDir'."
            $failed = $true
        }
        elseif ($content.Length -eq 0) {
            Write-Host "::error::Smoke gate: the $($logs.Count) log file(s) under '$logDir' read as ZERO BYTES. Under panic = abort an empty log is what a crash looks like."
            $failed = $true
        }
        else {
            Write-Host "PASS  log content is non-empty ($($content.Length) chars across $($logs.Count) file(s))"

            if ($content.Contains($StartupLine)) {
                Write-Host "PASS  log contains the startup line '$StartupLine'"
            }
            else {
                Write-Host "::error::Smoke gate: the log is non-empty ($($content.Length) chars) but does NOT contain the startup line '$StartupLine'."
                $failed = $true
            }

            if ($content.Contains($ReadyEvent)) {
                Write-Host "PASS  log contains the readiness marker event=$ReadyEvent"
            }
            else {
                Write-Host "::error::Smoke gate: the log no longer contains the readiness marker event=$ReadyEvent."
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

    # --- Phase 4: launch it AGAIN and assert the singleton guard turns that one away --
    #
    # BL-NI-73 (nothing stops two RepoSync instances sharing one database). This is the
    # machine-checkable half of the paired verification that row demands; the other half
    # is a human launching the packaged app twice and confirming the first window comes
    # to the front with no second tray icon, which no CI runner can see.
    #
    # Why this needs a launch at all: the guard is a property of the PROCESS. It is a
    # named mutex claimed in a plugin setup hook, so cargo test, clippy and vitest cannot
    # reach it - the same gap BL-NI-88 (no gate ever launches the built binary) exists to
    # close, one layer in.
    #
    # FOUR assertions, and each covers something the others do not:
    #
    #   1. The second process EXITS ON ITS OWN, within the budget. This is the assertion
    #      the whole row is about. Without the guard it simply keeps running, and a
    #      second RepoSync over one data directory is two schedulers, two sets of git
    #      processes in the SAME working trees, and racing writes to repo_local_state.
    #   2. Its exit code is 0. "It exited" is also true of "it crashed", and a crash
    #      would be a different and worse defect wearing the same evidence. The plugin's
    #      Windows path leaves through std::process::exit(0), so a clean zero is the
    #      guard's own signature.
    #   3. The FIRST process is still alive. A guard that turned away the wrong instance,
    #      or took both down, would satisfy 1 and 2 perfectly.
    #   4. The readiness marker appears EXACTLY ONCE across the run's logs. This is the
    #      one that survives a broken guard failing in an unexpected shape: two instances
    #      that both complete startup write it twice, whatever they do afterwards.
    #      Presence alone proves nothing here, because this run launches the binary twice
    #      by design.
    #
    # Plus one positive check that is not strictly an assertion about the second process
    # at all: the RUNNING instance must log the deferral event. That is the difference
    # between "the guard turned it away" and "it happened to die", which assertions 1
    # to 3 cannot tell apart on their own.
    #
    # NOT asserted: that the window actually came to the FRONT. On Windows the right to
    # call SetForegroundWindow belongs to the process the user just launched - the second
    # instance - not to the one receiving the message, so a perfectly working guard can
    # legitimately end with a flashing taskbar button. The app records that as a
    # focus_failed field rather than a failure, and so does this gate. Confirming the
    # raise visually is part of the human verification the backlog row requires.
    #
    # The startup LINE is deliberately not counted. It appears once per process, so a
    # count of it would be 2 on a healthy run - and could be 1, because the second
    # process leaves through process::exit, which unwinds nothing, so its log worker
    # never flushes and its line may never land. Nothing about that line is a stable
    # signal here, which is exactly why the count is taken on the readiness marker
    # instead: the second instance provably never reaches the code that emits it, since
    # the guard runs in the FIRST plugin's setup hook, long before the setup closure.

    if (-not $failed) {
        Write-Section "Second launch (BL-NI-73 single-instance guard, up to $SecondLaunchTimeoutSeconds s)"
        Write-Host 'Launching the SAME binary again against the SAME scratch data directory, which'
        Write-Host 'is what a user double-clicking the shortcut a second time does. The running'
        Write-Host 'instance should take the launch over and the new process should exit by itself.'

        try {
            $proc2 = Start-Process -FilePath $ExePath `
                -WorkingDirectory $scratchRoot `
                -RedirectStandardError $stdErrPath2 `
                -RedirectStandardOutput $stdOutPath2 `
                -PassThru
        }
        catch {
            Write-Host "::error::Smoke gate: could not start the second instance from '$ExePath': $($_.Exception.Message)"
            Write-Diagnostics -Process $proc -LogDir $logDir -StdErrPath $stdErrPath -StdOutPath $stdOutPath -ScratchDir $scratchRoot
            exit 1
        }

        Write-Host "second instance started as pid $($proc2.Id) at $(Get-Date -Format 'HH:mm:ss.fff')"

        $second = [System.Diagnostics.Stopwatch]::StartNew()
        while ($second.Elapsed.TotalSeconds -lt $SecondLaunchTimeoutSeconds -and -not $proc2.HasExited) {
            Start-Sleep -Milliseconds $PollMs
        }

        if (-not $proc2.HasExited) {
            Write-Host "::error::Smoke gate: THE SECOND INSTANCE IS STILL RUNNING $SecondLaunchTimeoutSeconds s after launch (pid $($proc2.Id)), while the first (pid $($proc.Id)) is also alive. The single-instance guard did not turn it away, so this build allows two RepoSync processes over one data directory: two schedulers, two git processes in the SAME working trees - which the in-memory per-repo lock map cannot serialize across processes and which can corrupt an index - and racing read-modify-writes on repo_local_state. This is BL-NI-73 exactly as filed."
            $failed = $true
        }
        else {
            Write-Host "second instance exited on its own after $([math]::Round($second.Elapsed.TotalSeconds, 2)) s, exit code $(Format-ExitCode -Code $proc2.ExitCode)"

            if ($proc2.ExitCode -eq 0) {
                Write-Host 'PASS  the second launch exited on its own, cleanly (code 0)'
            }
            else {
                Write-Host "::error::Smoke gate: the second instance exited, but NOT cleanly - exit code $(Format-ExitCode -Code $proc2.ExitCode). The guard leaves through std::process::exit(0), so a non-zero code means the second process CRASHED rather than being turned away, which would satisfy an exit-only check while being a worse defect than the one this asserts against. Its captured stderr is printed below."
                $failed = $true
            }

            if ($proc.HasExited) {
                Write-Host "::error::Smoke gate: the FIRST instance (pid $($proc.Id)) is no longer running after the second launch. Exit code $(Format-ExitCode -Code $proc.ExitCode). A second launch must be turned away WITHOUT disturbing the instance the user already had; taking the running app down is worse than allowing two."
                $failed = $true
            }
            else {
                Write-Host "PASS  the first instance is still alive (pid $($proc.Id))"
            }
        }

        # Poll for the deferral line rather than reading once. The callback runs
        # synchronously in the running instance while the second process is still blocked
        # handing over its arguments, so by now it HAS run - but the log writer is
        # non-blocking, so the line reaches disk a moment later.
        $deferral = [System.Diagnostics.Stopwatch]::StartNew()
        $sawDeferral = $false
        while ($deferral.Elapsed.TotalSeconds -lt $SecondLaunchTimeoutSeconds) {
            if ((Read-AllLogText -LogDir $logDir).Contains($DeferralEvent)) {
                $sawDeferral = $true
                break
            }
            Start-Sleep -Milliseconds $PollMs
        }

        if ($sawDeferral) {
            Write-Host "PASS  the running instance logged event=$DeferralEvent, so the second launch was DEFERRED to it rather than merely dying"
        }
        else {
            # Two different situations reach this point and they mean different things.
            # If the second process is STILL RUNNING, the absent deferral line is simply
            # the same failure the phase already reported, restated - the guard did not
            # fire, so of course nothing logged that it did. If the second process is
            # GONE, this is the interesting case: something turned it away, or it died,
            # and the missing line is the only thing that could have told them apart.
            # Saying "the second process is gone" in both would be the gate asserting a
            # fact it did not check.
            if ($proc2.HasExited) {
                Write-Host "::error::Smoke gate: the second process EXITED but the running instance never logged event=$DeferralEvent. Every assertion above is also satisfied by a second instance that CRASHED on startup, so without this line there is no evidence the guard did anything. Either the single-instance callback did not fire, or it fired and the running instance's log writer is no longer reaching disk."
            }
            else {
                Write-Host "::error::Smoke gate: the running instance never logged event=$DeferralEvent, which follows from the second instance still running above: nothing was deferred to it. Reported separately because this line is what distinguishes a guard that fired from a second process that merely died, and it is absent."
            }
            $failed = $true
        }

        $content = Read-AllLogText -LogDir $logDir
        $readyCount = Measure-Occurrence -Text $content -Needle $ReadyEvent
        if ($readyCount -eq 1) {
            Write-Host "PASS  event=$ReadyEvent appears EXACTLY ONCE across this run's logs, so only one instance completed startup"
        }
        else {
            Write-Host "::error::Smoke gate: event=$ReadyEvent appears $readyCount time(s) across this run's logs; exactly 1 is required. This run launched the binary twice against one data directory, and only the first launch may reach the end of startup. More than one means the guard let a second instance through. Zero means the marker vanished from a log that carried it moments ago, which is a different failure and just as much a red gate."
            $failed = $true
        }
    }

    if ($failed) {
        Write-Diagnostics -Process $proc -LogDir $logDir -StdErrPath $stdErrPath -StdOutPath $stdOutPath -ScratchDir $scratchRoot
        if ($proc2) {
            Write-Host ''
            if ($proc2.HasExited) {
                Write-Host "second instance: EXITED, exit code $(Format-ExitCode -Code $proc2.ExitCode)"
            }
            else {
                Write-Host "second instance: still running (pid $($proc2.Id))"
            }
            Show-FileOrNote -Label 'captured stderr (second instance)' -Path $stdErrPath2
            Show-FileOrNote -Label 'captured stdout (second instance)' -Path $stdOutPath2
        }
        exit 1
    }

    Write-Section 'RESULT'
    Write-Host 'Smoke gate PASSED: the built binary launched, wrote its startup line, reached'
    Write-Host 'readiness, was still alive after the settle window, and turned a second launch'
    Write-Host 'away without disturbing itself.'
    exit 0
}
finally {
    # Terminate by PID, never by name. A developer running this locally very likely has
    # the real RepoSync in their tray; Stop-Process -Name reposync would kill it, and
    # Get-Process -Name reposync would have let this gate pass on THEIR instance without
    # ever launching the build under test.
    # Both launches, and the second one FIRST. On a healthy run it is already gone; on
    # the run this gate exists to fail - the guard missing or broken - it is a live
    # second RepoSync holding the scratch database open, and leaving it behind would
    # both leak a process and block the cleanup below.
    foreach ($p in @($proc2, $proc)) {
        if ($p -and -not $p.HasExited) {
            Write-Host ''
            Write-Host "Terminating pid $($p.Id) and its WebView2 child processes."
            # /T kills the tree: a Tauri app spawns msedgewebview2.exe children that
            # would otherwise be orphaned.
            try { & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null } catch { }
            try { $p.WaitForExit(5000) | Out-Null } catch { }
        }
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
