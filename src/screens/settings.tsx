import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Loader2, RefreshCw, RotateCcw, Save } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { Settings, UpdateAvailability } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { AsyncPanel } from "@/components/async-panel";
import { useSettings } from "@/hooks/queries";
import { useToast } from "@/hooks/use-toast";

export function SettingsScreen() {
  const settings = useSettings();

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Settings</h2>
        <p className="text-sm text-muted-foreground">How RepoSync checks, notifies, and integrates.</p>
      </div>

      <AsyncPanel state={settings}>
        {(s) => <SettingsForm initial={s} onSaved={settings.refetch} />}
      </AsyncPanel>
    </div>
  );
}

function SettingsForm({ initial, onSaved }: { initial: Settings; onSaved: () => void }) {
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
          <Field label="Quiet hours" hint="Pause scheduled checks during a daily window (local clock).">
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
              <TimeInput value={draft.quietHoursStart ?? 0} onChange={(v) => set("quietHoursStart", v)} />
              <span className="text-xs text-muted-foreground">to</span>
              <TimeInput value={draft.quietHoursEnd ?? 0} onChange={(v) => set("quietHoursEnd", v)} />
            </Field>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Notifications</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Field label="Notify on new release" hint="A tray notification when an upstream release appears.">
            <Switch checked={draft.notifyOnRelease} onCheckedChange={(v) => set("notifyOnRelease", v)} />
          </Field>
          <Field label="Notify on failure" hint="A tray notification when a check or update fails.">
            <Switch checked={draft.notifyOnFailure} onCheckedChange={(v) => set("notifyOnFailure", v)} />
          </Field>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>System</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Field label="Launch on login" hint="Start RepoSync automatically when you sign in.">
            <Switch checked={draft.autostart} onCheckedChange={(v) => set("autostart", v)} />
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

      <UpdatesCard
        autoUpdateCheck={draft.autoUpdateCheck}
        onToggle={(v) => set("autoUpdateCheck", v)}
      />

      <Card>
        <CardHeader>
          <CardTitle>Integrations</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Field label="GitHub token" hint="Stored in the OS keychain, never on disk. Managed outside this screen.">
            <span
              className={
                draft.githubTokenPresent
                  ? "font-mono text-xs font-semibold text-status-sync"
                  : "font-mono text-xs text-muted-foreground"
              }
            >
              {draft.githubTokenPresent ? "present" : "not set"}
            </span>
          </Field>
        </CardContent>
      </Card>

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
 */
function UpdatesCard({
  autoUpdateCheck,
  onToggle,
}: {
  autoUpdateCheck: boolean;
  onToggle: (value: boolean) => void;
}) {
  const toast = useToast();
  const [version, setVersion] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<UpdateAvailability | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    let active = true;
    void getVersion()
      .then((v) => active && setVersion(v))
      .catch(() => active && setVersion(null));
    return () => {
      active = false;
    };
  }, []);

  async function check() {
    setChecking(true);
    setResult(null);
    try {
      // appCheckForUpdate is infallible by design (unreachable is a payload state,
      // not a thrown error), so it resolves to the value directly, not a Result.
      const availability = await commands.appCheckForUpdate();
      setResult(availability);
      if (availability.currentVersion) setVersion(availability.currentVersion);
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

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border px-4 py-3 last:border-b-0">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
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
function TimeInput({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  return (
    <Input
      type="time"
      className="w-32"
      value={minutesToHhMm(value)}
      onChange={(e) => onChange(hhMmToMinutes(e.target.value))}
    />
  );
}

function minutesToHhMm(minutes: number): string {
  const wrapped = ((Math.trunc(minutes) % 1440) + 1440) % 1440;
  const hh = Math.floor(wrapped / 60);
  const mm = wrapped % 60;
  return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

function hhMmToMinutes(value: string): number {
  const [h, m] = value.split(":");
  const hours = Number(h);
  const mins = Number(m);
  if (Number.isNaN(hours) || Number.isNaN(mins)) return 0;
  return hours * 60 + mins;
}
