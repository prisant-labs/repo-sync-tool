-- 0003_cadence_inherit.sql - migrate existing repos to the INHERIT cadence model
-- (backlog BL-NI-20: the global cadence control had no effect).
--
-- The scheduler now treats repos.check_frequency_min = 0 as "inherit the global
-- cadence" (settings.global_check_minutes), and a positive value as an explicit
-- per-repo override. Newly-added repos are inserted with check_frequency_min = 0
-- by repo::add; this migration brings EXISTING rows onto the same model so the
-- global control takes effect for them too.
--
-- This is safe: V1 ships no per-repo cadence UI, so every existing value is the
-- old column default (360), never a user-chosen override. Rewriting them all to
-- 0 loses no user intent.
--
-- Same migration discipline as 0001/0002 (see migrations/README.md): additive,
-- data-only, non-destructive (no dropped/renamed columns, no lost rows).

UPDATE repos SET check_frequency_min = 0;
