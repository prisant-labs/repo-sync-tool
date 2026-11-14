import type { ReactNode } from "react";
import {
  ButtonsSection,
  CardsSection,
  FormSection,
  TabsSection,
  DataTableSection,
  DrawerCell,
  DialogCell,
  ToastCell,
  StatusSection,
  FilterChipSection,
  EmptyStatesSection,
  AsyncPanelSection,
  PageShellSection,
  GroupsNavSection,
  GroupDialogCells,
  AddReposCell,
  ReceiptCell,
  DiagnosticsSection,
  StatusOverviewSection,
  ScreenHost,
  DrawerScreen,
  ShellSection,
} from "@/gallery/demos";

import { OptionCardSection, GroupChipSection } from "@/gallery/recipes-demos";

export const GROUP_ORDER = ["Primitives", "Product components", "Recipes", "Screens"] as const;
type GroupName = (typeof GROUP_ORDER)[number];

export interface Section {
  id: string;
  group: GroupName;
  title: string;
  note?: string;
  render: () => ReactNode;
}

export const SECTIONS: Section[] = [
  { id: "buttons", group: "Primitives", title: "Button", render: () => <ButtonsSection /> },
  { id: "cards", group: "Primitives", title: "Card", render: () => <CardsSection /> },
  { id: "form", group: "Primitives", title: "Input, Switch, FieldLabel", render: () => <FormSection /> },
  { id: "tabs", group: "Primitives", title: "Tabs", render: () => <TabsSection /> },
  { id: "data-table", group: "Primitives", title: "DataTable, ColumnsPopover", render: () => <DataTableSection /> },
  { id: "drawer", group: "Primitives", title: "Drawer", note: "Mounted open, as the app mounts it after a row click.", render: () => <DrawerCell /> },
  { id: "dialog", group: "Primitives", title: "Dialog", render: () => <DialogCell /> },
  { id: "toast", group: "Primitives", title: "Toast", render: () => <ToastCell /> },

  { id: "status", group: "Product components", title: "Status, lag, signals", render: () => <><StatusSection /><StatusOverviewSection /></> },
  { id: "chips", group: "Product components", title: "FilterChip", render: () => <FilterChipSection /> },
  { id: "empty", group: "Product components", title: "EmptyState, AllClear", render: () => <EmptyStatesSection /> },
  { id: "async", group: "Product components", title: "AsyncPanel", render: () => <AsyncPanelSection /> },
  { id: "page-shell", group: "Product components", title: "PageShell", render: () => <PageShellSection /> },
  { id: "groups-nav", group: "Product components", title: "GroupsNav", render: () => <GroupsNavSection /> },
  { id: "group-dialog", group: "Product components", title: "GroupDialog", render: () => <GroupDialogCells /> },
  { id: "add-repos", group: "Product components", title: "AddReposDialog", render: () => <AddReposCell /> },
  { id: "receipt", group: "Product components", title: "ActivityReceipt", render: () => <ReceiptCell /> },
  { id: "diagnostics", group: "Product components", title: "DiagnosticsCard", render: () => <DiagnosticsSection /> },

  { id: "recipes-option-card", group: "Recipes", title: "OptionCard, RadioDot", render: () => <OptionCardSection /> },
  { id: "recipes-group-chip", group: "Recipes", title: "GroupChip, GroupDot", render: () => <GroupChipSection /> },

  { id: "screen-dashboard", group: "Screens", title: "Dashboard", render: () => <ScreenHost screen="dashboard" /> },
  { id: "screen-repos", group: "Screens", title: "Repos", render: () => <ScreenHost screen="repos" /> },
  { id: "screen-activity", group: "Screens", title: "Activity", render: () => <ScreenHost screen="activity" /> },
  { id: "screen-settings", group: "Screens", title: "Settings", render: () => <ScreenHost screen="settings" /> },
  { id: "screen-drawer", group: "Screens", title: "Repository detail drawer", render: () => <DrawerScreen /> },
  { id: "screen-shell", group: "Screens", title: "App shell (whole app)", render: () => <ShellSection /> },
];

