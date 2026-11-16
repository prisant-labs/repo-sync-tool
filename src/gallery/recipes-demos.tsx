import { useState } from "react";
import { Cell } from "@/gallery/cells";
import { OptionCard, RadioDot } from "@/components/recipes/option-card";
import { GroupChip, GroupDot } from "@/components/recipes/group-chip";
import { SettingsField, NumberInput, TextInput, TimeInput } from "@/components/recipes/settings-fields";
import { Switch } from "@/components/ui/switch";
import { GROUPS } from "@/gallery/fixtures";
import type { GroupSummary } from "@/lib/bindings";

// ------------------------------------------------------------------- Recipes
//
// One section per shared recipe in `src/components/recipes/`, each showing
// every variant the recipe has.

export function OptionCardSection() {
  const [choice, setChoice] = useState("ff");
  const choices = [
    { id: "check", label: "Check only", description: "Detect changes, never fetch or pull." },
    { id: "fetch", label: "Fetch only", description: "Download objects, leave the working tree untouched." },
    { id: "ff", label: "Fast-forward", description: "Pull when the update is a clean fast-forward; otherwise leave it and say why." },
  ];
  return (
    <>
      <Cell title="OptionCard, a group of choices" note="click an unchosen card; the chosen card is not clickable">
        <div className="flex max-w-xl flex-col gap-1.5">
          {choices.map((c) => (
            <OptionCard
              key={c.id}
              label={c.label}
              description={c.description}
              active={choice === c.id}
              onSelect={() => setChoice(c.id)}
            />
          ))}
        </div>
      </Cell>
      <Cell title="OptionCard, disabled and dimmed" note="disabled: a busy panel. dimmed: a choice that is not offered yet">
        <div className="flex max-w-xl flex-col gap-1.5">
          <OptionCard label="Disabled, unchosen" description="The caller reports it disabled." active={false} disabled onSelect={() => {}} />
          <OptionCard label="Dimmed and disabled" description="Not offered yet." active={false} disabled dimmed onSelect={() => {}} />
        </div>
      </Cell>
      <Cell title="RadioDot" note="the dot alone, as the Custom interval card uses it">
        <div className="flex items-center gap-4 text-body">
          <span className="flex items-center gap-2"><RadioDot active={false} /> inactive</span>
          <span className="flex items-center gap-2"><RadioDot active /> active</span>
        </div>
      </Cell>
    </>
  );
}

export function SettingsFieldsSection() {
  const [n, setN] = useState(15);
  const [text, setText] = useState<string | null>(null);
  const [start, setStart] = useState(22 * 60);
  const [end, setEnd] = useState(7 * 60);
  const [on, setOn] = useState(true);
  return (
    <Cell title="SettingsField with each input" note="the rows the Settings screen is built from">
      <div className="max-w-3xl rounded-md border border-border">
        <SettingsField label="Switch" hint="A field with a hint, holding a switch.">
          <Switch checked={on} onCheckedChange={setOn} />
        </SettingsField>
        <SettingsField label="NumberInput" hint="With a suffix after the box.">
          <NumberInput value={n} min={1} max={999} suffix="min" onChange={setN} />
        </SettingsField>
        <SettingsField label="NumberInput, no suffix">
          <NumberInput value={n} min={1} onChange={setN} />
        </SettingsField>
        <SettingsField label="TextInput" hint="Empty means not set (null).">
          <TextInput value={text} placeholder="not set" onChange={setText} />
        </SettingsField>
        <SettingsField label="Quiet window" hint="Two TimeInputs in one field, each with its own aria-label.">
          <TimeInput value={start} aria-label="Quiet hours start" onChange={setStart} />
          <span className="text-cell text-muted-foreground">to</span>
          <TimeInput value={end} aria-label="Quiet hours end" onChange={setEnd} />
        </SettingsField>
      </div>
    </Cell>
  );
}

const NO_COLOUR_GROUP: GroupSummary = { id: 99, name: "uncoloured", color: null, repoCount: 0 };

export function GroupChipSection() {
  const groups = [...GROUPS, NO_COLOUR_GROUP];
  return (
    <>
      <Cell title="GroupChip, removable" note="the repository detail panel's header">
        <div className="flex flex-wrap items-center gap-1.5">
          {groups.map((g) => (
            <GroupChip key={g.id} variant="removable" group={g} busy={false} onRemove={() => {}} />
          ))}
          <GroupChip variant="removable" group={GROUPS[0]} busy onRemove={() => {}} />
          <span className="text-cell text-muted-foreground">last one: busy, so its X is disabled</span>
        </div>
      </Cell>
      <Cell title="GroupChip, compact" note="the Repos table's Groups column">
        <div className="flex flex-wrap items-center gap-1">
          {groups.map((g) => (
            <GroupChip key={g.id} variant="compact" group={g} />
          ))}
        </div>
      </Cell>
      <Cell title="GroupDot" note="regular, compact, and with no colour">
        <div className="flex items-center gap-4 text-body">
          <span className="flex items-center gap-2"><GroupDot color={GROUPS[0].color} /> regular</span>
          <span className="flex items-center gap-2"><GroupDot color={GROUPS[0].color} compact /> compact</span>
          <span className="flex items-center gap-2"><GroupDot color={null} /> no colour</span>
        </div>
      </Cell>
    </>
  );
}
