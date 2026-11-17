# Testing notes

Historical validation recorded before the repository cleanup. These results were reported on one Windows 11 PC; they are not a cross-machine compatibility guarantee or a fresh test run.

**Tested**

- 63 unit tests.
- Install to a clean folder, setup, start, status, stop and uninstall from the installed copy; a portable copy on another drive running a second server at the same time.
- Starting an official map, a workshop map, and a workshop map together with a mod.
- The mod menu, run on a bot inside the server on three setups (official map, workshop map, map with a mod): every menu opened, every action ran without a script error, every weapon of the map was given, every upgradable weapon was Pack-a-Punched, every power-up was dropped, perks and points changed as expected.
- A map rotation of an official and a workshop map, through two map changes.
- The scoreboard: a finished game was written by the server and shown in the app.
- Repair: after a real power loss it replaced exactly the damaged files.
- Save and continue: points, all perks, the power and two weapons (one Pack-a-Punched with attachments) with their exact ammo were saved, wiped, and restored by Load; and restored again after a full server restart, both by Load from the menu and by Continue on start.
- The game launch next to a running server.
- The fallback to the classic window when the graphics driver was in an error state.

**Not tested**

- A second PC.
- The mod menu on a real player's screen: how it looks, the button handling, the arrow keys, and effects that only show in play. Round skipping could not be observed, because the game does not start rounds with only a bot.
- Joining through the pasted command, friends joining from outside, and friends being offered a workshop download.
- The steamcmd download (it needs the account owner to sign in) and the automatic download of `boiii.exe` (a copy was always present).
- A rotation of official maps only.
- Save and continue with real players: matching by name was only exercised with test bots, whose names change on every join. Restoring a round number above 1 and autosave at a new round could not be observed, because rounds do not start with only a bot.
- Gobblegums, shields and other equipment are not saved.
- Other mods and other workshop maps than the ones tried.
- Clicking through the main window by hand; its pages were inspected in screen captures and its actions run through `--do`.


