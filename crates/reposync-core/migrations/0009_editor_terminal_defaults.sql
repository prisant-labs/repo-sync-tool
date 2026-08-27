-- Seed the editor and terminal commands that 0002 declared with no default.
--
-- Both columns have been NULL on every install since 0002, and `repo_open_editor`
-- / `repo_open_terminal` return `InvalidSetting` when they are, so "Open in ->
-- Editor" and "-> Terminal" have never worked out of the box. The Settings fields
-- show placeholder text ("code", "default") that reads like a configured value,
-- so nothing on screen explained why the buttons failed.
--
-- `code` resolves through PATH and PATHEXT (so VS Code's `code.cmd` shim is
-- found), and `wt` is Windows Terminal, which `open_terminal` already special
-- cases with `-d`. Neither is guaranteed to exist on a given machine; when one
-- does not, the opener now reports which command was not found rather than
-- claiming the setting is invalid, and the value is a normal editable setting.
--
-- Only NULL and blank values are touched, so a deliberate choice is never
-- overwritten. Fresh installs get the same values from the seeding INSERT in
-- `store::settings_get`, because the singleton row does not exist yet when this
-- migration runs.
UPDATE settings SET editor_command = 'code'
  WHERE editor_command IS NULL OR trim(editor_command) = '';

UPDATE settings SET terminal_command = 'wt'
  WHERE terminal_command IS NULL OR trim(terminal_command) = '';
