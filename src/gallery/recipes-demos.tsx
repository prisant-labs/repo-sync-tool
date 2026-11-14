import { useState } from "react";
import { Cell } from "@/gallery/cells";
import { OptionCard, RadioDot } from "@/components/recipes/option-card";
import { GroupChip, GroupDot } from "@/components/recipes/group-chip";
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
        <div className="flex items-center gap-6 text-body">
          <span className="flex items-center gap-2"><RadioDot active={false} /> inactive</span>
          <span className="flex items-center gap-2"><RadioDot active /> active</span>
        </div>
      </Cell>
    </>
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
