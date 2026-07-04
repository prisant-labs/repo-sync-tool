import { useMemo, useState } from "react";
import type { ReactNode } from "react";
import { RotateCcw, Save } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { Settings } from "@/lib/bindings";
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
