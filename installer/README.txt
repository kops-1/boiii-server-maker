Boiii Server Maker
==================

Hosts a Call of Duty: Black Ops III Zombies server for the BOIII client on
your own Windows PC.

What you need
-------------
- Windows 10 or 11, 64-bit.
- Call of Duty: Black Ops III installed through Steam, with Zombies.
- An internet connection for the first setup.

This program contains no game files. It uses the files of your own Black
Ops III install, the free Steam tool "Call of Duty: Black Ops III - Unranked
Dedicated Server" (Steam app 545990) and the BOIII client (boiii.exe), an
unofficial community program that it copies from your game folder or
downloads from the BOIII project's release page.

How to use it
-------------
1. Start "Boiii Server Maker" (Start Menu) or BoiiiServerMaker.exe.
2. On the Host page choose a map (and a mod if you want one).
3. Press "Start server". The first time this also sets everything up; the
   header then shows Starting and, after up to a minute, Running.
   If it says the server files are missing, the Setup page opens: press
   "Install through Steam" (easiest), wait for Steam to finish, and press
   Start server again. Without the Steam client use "Download with
   steamcmd": type your Steam login name in the box first; the password is
   typed in steamcmd's own window, never in this program. With the Steam
   mobile app, approve the sign-in on your phone straight away; if it times
   out, choose "Try again".
4. Press "Play" to open the game on this PC. It starts BOIII on a port of
   its own, so it runs next to the server. When the game shows its main
   menu, press the ~ key, paste with Ctrl+V and press Enter: the command
   that joins your server is already on the clipboard. (The game cannot be
   told to join by itself; the paste is needed.)
5. Press "Copy invite" and send it to your friends. They paste it into the
   game console (the ~ key).
6. Press the star to put your server into the game's Favorites list on this
   PC (server browser > Favorites). Close the game first: it reads the list
   when it starts. Press the star again if you change the port.
7. "Stop server" stops it. Closing the window does not stop the server.

The pages
---------
Host      Map, mod, server name, password, players, port, the map rotation
          and presets. A preset remembers the map, the mod, the rotation
          and the cheat switches. The server name is shown at the top.
Cheats    The in-game mod menu and the switches that apply to every player.
Friends   The invite, the other addresses of this PC, a tunnel address
          (Playit) for invites, and "Check network", which looks at the
          firewall, your public address and the BOIII server list. It
          changes nothing.
Setup     The checklist, Run setup, Repair files, getting the server files,
          and the folders.
Scores    Best games: the round reached and each player's kills, headshots,
          points and downs, recorded when a game ends.
Log       What the program did.

If the window does not open or looks broken (very old graphics driver),
start BoiiiServerMakerClassic.exe in the program folder: the same functions
with plain Windows controls.

Shortcut: "BoiiiServerMaker.exe --do start" opens the window and starts the
server straight away.
Maps and mods
-------------
"Map" lists every Zombies map this PC can host: the official maps you own
and the Steam Workshop maps you are subscribed to (marked "workshop").
"Mod" lists your Workshop mods; "No mod" is the normal game. To get more,
subscribe in the Steam Workshop, let Steam finish downloading, and press
"Check again". Workshop content is linked into the server folder, never
copied. Friends need the same Workshop map or mod; BOIII offers to download
it when they join.

Saving a game
-------------
In the game, open the mod menu > Save / Load and save to slot 1, 2 or 3.
Or switch on "Autosave every round" on the Host page (slot "auto").
A save keeps the round, the power, and each player's points, weapons
(with ammo, Pack-a-Punch and attachments) and perks. Doors, buildables,
gobblegums and easter-egg progress are not saved.

To continue later, even after closing the server: on the Host page, under
"Saved games", press Continue next to the save, then start the server.
It loads as soon as the first player is in. Players get their things back
by name, so everyone should use the same name as when the game was saved.
You can also load a slot at any time from the mod menu (same map only).

Map rotation
------------
On the Host page, add maps to the rotation to play them one after another.
When a game ends the next map loads; after the last it starts at the top.

Official maps only: the game changes the map itself.
With a workshop map in the rotation: this program changes the map, by
restarting the server on the next one a few seconds after the game ends
(the game itself crashes when it leaves a workshop map). Keep the window
open. Players are dropped at the change and rejoin with the same invite.
From a terminal, "bsm next" does the same change by hand.

Mod menu
--------
Switch on "Mod menu" on the Cheats page and start (or restart) the server.
In the game:

    open / close   hold AIM and press MELEE
    down           FIRE
    up             AIM
    select         USE
    back           MELEE

It also works with the D-pad of a controller, or keys 1 and 2 (up, down),
4 (select) and 3 (back). To use the arrow keys, press "Copy arrow-key
command" on the Cheats page and paste it once into the game console (~).
Every player who wants the arrow keys does this once; the game remembers.

The menu has:
  Player     god mode, infinite ammo, zombies ignore me, auto revive,
             invisible, run speed, unlimited sprint, no weapon spread, fly,
             third person, teleport, save and return to a position, revive
  Points     add, take, set to 0, double points for all
  Weapons    max ammo, Pack-a-Punch one or all, give any weapon of the map,
             drop, reset to the starting weapon
  Perks      all, none, one, keep perks when down
  Power-ups  drop any power-up of the map
  Zombies    kill all, insta-kill, freeze, speed, bring to crosshair,
             stop spawning
  Round      next, skip, go to a round
  Map        power on, open all doors, low gravity, end game, restart
  Everyone   god mode, ammo, points, perks, Pack-a-Punch, revive, teleport,
             freeze the other players
  Save/Load  save to slot 1-3, load a slot or the autosave, autosave on/off
  Players    for one player: points, perks, ammo, revive, god mode, freeze,
             Pack-a-Punch, bring, go to, kick
Fly: switch it on, then hold SPRINT to move where you look. Fly and
"Teleport to crosshair" can take you outside the map; the game normally
kills players there, so both switch that off for you until you respawn.

"Only for" limits the menu to the names you list (comma separated).
Leave it empty and every player on the server can open it.

The switches under "For every player, from the start" work without the
menu: god mode, infinite ammo, all perks, insta-kill, double points, keep
perks, and starting points.

With everything switched off the server runs a normal game and no extra script
is placed in the server folder.

If the server stays at "loading the map"
----------------------------------------
A power cut or a full disk during setup can leave game files in the server
folder with the right size but damaged content. Stop the server and press
"Repair files" (command line: bsm setup --repair). It compares every game
file with the Steam install and the game and replaces the damaged ones.
Your own settings, logs and player data are left alone.

Playing on the same PC: the server uses UDP port 27017 by default and the
game uses another one, so both can run together. Change "UDP port" if 27017
is taken.

Command line
------------
bsm.exe does the same from a terminal:

    bsm status
    bsm maps
    bsm mods
    bsm setup --map zm_zod --port 27017
    bsm setup --map zm_town --mod none --menu on --start-points 5000
    bsm start --wait 180
    bsm play
    bsm stop
    bsm join
    bsm help

Where things are kept
---------------------
Settings, logs and the server folder:  %LOCALAPPDATA%\BoiiiServerMaker
Your own extra server settings:        <server folder>\zone\servermaker_custom.cfg

Portable use: create an empty file named portable.txt next to
BoiiiServerMaker.exe. Everything is then kept in a "data" folder beside the
program and the whole folder can be moved to another drive or PC.

The program folder itself can be moved or copied anywhere at any time.
Nothing in it depends on where it is.

Removing it
-----------
Windows Settings > Apps > Boiii Server Maker, or run uninstall.exe.
The server folder and settings are kept; delete
%LOCALAPPDATA%\BoiiiServerMaker yourself if you do not want them.

Not affiliated with Activision, Treyarch, Valve or the BOIII project.
