// Boiii Server Maker - in-game mod menu and start options for BOIII Zombies servers.
//
// Switched on by dvars that Boiii Server Maker writes into the server config:
//   bsm_menu          1 = players can open the mod menu
//   bsm_menu_players  lower-case player names separated by commas; empty = everyone
//   bsm_god           1 = every player is invulnerable
//   bsm_ammo          1 = every player has infinite ammo
//   bsm_start_points  points every player has at least when they first spawn
//   bsm_start_perks   1 = every player starts with all perks of the map
//   bsm_instakill     1 = zombies die in one hit, all game
//   bsm_double_points 1 = double points, all game
//   bsm_keep_perks    1 = players keep their perks when they go down
//   bsm_scoreboard    1 = write each finished game to the game log for the scoreboard
//   bsm_autosave      1 = save the game at the start of every round (slot "auto")
//   bsm_autoload      slot to continue from when the game starts; empty = none
//
// Menu controls:
//   open / close   hold AIM and press MELEE
//   down           FIRE
//   up             AIM
//   select         USE
//   back           MELEE
// The four action-slot buttons work as well: D-pad on a controller, keys
// 1 2 3 4 on a keyboard (or the arrow keys once bound to +actionslot):
//   up / down      action slot 1 / 2
//   select / back  action slot 4 / 3
#using scripts\shared\flag_shared;
#using scripts\zm\_zm;
#using scripts\zm\_zm_score;
#using scripts\zm\_zm_perks;
#using scripts\zm\_zm_weapons;
#using scripts\zm\_zm_powerups;
#using scripts\zm\_zm_laststand;
#using scripts\zm\_zm_power;
#using scripts\shared\ai\zombie_utility;

autoexec function main()
{
    if (isdefined(level.bsm_running))
    {
        return;
    }
    level.bsm_running = true;

    // Lets the host tool confirm that this script was compiled and started.
    SetDvar("bsm_loaded", "1");

    level.bsm_menu = GetDvarInt("bsm_menu") != 0;
    level.bsm_god_all = GetDvarInt("bsm_god") != 0;
    level.bsm_ammo_all = GetDvarInt("bsm_ammo") != 0;
    level.bsm_instakill = GetDvarInt("bsm_instakill") != 0;
    level.bsm_double = GetDvarInt("bsm_double_points") != 0;
    level.bsm_keep_perks = GetDvarInt("bsm_keep_perks") != 0;
    level.bsm_freeze = false;
    level.bsm_zspeed = 0;
    level.bsm_gravity = false;
    level.bsm_freeze_players = false;
    level.bsm_lines = 10;

    level.bsm_game_id = RandomInt(2000000000);

    level thread bsm_on_connect();
    level thread bsm_level_keep();
    if (GetDvarInt("bsm_scoreboard") != 0)
    {
        level thread bsm_scoreboard();
    }
    level.bsm_autosave = GetDvarInt("bsm_autosave") != 0;
    level thread bsm_autosave_watch();
    if (GetDvarString("bsm_autoload") != "")
    {
        level thread bsm_autoload(GetDvarString("bsm_autoload"));
    }
}

// When the game ends, writes one line for the game and one per player into
// the game log. Boiii Server Maker reads them for its scoreboard.
bsm_scoreboard()
{
    level waittill("end_game");
    players = GetPlayers();
    LogPrint("BSM_GAME;" + level.bsm_game_id + ";" + level.script + ";" + level.round_number + ";" + players.size + "\n");
    foreach (player in players)
    {
        LogPrint("BSM_PLAYER;" + level.bsm_game_id + ";" + player.name + ";" + bsm_number(player.kills) + ";" + bsm_number(player.score) + ";" + bsm_number(player.downs) + ";" + bsm_number(player.headshots) + "\n");
    }
}

bsm_number(value)
{
    if (!isdefined(value))
    {
        return 0;
    }
    return value;
}

// Re-applies the game-wide switches once a second, so they also cover
// zombies that spawn later and survive the game's own power-up timers.
bsm_level_keep()
{
    level endon("end_game");
    insta_was = false;
    double_was = false;
    for (;;)
    {
        wait 1;
        players = GetPlayers();
        foreach (player in players)
        {
            if (isdefined(level.zombie_vars) && isdefined(level.zombie_vars[player.team]))
            {
                if (level.bsm_instakill)
                {
                    level.zombie_vars[player.team]["zombie_insta_kill"] = 1;
                }
                else if (insta_was)
                {
                    level.zombie_vars[player.team]["zombie_insta_kill"] = 0;
                }
                if (level.bsm_double)
                {
                    level.zombie_vars[player.team]["zombie_point_scalar"] = 2;
                }
                else if (double_was)
                {
                    level.zombie_vars[player.team]["zombie_point_scalar"] = 1;
                }
            }
            if (level.bsm_keep_perks)
            {
                player._retain_perks = true;
            }
        }
        insta_was = level.bsm_instakill;
        double_was = level.bsm_double;

        if (level.bsm_freeze || level.bsm_zspeed != 0)
        {
            zombies = GetAITeamArray(level.zombie_team);
            foreach (zombie in zombies)
            {
                if (!isdefined(zombie) || !IsAlive(zombie))
                {
                    continue;
                }
                if (level.bsm_freeze && !isdefined(zombie.bsm_frozen))
                {
                    zombie.bsm_frozen = true;
                    zombie PathMode("dont move");
                    zombie.ignoreall = true;
                }
                if (level.bsm_zspeed != 0 && (!isdefined(zombie.bsm_speed) || zombie.bsm_speed != level.bsm_zspeed))
                {
                    zombie.bsm_speed = level.bsm_zspeed;
                    zombie zombie_utility::set_zombie_run_cycle(bsm_zspeed_name(level.bsm_zspeed));
                }
            }
        }
    }
}

bsm_zspeed_name(index)
{
    switch (index)
    {
        case 1: return "walk";
        case 2: return "run";
        case 3: return "sprint";
    }
    return "normal";
}

bsm_on_connect()
{
    level endon("end_game");
    for (;;)
    {
        level waittill("connected", player);
        player thread bsm_player();
    }
}

bsm_player()
{
    self endon("disconnect");
    self waittill("spawned_player");
    wait 0.5;

    if (isdefined(self.bsm_started))
    {
        return;
    }
    self.bsm_started = true;
    self.bsm_god = false;
    self.bsm_god_applied = false;
    self.bsm_ammo = false;
    self.bsm_ignore = false;
    self.bsm_speed = 0;
    self.bsm_third = false;
    self.bsm_noclip = false;
    self.bsm_sprint = false;
    self.bsm_invisible = false;
    self.bsm_nospread = false;
    self.bsm_autorevive = false;
    self.bsm_frozen = false;
    self.bsm_open = false;

    self thread bsm_keep_applied();
    self bsm_start_options();
    // A loaded save may hold this player's points, weapons and perks.
    self thread bsm_apply_pending();

    if (level.bsm_menu && self bsm_allowed())
    {
        self thread bsm_menu_think();
        self iprintlnbold("Mod menu: hold AIM and press MELEE");
    }
}

bsm_allowed()
{
    list = ToLower(GetDvarString("bsm_menu_players"));
    if (list == "")
    {
        return true;
    }
    me = ToLower(self.name);
    names = StrTok(list, ",");
    foreach (name in names)
    {
        if (name == me)
        {
            return true;
        }
    }
    return false;
}

bsm_start_options()
{
    points = GetDvarInt("bsm_start_points");
    if (points > 0 && self.score < points)
    {
        self zm_score::add_to_player_score(points - self.score);
    }
    if (GetDvarInt("bsm_start_perks") != 0)
    {
        self bsm_give_all_perks();
    }
}

// Re-applies switched-on options so they survive revives, respawns and weapon changes.
bsm_keep_applied()
{
    self endon("disconnect");
    ticks = 0;
    for (;;)
    {
        wait 0.2;
        ticks++;

        if (self.bsm_ammo || level.bsm_ammo_all)
        {
            weapon = self GetCurrentWeapon();
            if (isdefined(weapon) && weapon != level.weaponNone)
            {
                self SetWeaponAmmoClip(weapon, weapon.clipSize);
                self GiveMaxAmmo(weapon);
            }
            // Grenades and the other carried weapons once a second.
            if (ticks % 5 == 0)
            {
                self bsm_max_ammo();
            }
        }

        if (ticks % 5 == 0)
        {
            if (self.bsm_god || level.bsm_god_all)
            {
                self EnableInvulnerability();
                self.bsm_god_applied = true;
            }
            else if (self.bsm_god_applied)
            {
                self DisableInvulnerability();
                self.bsm_god_applied = false;
            }
            if (self.bsm_ignore)
            {
                self.ignoreme = true;
            }
            if (self.bsm_speed > 0)
            {
                self SetMoveSpeedScale(bsm_speed_scale(self.bsm_speed));
            }
            if (self.bsm_autorevive && isdefined(self.laststand) && self.laststand)
            {
                self zm_laststand::auto_revive(self);
            }
        }
    }
}

bsm_speed_scale(index)
{
    switch (index)
    {
        case 1: return 1.5;
        case 2: return 2;
        case 3: return 3;
    }
    return 1;
}

bsm_speed_label(index)
{
    switch (index)
    {
        case 1: return "x1.5";
        case 2: return "x2";
        case 3: return "x3";
    }
    return "normal";
}

// ---------------------------------------------------------------- menu input

bsm_menu_think()
{
    self endon("disconnect");
    for (;;)
    {
        wait 0.05;

        if (!self.bsm_open)
        {
            if (self AdsButtonPressed() && self MeleeButtonPressed())
            {
                self bsm_menu_open();
                self bsm_wait_release();
            }
            continue;
        }

        // Action slots: D-pad, keys 1-4, or arrow keys bound to them.
        if (self ActionSlotOneButtonPressed())
        {
            self bsm_menu_move(-1);
            wait 0.15;
            continue;
        }
        if (self ActionSlotTwoButtonPressed())
        {
            self bsm_menu_move(1);
            wait 0.15;
            continue;
        }
        if (self ActionSlotFourButtonPressed())
        {
            self bsm_menu_select();
            while (self ActionSlotFourButtonPressed())
            {
                wait 0.05;
            }
            continue;
        }
        if (self ActionSlotThreeButtonPressed())
        {
            self bsm_menu_back();
            while (self ActionSlotThreeButtonPressed())
            {
                wait 0.05;
            }
            continue;
        }

        if (self MeleeButtonPressed())
        {
            if (self AdsButtonPressed())
            {
                self bsm_menu_close();
            }
            else
            {
                self bsm_menu_back();
            }
            self bsm_wait_release();
        }
        else if (self UseButtonPressed())
        {
            self bsm_menu_select();
            while (self UseButtonPressed())
            {
                wait 0.05;
            }
        }
        else if (self AttackButtonPressed())
        {
            self bsm_menu_move(1);
            wait 0.13;
        }
        else if (self AdsButtonPressed())
        {
            self bsm_menu_move(-1);
            wait 0.13;
        }
    }
}

bsm_wait_release()
{
    while (self MeleeButtonPressed() || self AdsButtonPressed())
    {
        wait 0.05;
    }
}

bsm_menu_open()
{
    self.bsm_open = true;
    self.bsm_stack_menu = [];
    self.bsm_stack_cursor = [];
    self.bsm_menu_id = "main";
    self.bsm_cursor = 0;
    self.bsm_scroll = 0;
    self bsm_hud_create();
    self bsm_build(self.bsm_menu_id);
    self bsm_draw();
}

bsm_menu_close()
{
    self.bsm_open = false;
    self bsm_hud_destroy();
}

bsm_menu_back()
{
    depth = self.bsm_stack_menu.size;
    if (depth == 0)
    {
        self bsm_menu_close();
        return;
    }
    self.bsm_menu_id = self.bsm_stack_menu[depth - 1];
    self.bsm_cursor = self.bsm_stack_cursor[depth - 1];
    self.bsm_stack_menu[depth - 1] = undefined;
    self.bsm_stack_cursor[depth - 1] = undefined;
    self bsm_build(self.bsm_menu_id);
    self bsm_clamp_cursor();
    self bsm_draw();
}

bsm_menu_enter(menu_id)
{
    depth = self.bsm_stack_menu.size;
    self.bsm_stack_menu[depth] = self.bsm_menu_id;
    self.bsm_stack_cursor[depth] = self.bsm_cursor;
    self.bsm_menu_id = menu_id;
    self.bsm_cursor = 0;
    self.bsm_scroll = 0;
    self bsm_build(menu_id);
    self bsm_draw();
}

bsm_menu_move(step)
{
    count = self.bsm_items.size;
    if (count == 0)
    {
        return;
    }
    self.bsm_cursor = self.bsm_cursor + step;
    if (self.bsm_cursor >= count)
    {
        self.bsm_cursor = 0;
    }
    if (self.bsm_cursor < 0)
    {
        self.bsm_cursor = count - 1;
    }
    self bsm_clamp_cursor();
    self bsm_draw();
}

bsm_clamp_cursor()
{
    count = self.bsm_items.size;
    if (self.bsm_cursor >= count)
    {
        self.bsm_cursor = count - 1;
    }
    if (self.bsm_cursor < 0)
    {
        self.bsm_cursor = 0;
    }
    if (self.bsm_cursor < self.bsm_scroll)
    {
        self.bsm_scroll = self.bsm_cursor;
    }
    if (self.bsm_cursor >= self.bsm_scroll + level.bsm_lines)
    {
        self.bsm_scroll = self.bsm_cursor - level.bsm_lines + 1;
    }
    if (self.bsm_scroll < 0)
    {
        self.bsm_scroll = 0;
    }
}

bsm_menu_select()
{
    if (self.bsm_items.size == 0)
    {
        return;
    }
    item = self.bsm_items[self.bsm_cursor];
    if (item.action == "menu")
    {
        self bsm_menu_enter(item.arg);
        return;
    }
    if (item.action == "target")
    {
        self.bsm_target = item.arg;
        self bsm_menu_enter("target");
        return;
    }

    self bsm_run(item.action, item.arg);

    // Labels of switches change, and lists (players, weapons) may have changed.
    if (self.bsm_open)
    {
        self bsm_build(self.bsm_menu_id);
        self bsm_clamp_cursor();
        self bsm_draw();
    }
}

// ----------------------------------------------------------------------- hud

bsm_hud_text(x, y, scale)
{
    elem = NewClientHudElem(self);
    elem.horzAlign = "left";
    elem.vertAlign = "top";
    elem.alignX = "left";
    elem.alignY = "top";
    elem.x = x;
    elem.y = y;
    elem.fontScale = scale;
    elem.alpha = 1;
    elem.sort = 20;
    elem.foreground = true;
    elem.hidewheninmenu = true;
    elem.color = (1, 1, 1);
    return elem;
}

bsm_hud_create()
{
    bg = NewClientHudElem(self);
    bg.horzAlign = "left";
    bg.vertAlign = "top";
    bg.alignX = "left";
    bg.alignY = "top";
    bg.x = 20;
    bg.y = 90;
    bg.sort = 10;
    bg.foreground = false;
    bg.hidewheninmenu = true;
    bg.color = (0, 0, 0);
    bg.alpha = 0.65;
    bg SetShader("white", 240, 60 + level.bsm_lines * 16);
    self.bsm_hud_bg = bg;

    self.bsm_hud_title = self bsm_hud_text(30, 96, 1.6);
    self.bsm_hud_title.color = (1, 0.75, 0.2);

    self.bsm_hud_lines = [];
    for (i = 0; i < level.bsm_lines; i++)
    {
        self.bsm_hud_lines[i] = self bsm_hud_text(30, 120 + i * 16, 1.3);
    }

    self.bsm_hud_help = self bsm_hud_text(30, 126 + level.bsm_lines * 16, 1);
    self.bsm_hud_help.color = (0.7, 0.7, 0.7);
    self.bsm_hud_help SetText("Arrows / D-pad or FIRE AIM USE MELEE");
}

bsm_hud_destroy()
{
    if (isdefined(self.bsm_hud_bg))
    {
        self.bsm_hud_bg Destroy();
    }
    if (isdefined(self.bsm_hud_title))
    {
        self.bsm_hud_title Destroy();
    }
    if (isdefined(self.bsm_hud_help))
    {
        self.bsm_hud_help Destroy();
    }
    if (isdefined(self.bsm_hud_lines))
    {
        foreach (line in self.bsm_hud_lines)
        {
            if (isdefined(line))
            {
                line Destroy();
            }
        }
    }
    self.bsm_hud_lines = undefined;
}

bsm_draw()
{
    self.bsm_hud_title SetText(self.bsm_title);
    for (i = 0; i < level.bsm_lines; i++)
    {
        line = self.bsm_hud_lines[i];
        index = self.bsm_scroll + i;
        if (index < self.bsm_items.size)
        {
            line SetText(self.bsm_items[index].label);
            line.alpha = 1;
            if (index == self.bsm_cursor)
            {
                line.color = (1, 0.75, 0.2);
            }
            else
            {
                line.color = (1, 1, 1);
            }
        }
        else
        {
            line.alpha = 0;
        }
    }
}

// --------------------------------------------------------------------- menus

bsm_add(label, action, arg)
{
    item = SpawnStruct();
    item.label = label;
    item.action = action;
    item.arg = arg;
    self.bsm_items[self.bsm_items.size] = item;
}

bsm_on_off(value)
{
    if (isdefined(value) && value)
    {
        return "ON";
    }
    return "OFF";
}

bsm_build(menu_id)
{
    self.bsm_items = [];
    switch (menu_id)
    {
        case "main":
            self.bsm_title = "MOD MENU";
            self bsm_add("Player >", "menu", "player");
            self bsm_add("Points >", "menu", "points");
            self bsm_add("Weapons >", "menu", "weapons");
            self bsm_add("Perks >", "menu", "perks");
            self bsm_add("Power-ups >", "menu", "powerups");
            self bsm_add("Zombies >", "menu", "zombies");
            self bsm_add("Round >", "menu", "round");
            self bsm_add("Map >", "menu", "map");
            self bsm_add("Everyone >", "menu", "everyone");
            self bsm_add("Players >", "menu", "players");
            self bsm_add("Save / Load >", "menu", "saves");
            break;

        case "player":
            self.bsm_title = "PLAYER";
            self bsm_add("God mode: " + bsm_on_off(self.bsm_god), "god", 0);
            self bsm_add("Infinite ammo: " + bsm_on_off(self.bsm_ammo), "ammo", 0);
            self bsm_add("Zombies ignore me: " + bsm_on_off(self.bsm_ignore), "ignore", 0);
            self bsm_add("Auto revive: " + bsm_on_off(self.bsm_autorevive), "autorevive", 0);
            self bsm_add("Invisible: " + bsm_on_off(self.bsm_invisible), "invisible", 0);
            self bsm_add("Run speed: " + bsm_speed_label(self.bsm_speed), "fast", 0);
            self bsm_add("Unlimited sprint: " + bsm_on_off(self.bsm_sprint), "sprint", 0);
            self bsm_add("No weapon spread: " + bsm_on_off(self.bsm_nospread), "nospread", 0);
            self bsm_add("Fly (hold SPRINT to move): " + bsm_on_off(self.bsm_noclip), "noclip", 0);
            self bsm_add("Third person: " + bsm_on_off(self.bsm_third), "third", 0);
            self bsm_add("Teleport to crosshair", "tp_cross", 0);
            self bsm_add("Save this position", "pos_save", 0);
            self bsm_add("Return to saved position", "pos_load", 0);
            self bsm_add("Revive me", "revive_me", 0);
            break;

        case "points":
            self.bsm_title = "POINTS";
            self bsm_add("+1,000", "points", 1000);
            self bsm_add("+10,000", "points", 10000);
            self bsm_add("+100,000", "points", 100000);
            self bsm_add("+1,000,000", "points", 1000000);
            self bsm_add("-10,000", "points", -10000);
            self bsm_add("Set to 0", "points_zero", 0);
            self bsm_add("Double points for all: " + bsm_on_off(level.bsm_double), "double", 0);
            break;

        case "weapons":
            self.bsm_title = "WEAPONS";
            self bsm_add("Max ammo (all my weapons)", "maxammo", 0);
            self bsm_add("Pack-a-Punch current weapon", "pap", 0);
            self bsm_add("Pack-a-Punch all my weapons", "pap_all", 0);
            self bsm_add("Give weapon >", "menu", "give");
            self bsm_add("Drop current weapon", "drop", 0);
            self bsm_add("Reset to starting weapon", "takeall", 0);
            break;

        case "give":
            self.bsm_title = "GIVE WEAPON";
            if (isdefined(level.zombie_weapons))
            {
                weapons = GetArrayKeys(level.zombie_weapons);
                foreach (weapon in weapons)
                {
                    if (isdefined(weapon) && weapon != level.weaponNone)
                    {
                        self bsm_add(weapon.name, "give", weapon);
                    }
                }
            }
            break;

        case "perks":
            self.bsm_title = "PERKS";
            self bsm_add("Give all perks", "perks_all", 0);
            self bsm_add("Remove all perks", "perks_none", 0);
            self bsm_add("Keep perks when down: " + bsm_on_off(level.bsm_keep_perks), "keep_perks", 0);
            perks = bsm_map_perks();
            foreach (perk in perks)
            {
                self bsm_add(bsm_perk_name(perk), "perk", perk);
            }
            break;

        case "powerups":
            self.bsm_title = "DROP POWER-UP";
            if (isdefined(level.zombie_powerups))
            {
                names = GetArrayKeys(level.zombie_powerups);
                foreach (name in names)
                {
                    self bsm_add(name, "powerup", name);
                }
            }
            break;

        case "zombies":
            self.bsm_title = "ZOMBIES";
            self bsm_add("Kill all zombies", "killall", 0);
            self bsm_add("Insta-kill: " + bsm_on_off(level.bsm_instakill), "instakill", 0);
            self bsm_add("Freeze zombies: " + bsm_on_off(level.bsm_freeze), "freeze", 0);
            self bsm_add("Zombie speed: " + bsm_zspeed_name(level.bsm_zspeed), "zspeed", 0);
            self bsm_add("Bring zombies to crosshair", "ztele", 0);
            self bsm_add("Zombie spawning: " + bsm_on_off(level flag::get("spawn_zombies")), "spawns", 0);
            break;

        case "round":
            self.bsm_title = "ROUND " + level.round_number;
            self bsm_add("Next round", "round_add", 1);
            self bsm_add("Skip 5 rounds", "round_add", 5);
            self bsm_add("Skip 10 rounds", "round_add", 10);
            self bsm_add("Go to round 20", "round_set", 20);
            self bsm_add("Go to round 30", "round_set", 30);
            self bsm_add("Go to round 50", "round_set", 50);
            self bsm_add("Go to round 100", "round_set", 100);
            self bsm_add("Go to round 255", "round_set", 255);
            break;

        case "map":
            self.bsm_title = "MAP";
            self bsm_add("Turn power on", "power", 0);
            self bsm_add("Open all doors", "doors", 0);
            self bsm_add("Low gravity: " + bsm_on_off(level.bsm_gravity), "gravity", 0);
            self bsm_add("End game", "endgame", 0);
            self bsm_add("Restart map", "restart", 0);
            break;

        case "everyone":
            self.bsm_title = "EVERYONE";
            self bsm_add("God mode for all: " + bsm_on_off(level.bsm_god_all), "all_god", 0);
            self bsm_add("Infinite ammo for all: " + bsm_on_off(level.bsm_ammo_all), "all_ammo", 0);
            self bsm_add("+10,000 points each", "all_points", 10000);
            self bsm_add("All perks for all", "all_perks", 0);
            self bsm_add("Max ammo for all", "all_maxammo", 0);
            self bsm_add("Pack-a-Punch everyone's weapons", "all_pap", 0);
            self bsm_add("Revive everyone", "all_revive", 0);
            self bsm_add("Teleport everyone to me", "all_tp", 0);
            self bsm_add("Freeze the other players: " + bsm_on_off(level.bsm_freeze_players), "all_freeze", 0);
            break;

        case "saves":
            self.bsm_title = "SAVE / LOAD";
            self bsm_add("Save to slot 1: " + bsm_slot_text("1"), "save", "1");
            self bsm_add("Save to slot 2: " + bsm_slot_text("2"), "save", "2");
            self bsm_add("Save to slot 3: " + bsm_slot_text("3"), "save", "3");
            self bsm_add("Load slot 1: " + bsm_slot_text("1"), "load", "1");
            self bsm_add("Load slot 2: " + bsm_slot_text("2"), "load", "2");
            self bsm_add("Load slot 3: " + bsm_slot_text("3"), "load", "3");
            self bsm_add("Load autosave: " + bsm_slot_text("auto"), "load", "auto");
            self bsm_add("Autosave every round: " + bsm_on_off(level.bsm_autosave), "autosave", 0);
            break;

        case "players":
            self.bsm_title = "PLAYERS";
            players = GetPlayers();
            foreach (player in players)
            {
                self bsm_add(player.name + " >", "target", player);
            }
            break;

        case "target":
            if (!isdefined(self.bsm_target))
            {
                self.bsm_title = "PLAYER LEFT";
                break;
            }
            self.bsm_title = self.bsm_target.name;
            self bsm_add("+10,000 points", "t_points", 10000);
            self bsm_add("All perks", "t_perks", 0);
            self bsm_add("Max ammo", "t_maxammo", 0);
            self bsm_add("Revive", "t_revive", 0);
            self bsm_add("God mode: " + bsm_on_off(self.bsm_target.bsm_god), "t_god", 0);
            self bsm_add("Frozen: " + bsm_on_off(self.bsm_target.bsm_frozen), "t_freeze", 0);
            self bsm_add("Pack-a-Punch their weapons", "t_pap", 0);
            self bsm_add("Bring to me", "t_bring", 0);
            self bsm_add("Go to player", "t_goto", 0);
            self bsm_add("Kick from server", "t_kick", 0);
            break;

        default:
            self.bsm_title = "MOD MENU";
            break;
    }
}

// ------------------------------------------------------------------- actions

bsm_run(action, arg)
{
    switch (action)
    {
        case "god":
            self.bsm_god = !self.bsm_god;
            if (self.bsm_god)
            {
                self EnableInvulnerability();
                self.bsm_god_applied = true;
            }
            break;
        case "ammo":
            self.bsm_ammo = !self.bsm_ammo;
            break;
        case "ignore":
            self.bsm_ignore = !self.bsm_ignore;
            self.ignoreme = self.bsm_ignore;
            break;
        case "fast":
            self.bsm_speed = self.bsm_speed + 1;
            if (self.bsm_speed > 3)
            {
                self.bsm_speed = 0;
            }
            self SetMoveSpeedScale(bsm_speed_scale(self.bsm_speed));
            break;
        case "autorevive":
            self.bsm_autorevive = !self.bsm_autorevive;
            break;
        case "invisible":
            self.bsm_invisible = !self.bsm_invisible;
            if (self.bsm_invisible)
            {
                self Hide();
            }
            else
            {
                self Show();
            }
            break;
        case "nospread":
            self.bsm_nospread = !self.bsm_nospread;
            if (self.bsm_nospread)
            {
                self SetSpreadOverride(1);
            }
            else
            {
                self ResetSpreadOverride();
            }
            break;
        case "pos_save":
            self.bsm_saved_pos = self.origin;
            self.bsm_saved_angles = self GetPlayerAngles();
            self iprintlnbold("Position saved");
            break;
        case "pos_load":
            if (isdefined(self.bsm_saved_pos))
            {
                self bsm_allow_out_of_map();
                self SetOrigin(self.bsm_saved_pos);
                self SetPlayerAngles(self.bsm_saved_angles);
            }
            else
            {
                self iprintlnbold("No position saved yet");
            }
            break;
        case "sprint":
            self.bsm_sprint = !self.bsm_sprint;
            if (self.bsm_sprint)
            {
                self SetPerk("specialty_unlimitedsprint");
            }
            else
            {
                self UnSetPerk("specialty_unlimitedsprint");
            }
            break;
        case "noclip":
            self bsm_toggle_noclip();
            break;
        case "third":
            self.bsm_third = !self.bsm_third;
            if (self.bsm_third)
            {
                self SetClientThirdPerson(1);
            }
            else
            {
                self SetClientThirdPerson(0);
            }
            break;
        case "tp_cross":
            self bsm_teleport_to_crosshair();
            break;
        case "revive_me":
            self bsm_revive(self);
            break;

        case "points":
            self bsm_points(self, arg);
            break;
        case "points_zero":
            if (self.score > 0)
            {
                self zm_score::minus_to_player_score(self.score);
            }
            break;

        case "maxammo":
            self bsm_max_ammo();
            break;
        case "pap":
            self bsm_pack_a_punch();
            break;
        case "pap_all":
            self bsm_pack_a_punch_all();
            break;
        case "double":
            level.bsm_double = !level.bsm_double;
            break;
        case "keep_perks":
            level.bsm_keep_perks = !level.bsm_keep_perks;
            if (!level.bsm_keep_perks)
            {
                players = GetPlayers();
                foreach (player in players)
                {
                    player._retain_perks = false;
                }
            }
            break;
        case "instakill":
            level.bsm_instakill = !level.bsm_instakill;
            break;
        case "freeze":
            level.bsm_freeze = !level.bsm_freeze;
            if (!level.bsm_freeze)
            {
                zombies = GetAITeamArray(level.zombie_team);
                foreach (zombie in zombies)
                {
                    if (isdefined(zombie) && IsAlive(zombie) && isdefined(zombie.bsm_frozen))
                    {
                        zombie.bsm_frozen = undefined;
                        zombie PathMode("move allowed");
                        zombie.ignoreall = false;
                    }
                }
            }
            break;
        case "zspeed":
            level.bsm_zspeed = level.bsm_zspeed + 1;
            if (level.bsm_zspeed > 3)
            {
                level.bsm_zspeed = 0;
            }
            break;
        case "ztele":
            self bsm_zombies_to_crosshair();
            break;
        case "gravity":
            level.bsm_gravity = !level.bsm_gravity;
            if (level.bsm_gravity)
            {
                SetDvar("bg_gravity", "300");
            }
            else
            {
                SetDvar("bg_gravity", "800");
            }
            break;
        case "all_pap":
            players = GetPlayers();
            foreach (player in players)
            {
                player bsm_pack_a_punch_all();
            }
            break;
        case "all_freeze":
            level.bsm_freeze_players = !level.bsm_freeze_players;
            players = GetPlayers();
            foreach (player in players)
            {
                if (player != self)
                {
                    player.bsm_frozen = level.bsm_freeze_players;
                    player FreezeControls(level.bsm_freeze_players);
                }
            }
            break;
        case "t_god":
            if (isdefined(self.bsm_target) && isdefined(self.bsm_target.bsm_started))
            {
                self.bsm_target.bsm_god = !self.bsm_target.bsm_god;
            }
            break;
        case "t_freeze":
            if (isdefined(self.bsm_target) && isdefined(self.bsm_target.bsm_started))
            {
                self.bsm_target.bsm_frozen = !self.bsm_target.bsm_frozen;
                self.bsm_target FreezeControls(self.bsm_target.bsm_frozen);
            }
            break;
        case "t_pap":
            if (isdefined(self.bsm_target))
            {
                self.bsm_target bsm_pack_a_punch_all();
            }
            break;
        case "give":
            self bsm_give_weapon(arg);
            break;
        case "drop":
            weapon = self GetCurrentWeapon();
            if (isdefined(weapon) && weapon != level.weaponNone)
            {
                self DropItem(weapon);
            }
            break;
        case "takeall":
            // Never leave a player with no weapon at all: some mods end the
            // game for a player in that state.
            weapons = self GetWeaponsListPrimaries();
            foreach (weapon in weapons)
            {
                self TakeWeapon(weapon);
            }
            if (isdefined(level.start_weapon) && level.start_weapon != level.weaponNone)
            {
                self bsm_give_weapon(level.start_weapon);
            }
            break;

        case "perks_all":
            self bsm_give_all_perks();
            break;
        case "perks_none":
            perks = bsm_map_perks();
            foreach (perk in perks)
            {
                if (self HasPerk(perk))
                {
                    self notify(perk + "_stop");
                }
            }
            break;
        case "perk":
            if (!self HasPerk(arg))
            {
                self zm_perks::give_perk(arg, false);
            }
            break;

        case "powerup":
            spot = self.origin + VectorScale(AnglesToForward((0, self.angles[1], 0)), 90);
            level thread zm_powerups::specific_powerup_drop(arg, spot);
            break;

        case "killall":
            bsm_kill_all_zombies();
            break;
        case "spawns":
            if (level flag::get("spawn_zombies"))
            {
                level flag::clear("spawn_zombies");
            }
            else
            {
                level flag::set("spawn_zombies");
            }
            break;

        case "round_add":
            level thread bsm_goto_round(level.round_number + arg);
            break;
        case "round_set":
            level thread bsm_goto_round(arg);
            break;

        case "power":
            level thread zm_power::turn_power_on_and_open_doors();
            break;
        case "doors":
            self bsm_open_doors();
            break;
        case "endgame":
            level notify("end_game");
            break;
        case "restart":
            Map_Restart(false);
            break;

        case "all_god":
            level.bsm_god_all = !level.bsm_god_all;
            break;
        case "all_ammo":
            level.bsm_ammo_all = !level.bsm_ammo_all;
            break;
        case "all_points":
            players = GetPlayers();
            foreach (player in players)
            {
                self bsm_points(player, arg);
            }
            break;
        case "all_perks":
            players = GetPlayers();
            foreach (player in players)
            {
                player bsm_give_all_perks();
            }
            break;
        case "all_maxammo":
            players = GetPlayers();
            foreach (player in players)
            {
                player bsm_max_ammo();
            }
            break;
        case "all_revive":
            players = GetPlayers();
            foreach (player in players)
            {
                self bsm_revive(player);
            }
            break;
        case "all_tp":
            players = GetPlayers();
            foreach (player in players)
            {
                if (player != self)
                {
                    player SetOrigin(self.origin);
                }
            }
            break;

        case "save":
            if (bsm_save_game(arg))
            {
                bsm_tell_all(self.name + " saved the game (slot " + arg + ", round " + level.round_number + ")");
            }
            else
            {
                self iprintlnbold("Saving failed");
            }
            break;
        case "load":
            self bsm_menu_close();
            level thread bsm_load_game(arg, self);
            break;
        case "autosave":
            level.bsm_autosave = !level.bsm_autosave;
            break;
        case "t_points":
            if (isdefined(self.bsm_target))
            {
                self bsm_points(self.bsm_target, arg);
            }
            break;
        case "t_perks":
            if (isdefined(self.bsm_target))
            {
                self.bsm_target bsm_give_all_perks();
            }
            break;
        case "t_maxammo":
            if (isdefined(self.bsm_target))
            {
                self.bsm_target bsm_max_ammo();
            }
            break;
        case "t_revive":
            if (isdefined(self.bsm_target))
            {
                self bsm_revive(self.bsm_target);
            }
            break;
        case "t_bring":
            if (isdefined(self.bsm_target))
            {
                self.bsm_target SetOrigin(self.origin);
            }
            break;
        case "t_goto":
            if (isdefined(self.bsm_target))
            {
                self SetOrigin(self.bsm_target.origin);
            }
            break;
        case "t_kick":
            if (isdefined(self.bsm_target) && self.bsm_target != self)
            {
                Kick(self.bsm_target GetEntityNumber());
            }
            break;
    }
}

bsm_points(player, amount)
{
    if (amount >= 0)
    {
        player zm_score::add_to_player_score(amount);
        return;
    }
    take = 0 - amount;
    if (take > player.score)
    {
        take = player.score;
    }
    if (take > 0)
    {
        player zm_score::minus_to_player_score(take);
    }
}

bsm_revive(player)
{
    if (isdefined(player.laststand) && player.laststand)
    {
        player zm_laststand::auto_revive(self);
    }
}

bsm_max_ammo()
{
    weapons = self GetWeaponsList();
    foreach (weapon in weapons)
    {
        self GiveMaxAmmo(weapon);
        self SetWeaponAmmoClip(weapon, weapon.clipSize);
    }
}

// Gives a weapon the way the mystery box does. The game may hand out a
// variant of it (with the player's attachments), so the weapon actually
// given is returned and switched to.
bsm_give_weapon(weapon)
{
    given = self zm_weapons::weapon_give(weapon);
    if (!isdefined(given))
    {
        given = weapon;
    }
    if (self HasWeapon(given))
    {
        self SwitchToWeapon(given);
    }
    return given;
}

bsm_pack_a_punch()
{
    weapon = self GetCurrentWeapon();
    if (!isdefined(weapon) || weapon == level.weaponNone || !zm_weapons::can_upgrade_weapon(weapon))
    {
        self iprintlnbold("This weapon cannot be upgraded");
        return;
    }
    upgraded = zm_weapons::get_upgrade_weapon(weapon);
    self TakeWeapon(weapon);
    return self bsm_give_weapon(upgraded);
}

// Upgrades every weapon the player carries that can be upgraded.
bsm_pack_a_punch_all()
{
    weapons = self GetWeaponsListPrimaries();
    foreach (weapon in weapons)
    {
        if (!isdefined(weapon) || weapon == level.weaponNone || !zm_weapons::can_upgrade_weapon(weapon))
        {
            continue;
        }
        upgraded = zm_weapons::get_upgrade_weapon(weapon);
        self TakeWeapon(weapon);
        self bsm_give_weapon(upgraded);
    }
}

// Moves every living zombie to the spot the player is looking at.
bsm_zombies_to_crosshair()
{
    start = self GetEye();
    end = start + VectorScale(AnglesToForward(self GetPlayerAngles()), 10000);
    trace = BulletTrace(start, end, false, self);
    if (!isdefined(trace["position"]))
    {
        return;
    }
    zombies = GetAITeamArray(level.zombie_team);
    foreach (zombie in zombies)
    {
        if (isdefined(zombie) && IsAlive(zombie))
        {
            zombie ForceTeleport(trace["position"], zombie.angles);
        }
    }
}

// Perks that have a machine on the current map.
bsm_map_perks()
{
    perks = [];
    machines = GetEntArray("zombie_vending", "targetname");
    foreach (machine in machines)
    {
        if (!isdefined(machine.script_noteworthy))
        {
            continue;
        }
        known = false;
        foreach (perk in perks)
        {
            if (perk == machine.script_noteworthy)
            {
                known = true;
            }
        }
        if (!known)
        {
            perks[perks.size] = machine.script_noteworthy;
        }
    }
    return perks;
}

bsm_perk_name(perk)
{
    switch (perk)
    {
        case "specialty_armorvest": return "Juggernog";
        case "specialty_quickrevive": return "Quick Revive";
        case "specialty_fastreload": return "Speed Cola";
        case "specialty_doubletap2": return "Double Tap";
        case "specialty_staminup": return "Stamin-Up";
        case "specialty_deadshot": return "Deadshot Daiquiri";
        case "specialty_additionalprimaryweapon": return "Mule Kick";
        case "specialty_widowswine": return "Widow's Wine";
        case "specialty_electriccherry": return "Electric Cherry";
    }
    return perk;
}

bsm_give_all_perks()
{
    perks = bsm_map_perks();
    foreach (perk in perks)
    {
        if (!self HasPerk(perk))
        {
            self zm_perks::give_perk(perk, false);
        }
    }
}

bsm_kill_all_zombies()
{
    zombies = GetAITeamArray(level.zombie_team);
    foreach (zombie in zombies)
    {
        if (isdefined(zombie) && IsAlive(zombie))
        {
            zombie DoDamage(zombie.health + 666, zombie.origin);
        }
    }
}

bsm_goto_round(target)
{
    if (target < 1)
    {
        target = 1;
    }
    if (target > 255)
    {
        target = 255;
    }
    level.zombie_total = 0;
    zm::set_round_number(target - 1);
    level notify("kill_round");
    wait 1;
    bsm_kill_all_zombies();
}

// Opens every buyable door and clears every debris pile for free.
bsm_open_doors()
{
    names = [];
    names[0] = "zombie_door";
    names[1] = "zombie_airlock_buy";
    names[2] = "zombie_debris";
    foreach (name in names)
    {
        doors = GetEntArray(name, "targetname");
        foreach (door in doors)
        {
            door notify("trigger", self, true);
        }
    }
}

// The game kills a player who is outside the map's playable area, even in
// god mode. Flying and teleporting take players there, so that watch is
// stopped for this player (it starts again when the player respawns).
bsm_allow_out_of_map()
{
    self notify("stop_player_out_of_playable_area_monitor");
}

bsm_teleport_to_crosshair()
{
    self bsm_allow_out_of_map();
    start = self GetEye();
    end = start + VectorScale(AnglesToForward(self GetPlayerAngles()), 10000);
    trace = BulletTrace(start, end, false, self);
    if (isdefined(trace["position"]))
    {
        self SetOrigin(trace["position"]);
    }
}

bsm_toggle_noclip()
{
    self.bsm_noclip = !self.bsm_noclip;
    if (self.bsm_noclip)
    {
        self bsm_allow_out_of_map();
        self thread bsm_noclip_think();
        return;
    }
    self notify("bsm_noclip_off");
    self Unlink();
    if (isdefined(self.bsm_anchor))
    {
        self.bsm_anchor Delete();
    }
}

bsm_noclip_think()
{
    self endon("disconnect");
    self endon("bsm_noclip_off");
    anchor = Spawn("script_origin", self.origin);
    self.bsm_anchor = anchor;
    self PlayerLinkTo(anchor);
    for (;;)
    {
        wait 0.05;
        if (self SprintButtonPressed())
        {
            anchor.origin = anchor.origin + VectorScale(AnglesToForward(self GetPlayerAngles()), 40);
        }
    }
}

// ------------------------------------------------------------------ save / load
//
// Saves go into the server's boiii\scriptdata\servermaker folder through
// BOIII's own file functions, one text file per slot:
//
//   BSMSAVE;1
//   map;zm_zod
//   round;12
//   power;1
//   player;<points>;<perk,perk,...|->;<weapon:clip:stock,...|->;<name>
//
// Doors, quest and easter-egg progress are not saved.

bsm_save_path(slot)
{
    return "servermaker/save_" + slot + ".txt";
}

bsm_tell_all(text)
{
    players = GetPlayers();
    foreach (player in players)
    {
        player iprintlnbold(text);
    }
}

bsm_save_game(slot)
{
    power = 0;
    if (level flag::get("power_on"))
    {
        power = 1;
    }
    data = "BSMSAVE;1\n";
    data = data + "map;" + level.script + "\n";
    data = data + "round;" + level.round_number + "\n";
    data = data + "power;" + power + "\n";
    players = GetPlayers();
    foreach (player in players)
    {
        data = data + player bsm_save_player_line() + "\n";
    }
    ok = writefile(bsm_save_path(slot), data);
    return isdefined(ok) && ok;
}

bsm_save_player_line()
{
    perks = "";
    map_perks = bsm_map_perks();
    foreach (perk in map_perks)
    {
        if (self HasPerk(perk))
        {
            perks = perks + perk + ",";
        }
    }
    if (perks == "")
    {
        perks = "-";
    }
    weapons = "";
    held = self GetWeaponsListPrimaries();
    foreach (weapon in held)
    {
        if (!isdefined(weapon) || weapon == level.weaponNone)
        {
            continue;
        }
        weapons = weapons + weapon.name + ":" + self GetWeaponAmmoClip(weapon) + ":" + self GetWeaponAmmoStock(weapon) + ",";
    }
    if (weapons == "")
    {
        weapons = "-";
    }
    return "player;" + self.score + ";" + perks + ";" + weapons + ";" + self.name;
}

// Reads a save file. Returns undefined if the slot is empty or not a save.
bsm_read_save(slot)
{
    path = bsm_save_path(slot);
    if (!fileexists(path))
    {
        return undefined;
    }
    text = readfile(path);
    if (!isdefined(text))
    {
        return undefined;
    }
    lines = StrTok(text, "\n");
    if (lines.size < 1 || lines[0] != "BSMSAVE;1")
    {
        return undefined;
    }
    save = SpawnStruct();
    save.map = "";
    save.round = 1;
    save.power = 0;
    save.players = [];
    foreach (line in lines)
    {
        f = StrTok(line, ";");
        if (f.size < 2)
        {
            continue;
        }
        if (f[0] == "map")
        {
            save.map = f[1];
        }
        else if (f[0] == "round")
        {
            save.round = Int(f[1]);
        }
        else if (f[0] == "power")
        {
            save.power = Int(f[1]);
        }
        else if (f[0] == "player" && f.size >= 5)
        {
            p = SpawnStruct();
            p.score = Int(f[1]);
            p.perks = f[2];
            p.weapons = f[3];
            name = f[4];
            for (i = 5; i < f.size; i++)
            {
                name = name + ";" + f[i];
            }
            p.name = ToLower(name);
            save.players[save.players.size] = p;
        }
    }
    return save;
}

// Short description of a slot for the menu.
bsm_slot_text(slot)
{
    save = bsm_read_save(slot);
    if (!isdefined(save))
    {
        return "empty";
    }
    return save.map + ", round " + save.round + ", " + save.players.size + " player(s)";
}

// Loads a save into the running game: round, power, and every player's
// points, weapons and perks (by name; players who join later get theirs
// when they spawn).
bsm_load_game(slot, who)
{
    save = bsm_read_save(slot);
    if (!isdefined(save))
    {
        if (isdefined(who))
        {
            who iprintlnbold("Slot " + slot + " is empty");
        }
        return false;
    }
    if (save.map != level.script)
    {
        if (isdefined(who))
        {
            who iprintlnbold("Slot " + slot + " is a save of " + save.map + ", not this map");
        }
        return false;
    }
    level.bsm_pending = save.players;
    players = GetPlayers();
    foreach (player in players)
    {
        player.bsm_restored = undefined;
        player thread bsm_apply_pending();
    }
    if (save.power == 1 && !level flag::get("power_on"))
    {
        level thread zm_power::turn_power_on_and_open_doors();
    }
    if (save.round != level.round_number)
    {
        if (level flag::get("start_zombie_round_logic"))
        {
            level thread bsm_goto_round(save.round);
        }
        else
        {
            // Before the first round: the game starts its rounds from here.
            zm::set_round_number(save.round);
        }
    }
    SetDvar("bsm_loaded_save", slot + ";" + save.round);
    bsm_tell_all("Saved game loaded: round " + save.round);
    return true;
}

// Continues from a save when the game starts (set by the host app).
bsm_autoload(slot)
{
    level endon("end_game");
    while (GetPlayers().size == 0)
    {
        wait 1;
    }
    wait 5;
    bsm_load_game(slot, undefined);
}

// Saves to the slot "auto" whenever a new round begins.
bsm_autosave_watch()
{
    level endon("end_game");
    last = level.round_number;
    for (;;)
    {
        wait 2;
        if (!isdefined(level.round_number) || level.round_number == last)
        {
            continue;
        }
        last = level.round_number;
        if (level.bsm_autosave && last > 1 && GetPlayers().size > 0)
        {
            bsm_save_game("auto");
        }
    }
}

bsm_apply_pending()
{
    self endon("disconnect");
    if (!isdefined(level.bsm_pending) || isdefined(self.bsm_restored))
    {
        return;
    }
    me = ToLower(self.name);
    foreach (p in level.bsm_pending)
    {
        if (p.name != me)
        {
            continue;
        }
        self.bsm_restored = true;
        self bsm_restore_player(p);
        self iprintlnbold("Your saved points, weapons and perks are back");
        return;
    }
}

// "ar_standard_upgraded+extclip+fmj" back to a weapon, attachments included
// when the game accepts them, otherwise the plain weapon.
bsm_weapon_from_name(full)
{
    bits = StrTok(full, "+");
    weapon = undefined;
    switch (bits.size)
    {
        case 2: weapon = GetWeapon(bits[0], bits[1]); break;
        case 3: weapon = GetWeapon(bits[0], bits[1], bits[2]); break;
        case 4: weapon = GetWeapon(bits[0], bits[1], bits[2], bits[3]); break;
        case 5: weapon = GetWeapon(bits[0], bits[1], bits[2], bits[3], bits[4]); break;
        case 6: weapon = GetWeapon(bits[0], bits[1], bits[2], bits[3], bits[4], bits[5]); break;
    }
    if (!isdefined(weapon) || weapon == level.weaponNone)
    {
        weapon = GetWeapon(bits[0]);
    }
    return weapon;
}

bsm_restore_player(p)
{
    if (p.score > self.score)
    {
        self zm_score::add_to_player_score(p.score - self.score);
    }
    else if (p.score < self.score)
    {
        self zm_score::minus_to_player_score(self.score - p.score);
    }

    if (p.weapons != "-")
    {
        held = self GetWeaponsListPrimaries();
        foreach (h in held)
        {
            self TakeWeapon(h);
        }
        items = StrTok(p.weapons, ",");
        last = undefined;
        foreach (item in items)
        {
            parts = StrTok(item, ":");
            weapon = bsm_weapon_from_name(parts[0]);
            if (!isdefined(weapon) || weapon == level.weaponNone)
            {
                continue;
            }
            given = self bsm_give_weapon(weapon);
            if (parts.size >= 3)
            {
                self SetWeaponAmmoClip(given, Int(parts[1]));
                self SetWeaponAmmoStock(given, Int(parts[2]));
            }
            last = given;
        }
        if (isdefined(last))
        {
            self SwitchToWeapon(last);
        }
    }

    if (p.perks != "-")
    {
        perks = StrTok(p.perks, ",");
        foreach (perk in perks)
        {
            if (!self HasPerk(perk))
            {
                self zm_perks::give_perk(perk, false);
            }
        }
    }
}