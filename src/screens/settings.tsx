import { useEffect, useId, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { Loader2, RefreshCw, RotateCcw, Save } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { Settings, UpdateAvailability } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { FieldLabelContext } from "@/components/ui/field-label";
import { AsyncPanel } from "@/components/async-panel";
import { DiagnosticsCard } from "@/components/diagnostics-card";
import { PageShell } from "@/components/page-shell";
import { useSettings } from "@/hooks/queries";
import { useAppVersion } from "@/hooks/use-app-version";
import { hhMmToMinutes, minutesToHhMm } from "@/lib/time";
import { useToast } from "@/hooks/use-toast";

/**
 * AC-3 (E-21, composite pin 28, register F.3 / STG1): the screen's own
 * section list, in document order. A single source of truth for three
 * things that must otherwise drift apart - the nav rail's links, the ids the
 * scroll-spy observer watches, and each `<section>` wrapper below - so
 * adding or reordering a section only ever means editing this array.
 *
 * F.3 (`_local/design/3-decisions/2026-09-22_ui-delivery-plan.md`) is a
 * prose recommendation answering jp's own "what is standard" question:
 * "a docked section nav with scroll-spy ... one page, anchor links, the
 * highlight follows the scroll position, clicking one jumps to that
 * section." Register line 417 separately lists STG1's bench mark as "not
 * marked" and F.4 says Settings' whole redesign "waits on STG1 and the
 * STG2 hedge" - the register's own verdict was NOT ready. The E-21 spec
 * treats composite pin 28 as settled build work regardless and rules
 * re-deciding it out of scope; this file follows the spec, not the register,
 * and that tension is worth naming rather than quietly resolving.
 */
const SETTINGS_SECTIONS: { id: string; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "schedule", label: "Schedule" },
  { id: "notifications", label: "Notifications" },
  { id: "system", label: "System" },
  { id: "updates", label: "Updates" },
  { id: "integrations", label: "Integrations" },
  { id: "diagnostics", label: "Diagnostics" },
  { id: "about", label: "About" },
];
const SECTION_IDS = SETTINGS_SECTIONS.map((s) => s.id);

/**
 * Tracks which Settings section is currently in view, via
 * `IntersectionObserver` rather than only responding to a nav click (AC-3's
 * own wording: "the marker follows scroll position rather than only
 * responding to clicks").
 *
 * Everything the observer callback touches is `useState`, never a `useRef`:
 * this codebase's lint config (`react-hooks/refs`, the React Compiler rule)
 * flags a closure created during render that COULD read a ref's `.current`
 * when invoked, even one only ever invoked later, asynchronously, by the
 * browser - which is exactly the shape a ref-based "which sections are
 * currently intersecting" accumulator would have. `intersecting` therefore
 * updates functionally (`setIntersecting((prev) => ...)`) instead of
 * mutating a ref in place, and which section is "current" is derived in a
 * plain `useEffect` that runs whenever that set changes.
 *
 * The observer instance itself is created via `useState`'s LAZY initializer
 * form (`useState(() => ...)`), which runs exactly once at mount - early
 * enough that every section's `ref=` callback (fired during that same
 * mount's commit) already has a real observer to call `.observe()` on.
 * `typeof IntersectionObserver === "undefined"` guards jsdom test
 * environments that do not stub it (a plain `typeof` check on an undeclared
 * global never throws), so a test that never mounts this hook's caller keeps
 * working with no scroll-spy at all rather than crashing.
 *
 * `sectionRef(id)` is NOT identity-memoized across renders (a memoization
 * cache is itself the same ref-during-render shape this file avoids
 * elsewhere). A fresh render therefore hands each section a new ref
 * callback, which makes React detach-then-reattach it - but `observe()`ing
 * an element IntersectionObserver already watches is a no-op per spec, not
 * an error, so the only cost is a redundant call on every re-render of the
 * form above, not a correctness bug or a missed observation.
 *
 * Threshold geometry: a section counts as "current" once it has crossed a
 * band roughly 15%-25% down the viewport, the standard IntersectionObserver
 * scrollspy recipe. This is a best-effort default, NOT measured against the
 * packaged app's real rendered header height - the exact percentages were
 * not verified in a live browser.
 */
function useSectionScrollSpy(sectionIds: readonly string[]) {
  const [intersecting, setIntersecting] = useState<ReadonlySet<string>>(() => new Set());

  const [observer] = useState<IntersectionObserver | null>(() => {
    if (typeof IntersectionObserver === "undefined") return null;
    return new IntersectionObserver(
      (entries) => {
        setIntersecting((prev) => {
          const next = new Set(prev);
          for (const entry of entries) {
            const id = (entry.target as HTMLElement).dataset.sectionId;
            if (!id) continue;
            if (entry.isIntersecting) next.add(id);
            else next.delete(id);
          }
          return next;
        });
      },
      { rootMargin: "-15% 0px -75% 0px", threshold: 0 },
    );
  });

  // Derived during render, not stored in its own state: "current" is a pure
  // function of `intersecting`, so a separate `activeId` state kept in sync
  // via an effect would just be a second copy that could fall a render
  // behind (and this lint config specifically flags `setState` calls inside
  // an effect body for that reason). Document order, first match - with
  // several sections tall enough to intersect the band at once, the
  // earliest one in `sectionIds` wins, so the marker never has to pick
  // between two simultaneous reports. Falls back to the first section
  // before anything has been observed yet (mount, or no
  // `IntersectionObserver` at all).
  const activeId = sectionIds.find((id) => intersecting.has(id)) ?? sectionIds[0];

  // Only the FULL screen unmounting (navigating away from Settings) tears
  // down the observer today; no section here unmounts on its own while the
  // screen stays up, so there is no per-section unobserve path to write yet.
  useEffect(() => {
    return () => observer?.disconnect();
  }, [observer]);

  function sectionRef(id: string) {
    return (el: HTMLElement | null) => {
      if (el && observer) {
        el.dataset.sectionId = id;
        observer.observe(el);
      }
    };
  }

  return { activeId, sectionRef };
}

type SectionRef = (id: string) => (el: HTMLElement | null) => void;

/**
 * The docked nav itself. Rendered into `PageShell`'s sticky `toolbar` slot
 * (see `SettingsScreen` below) rather than as a separate vertical rail
 * beside the content: `PageShell`'s header is already `sticky top-0`, so
 * putting the nav there docks it for free, with no sticky-offset math
 * against a header height that would otherwise have to be measured and
 * hand-tuned. "Rail" in the acceptance criterion is read here as "a list of
 * section links that stays visible while scrolling," not as a specific
 * vertical-sidebar layout - a horizontal docked band is a valid instance
 * of that.
 *
 * Every link is a real `<a href="#id">`. That alone is what makes every
 * section reachable by keyboard (Tab moves to it, Enter/Space activates the
 * browser's native fragment jump) - no custom key handling was written or
 * is needed for that half of AC-3.
 *
 * This is the NAV STRUCTURE only. The card-per-section visual treatment the
 * maintainer pointed at on the composite
 * (`_local/design/_reference/ui-screenshots/settings/2026-09-22_14-53-52.png`
 * - a filled header band per card, with an icon and the section name) is
 * unratified and deliberately NOT built here; this file builds only the
 * section structure that treatment would apply to.
 */
function SettingsSectionNav({ activeId }: { activeId: string }) {
  return (
    <nav aria-label="Settings sections">
      <ul className="flex flex-wrap gap-x-4 gap-y-1">
        {SETTINGS_SECTIONS.map((section) => {
          const current = activeId === section.id;
          return (
            <li key={section.id}>
              <a
                href={`#${section.id}`}
                aria-current={current ? "location" : undefined}
                className={cn(
                  "text-sm transition-colors",
                  current
                    ? "font-semibold text-foreground"
                    : "font-medium text-muted-foreground hover:text-foreground",
                )}
              >
                {section.label}
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

export function SettingsScreen({ dark, onToggleTheme }: { dark: boolean; onToggleTheme: () => void }) {
  const settings = useSettings();
  const { activeId, sectionRef } = useSectionScrollSpy(SECTION_IDS);

  return (
    <PageShell title="Settings" width="narrow" toolbar={<SettingsSectionNav activeId={activeId} />}>

      {/*
        Appearance sits OUTSIDE the AsyncPanel on purpose. Theme is not a
        `Settings` field: it has no column, no migration and no wire
        representation, it is React state owned by `AppShell`. Rendering it
        inside the panel would tie the app's only theme control to a database
        read, so a failed `settings_get` - precisely when someone may be staring
        at an error they are struggling to read - would leave no way to switch
        themes at all. It has no data dependency, so it does not sit behind one.
      */}
      <section
        id="appearance"
        ref={sectionRef("appearance")}
        data-testid="settings-section-appearance"
        className="scroll-mt-32"
      >
        <Card>
          <CardHeader>
            <CardTitle>Appearance</CardTitle>
          </CardHeader>
          <CardContent className="p-0">
            <Field
              label="Dark theme"
              hint="Applies immediately. Not saved yet - RepoSync starts in light theme each launch."
            >
              <Switch checked={dark} onCheckedChange={() => onToggleTheme()} />
            </Field>
          </CardContent>
        </Card>
      </section>

      <AsyncPanel state={settings}>
        {(s) => <SettingsForm initial={s} onSaved={settings.refetch} sectionRef={sectionRef} />}
      </AsyncPanel>
    </PageShell>
  );
}

function SettingsForm({
  initial,
  onSaved,
  sectionRef,
}: {
  initial: Settings;
  onSaved: () => void;
  sectionRef: SectionRef;
}) {
  const toast = useToast();
  const [draft, setDraft] = useState<Settings>(initial);
  const [saving, setSaving] = useState(false);

  const dirty = useMemo(() => JSON.stringify(draft) !== JSON.stringify(initial), [draft, initial]);
  const quietOn = draft.quietHoursStart !== null && draft.quietHoursEnd !== null;

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    setDraft((d) => ({ ...d, [key]: value }));
  }

  async function save() {
    setSaving(true);
    try {
      await unwrap(commands.settingsSet(draft));
      toast("ok", "Settings saved");
      onSaved();
    } catch (e) {
      toast("error", "Could not save settings", e instanceof IpcError ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <section id="schedule" ref={sectionRef("schedule")} data-testid="settings-section-schedule" className="scroll-mt-32">
      <Card>
        <CardHeader>
          <CardTitle>Schedule</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Field label="Global cadence" hint="Minutes between automatic checks.">
            <NumberInput
              value={draft.globalCheckMinutes}
              min={1}
              onChange={(v) => set("globalCheckMinutes", v)}
              suffix="min"
            />
          </Field>
          {/*
            The hint used to say only "Pause scheduled checks during a daily
            window", which describes half of what the setting does. The same
            `in_quiet_hours` predicate gates the notification path in
            `notify::passes_gate`, so turning this on also silences tray
            notifications, and someone enabling it to stop overnight git
            activity was never told that.

            "Run when the window ends" is the scheduler's actual behavior and
            worth saying: there is no deferred queue, a repo that came due
            inside the window simply becomes selectable again on the first tick
            after it, so nothing is lost.

            BL-NI-80 then made the notification half true in the same way. A
            cycle can START outside the window and FINISH inside it, and those
            notifications used to be discarded outright rather than withheld.
            They are now held and delivered on the first tick after the window
            ends, so "silence" means postponed here, not lost, and the hint says
            which one it means.
          */}
          <Field
            label="Quiet hours"
            hint="Pause scheduled checks and silence notifications during a daily window (local clock). When the window ends, paused checks resume and any notifications held back during it arrive then."
          >
            <Switch
              checked={quietOn}
              onCheckedChange={(on) => {
                // Stored as minute-of-day (0..1439), which is what the scheduler
                // compares against. Defaults: 22:00 to 07:00.
                set("quietHoursStart", on ? 22 * 60 : null);
                set("quietHoursEnd", on ? 7 * 60 : null);
              }}
            />
          </Field>
          {quietOn && (
            <Field label="Quiet window" hint="From start time to end time, your local clock.">
              <TimeInput
                aria-label="Quiet window start"
                value={draft.quietHoursStart ?? 0}
                onChange={(v) => set("quietHoursStart", v)}
              />
              <span className="text-xs text-muted-foreground">to</span>
              <TimeInput
                aria-label="Quiet window end"
                value={draft.quietHoursEnd ?? 0}
                onChange={(v) => set("quietHoursEnd", v)}
              />
            </Field>
          )}
        </CardContent>
      </Card>
      </section>

      <section id="notifications" ref={sectionRef("notifications")} data-testid="settings-section-notifications" className="scroll-mt-32">
      <Card>
        <CardHeader>
          <CardTitle>Notifications</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Field label="Notify on new release" hint="A tray notification when an upstream release appears.">
            <Switch
              checked={draft.notifyOnRelease}
              onCheckedChange={(v) => set("notifyOnRelease", v)}
            />
          </Field>
          <Field label="Notify on failure" hint="A tray notification when a check or update fails.">
            <Switch
              checked={draft.notifyOnFailure}
              onCheckedChange={(v) => set("notifyOnFailure", v)}
            />
          </Field>
        </CardContent>
      </Card>
      </section>

      <section id="system" ref={sectionRef("system")} data-testid="settings-section-system" className="scroll-mt-32">
      <Card>
        <CardHeader>
          <CardTitle>System</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Field label="Launch on login" hint="Start RepoSync automatically when you sign in.">
            <Switch
              checked={draft.autostart}
              onCheckedChange={(v) => set("autostart", v)}
            />
          </Field>
          <Field
            label="Close button minimizes to tray"
            hint="When off, the close (X) button quits RepoSync instead of hiding it to the tray."
          >
            <Switch
              checked={draft.closeMinimizesToTray}
              onCheckedChange={(v) => set("closeMinimizesToTray", v)}
            />
          </Field>
          <Field label="Activity retention" hint="Days of activity history to keep.">
            <NumberInput
              value={draft.activityRetentionD}
              min={1}
              onChange={(v) => set("activityRetentionD", v)}
              suffix="days"
            />
          </Field>
          <Field label="Git executable" hint="Leave blank to use the git on your PATH.">
            <TextInput value={draft.gitExecutablePath} onChange={(v) => set("gitExecutablePath", v)} placeholder="auto" />
          </Field>
          <Field label="Editor command" hint="Used by Open in editor.">
            <TextInput value={draft.editorCommand} onChange={(v) => set("editorCommand", v)} placeholder="code" />
          </Field>
          <Field label="Terminal command" hint="Used by Open in terminal.">
            <TextInput value={draft.terminalCommand} onChange={(v) => set("terminalCommand", v)} placeholder="default" />
          </Field>
        </CardContent>
      </Card>
      </section>

      <section id="updates" ref={sectionRef("updates")} data-testid="settings-section-updates" className="scroll-mt-32">
      <UpdatesCard
        autoUpdateCheck={draft.autoUpdateCheck}
        onToggle={(v) => set("autoUpdateCheck", v)}
      />
      </section>

      <section id="integrations" ref={sectionRef("integrations")} data-testid="settings-section-integrations" className="scroll-mt-32">
      <Card>
        <CardHeader>
          <CardTitle>Integrations</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          {/*
            This row used to read "Stored in the OS keychain, never on disk.
            Managed outside this screen." None of that was true. The V1 token
            provider is `github::NoToken`, which always returns `None`;
            `github_token_present` is a derived flag nothing writes; and there
            is no other screen where a token could be managed. So the app was
            describing a keychain it has never written to, on the one surface
            where a user is entitled to know exactly where their credentials
            live.

            The wording now states the situation and the consequence, because
            the consequence is the part that affects someone with a large
            library: unauthenticated GitHub reads are capped at 60 per hour for
            the whole app, not per repo. Keeping that visible is the reason the
            row stays instead of being hidden until BL-V11-02 (keyring PAT).
          */}
          <Field
            label="GitHub token"
            hint="RepoSync reads GitHub without signing in. GitHub allows 60 requests an hour that way, shared across all your repos."
          >
            <span
              className={
                draft.githubTokenPresent
                  ? "font-mono text-xs font-semibold text-status-sync"
                  : "font-mono text-xs text-foreground/70"
              }
            >
              {/*
                The `present` branch cannot render in V1 and is kept for
                BL-V11-02 rather than deleted. "not supported yet" rather than
                "not set", because "not set" invites the user to go and set it
                and there is nowhere to do that.
              */}
              {draft.githubTokenPresent ? "present" : "not supported yet"}
            </span>
          </Field>
        </CardContent>
      </Card>
      </section>

      {/*
        Diagnostics sits below the editable sections and outside the save
        affordance on purpose: nothing on it is a setting, so putting it above
        would suggest the Save button applies to it.
      */}
      <section id="diagnostics" ref={sectionRef("diagnostics")} data-testid="settings-section-diagnostics" className="scroll-mt-32">
      <DiagnosticsCard />
      </section>

      {/*
        About (AC-4, E-21 composite pin 29, register STG4): the running
        version, plus a link-out to the releases page rather than an inline
        history. Last section, matching where "about this app" info usually
        sits.
      */}
      <section id="about" ref={sectionRef("about")} data-testid="settings-section-about" className="scroll-mt-32">
      <AboutCard />
      </section>

      <div className="sticky bottom-0 flex items-center gap-2 border-t border-border bg-background/80 py-3 backdrop-blur">
        <span className="text-xs text-muted-foreground">
          {dirty ? "Unsaved changes" : "All changes saved"}
        </span>
        <div className="ml-auto flex gap-2">
          <Button
            variant="ghost"
            size="sm"
            disabled={!dirty || saving}
            onClick={() => setDraft(initial)}
          >
            <RotateCcw /> Reset
          </Button>
          <Button size="sm" disabled={!dirty || saving} onClick={() => void save()}>
            <Save /> Save changes
          </Button>
        </div>
      </div>
    </div>
  );
}

/**
 * The "Updates" section (E-18 auto-update). Shows the running version, the
 * default-on `auto_update_check` toggle (which gates ONLY the on-launch check; the
 * manual button below always works and nothing ever installs without confirming),
 * and a "Check for updates" button with an inline outcome. No telemetry / account
 * surface - checks-and-install only, matching the no-telemetry OSS posture.
 *
 * The version reads from the shared `useAppVersion` hook (AC-4, E-21
 * composite pin 29) rather than its own `getVersion()` call. This card used
 * to also overwrite its local copy with `UpdateAvailability.currentVersion`
 * after a manual check; that write is gone too - both values name the same
 * running binary from the same Tauri config, so there was never a case
 * where they could legitimately disagree, only a second place a bug could
 * make them.
 */
function UpdatesCard({
  autoUpdateCheck,
  onToggle,
}: {
  autoUpdateCheck: boolean;
  onToggle: (value: boolean) => void;
}) {
  const toast = useToast();
  const version = useAppVersion();
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<UpdateAvailability | null>(null);
  const [installing, setInstalling] = useState(false);

  async function check() {
    setChecking(true);
    setResult(null);
    try {
      // appCheckForUpdate is infallible by design (unreachable is a payload state,
      // not a thrown error), so it resolves to the value directly, not a Result.
      const availability = await commands.appCheckForUpdate();
      setResult(availability);
    } catch (e) {
      toast("error", "Could not check for updates", String(e));
    } finally {
      setChecking(false);
    }
  }

  async function install() {
    setInstalling(true);
    try {
      await unwrap(commands.appInstallUpdate());
      // On success the app relaunches into the new version, so this rarely returns.
    } catch (e) {
      toast(
        "error",
        "Update could not be verified",
        e instanceof IpcError ? e.message : String(e),
      );
      setInstalling(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Updates</CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        <Field label="RepoSync version" hint="The version you are running now.">
          <span className="font-mono text-xs font-semibold text-foreground">{version ?? "unknown"}</span>
        </Field>
        <Field
          label="Check for updates on launch"
          hint="RepoSync looks for a new version when it starts. You always confirm before anything installs - nothing updates silently."
        >
          <Switch checked={autoUpdateCheck} onCheckedChange={onToggle} />
        </Field>
        <div className="flex flex-col gap-3 border-t border-border px-4 py-3">
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="text-sm font-medium">Check for updates</div>
              <div className="text-xs text-muted-foreground">Look for a newer version right now.</div>
            </div>
            <Button variant="secondary" size="sm" disabled={checking} onClick={() => void check()}>
              {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {checking ? "Checking..." : "Check for updates"}
            </Button>
          </div>
          {result && (
            <UpdateOutcome result={result} installing={installing} onInstall={() => void install()} />
          )}
        </div>
      </CardContent>
    </Card>
  );
}

/** The inline result of a manual update check: available / up to date / unreachable. */
function UpdateOutcome({
  result,
  installing,
  onInstall,
}: {
  result: UpdateAvailability;
  installing: boolean;
  onInstall: () => void;
}) {
  // Unreachable (offline, the inert private-repo endpoint, or a not-yet-enabled
  // updater) is reported gently off `error != null`, not as an alarm.
  if (result.error) {
    return (
      <div className="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-foreground">
        Could not reach the update server. RepoSync is still working on your current version; it will
        try again later.
      </div>
    );
  }

  if (result.available && result.newVersion) {
    return (
      <div className="flex flex-col gap-2 rounded-md border border-border bg-muted/40 px-3 py-2">
        <div className="text-sm font-medium text-foreground">
          Version {result.newVersion} is available.
        </div>
        {result.notes && <div className="text-xs text-muted-foreground">{result.notes}</div>}
        <div className="flex items-center gap-2">
          <Button size="sm" disabled={installing} onClick={onInstall}>
            {installing ? <Loader2 className="animate-spin" /> : null}
            {installing ? "Installing..." : "Install and restart"}
          </Button>
          <span className="text-xs text-muted-foreground">
            RepoSync will verify, install, and relaunch.
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-md border border-border bg-muted/40 px-3 py-2 text-sm font-medium text-status-sync">
      You are on the latest version.
    </div>
  );
}


/**
 * The "About" section (AC-4, E-21 composite pin 29, register STG4). Names
 * the running version via the shared `useAppVersion` hook (the same one
 * `app-shell.tsx`'s sidebar and `UpdatesCard` above use - one `getVersion()`
 * call site, not three) and links out to the releases page.
 *
 * Deliberately excludes a past-releases list. No data source for RepoSync's
 * own release history exists anywhere in this codebase - every `commands.*`
 * call that reads release information (`bindings.ts`) takes a REPO id and
 * reads THAT repo's upstream releases; nothing returns RepoSync's own
 * history. Inventing one would be a capability, not a build (AC-4's own
 * text), and AC-12 records the gap rather than leaving it implied.
 *
 * THERE IS ALSO NO LINK OUT, and that is a correction to an earlier version of
 * this comment, which said no `on_navigation` override was "seen" in
 * `src-tauri/src/lib.rs`. One is registered, at `lib.rs:440`, and
 * `allow_navigation` (`lib.rs:272`) returns true only for the `tauri` scheme,
 * the `tauri.localhost` host, and `localhost` under `tauri dev`. An
 * `https://github.com/...` anchor is therefore REFUSED by the webview, so the
 * "View releases" link this section briefly carried rendered, took keyboard
 * focus, and did nothing.
 *
 * A control that cannot perform its action is worse than no control - it is the
 * exact shape `src/lib/ui-reachability.test.ts` exists to catch one level up.
 * Every external open in this app goes through a bespoke backend command
 * instead (`repo_open_remote`, `repo_open_homepage`, `repo_open_folder`,
 * `diagnostics_open_log_dir`), and all of them are repo- or path-scoped: there
 * is no generic "open this URL" command to route a static link through.
 * Adding one is `src-tauri` work and therefore a capability, recorded on AC-12.
 */
function AboutCard() {
  const version = useAppVersion();
  return (
    <Card>
      <CardHeader>
        <CardTitle>About</CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        <Field label="RepoSync version" hint="The version you are running now.">
          <span className="font-mono text-xs font-semibold text-foreground">{version ?? "unknown"}</span>
        </Field>
      </CardContent>
    </Card>
  );
}

// Owns the accessible-name association for whatever control it wraps
// (BL-NI-90). The label text carries a generated id and every descendant
// control reads it through `FieldLabelContext`, so a control placed in a
// `Field` is named without its call site doing anything - see
// `ui/field-label.tsx` for why the association is `aria-labelledby` rather
// than a `<label htmlFor>`.
//
// The hint is deliberately NOT part of the name. It is a sentence of
// explanation, and gluing it to the name would make a screen reader read the
// whole paragraph before saying on or off.
function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  const labelId = useId();
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border px-4 py-3 last:border-b-0">
      <div className="min-w-0">
        <div id={labelId} className="text-sm font-medium">
          {label}
        </div>
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <FieldLabelContext.Provider value={labelId}>{children}</FieldLabelContext.Provider>
      </div>
    </div>
  );
}

function NumberInput({
  value,
  min,
  max,
  suffix,
  onChange,
}: {
  value: number;
  min?: number;
  max?: number;
  suffix?: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <Input
        type="number"
        className="w-20 text-right"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      {suffix && <span className="text-xs text-muted-foreground">{suffix}</span>}
    </div>
  );
}

function TextInput({
  value,
  placeholder,
  onChange,
}: {
  value: string | null;
  placeholder?: string;
  onChange: (value: string | null) => void;
}) {
  return (
    <Input
      className="w-48"
      value={value ?? ""}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
    />
  );
}

/** A native time picker bound to a minute-of-day integer (0..1439). */
// `aria-label` is REQUIRED here, unlike on `Switch` and `Input` where an
// enclosing `Field` supplies the name. Two TimeInputs share one `Field`
// ("Quiet window"), and a `Field` hands the SAME label id to every control
// inside it, so inheriting would name both endpoints identically and leave a
// screen-reader user unable to tell start from end - they could invert their
// own quiet hours. Caught by the Codex adversarial review of this change; the
// automatic naming was right for the twelve single-control fields and wrong
// for the one field with two.
function TimeInput({
  value,
  onChange,
  "aria-label": ariaLabel,
}: {
  value: number;
  onChange: (value: number) => void;
  "aria-label": string;
}) {
  return (
    <Input
      type="time"
      className="w-32"
      aria-label={ariaLabel}
      value={minutesToHhMm(value)}
      onChange={(e) => onChange(hhMmToMinutes(e.target.value))}
    />
  );
}

