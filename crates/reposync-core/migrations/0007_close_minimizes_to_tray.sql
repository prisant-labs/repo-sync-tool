-- 0007_close_minimizes_to_tray.sql - the close-button behavior toggle.
--
-- Adds one additive column to the settings singleton: close_minimizes_to_tray,
-- the toggle that decides whether the window's close (X) button HIDES the app to
-- the tray (default) or QUITS it. It defaults to 1 (minimize to tray), which
-- preserves the prior hardcoded behavior for every existing install: RepoSync is
-- a resident tray utility, so close-to-tray is the sensible default; a user who
-- prefers close-to-quit turns it off.
--
-- The column is NOT NULL with a DEFAULT, so the existing settings row (and any
-- future INSERT that omits it) backfills to 1. settings is a singleton with no
-- inbound foreign keys, so a plain ALTER TABLE ADD COLUMN is safe and needs no
-- table rebuild.
--
-- Migration discipline (see migrations/README.md): additive-only. 0001-0006 are
-- FROZEN; this is the only new file.

ALTER TABLE settings ADD COLUMN close_minimizes_to_tray INTEGER NOT NULL DEFAULT 1;
