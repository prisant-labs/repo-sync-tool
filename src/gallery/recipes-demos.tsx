import { useState } from "react";
import { Cell } from "@/gallery/cells";
import { OptionCard, RadioDot } from "@/components/recipes/option-card";

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
