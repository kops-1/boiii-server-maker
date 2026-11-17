
// ---------------------------------------------------------------- self-test
// Test harness only (never shipped): adds a test client and runs the menu
// and its actions on it, reporting through dvars read over rcon.

bsm_selftest()
{
    if (GetDvarInt("bsm_selftest") == 0)
    {
        return;
    }
    SetDvar("bsm_t_stage", "waiting");
    wait 15;
    bot = AddTestClient();
    if (!isdefined(bot))
    {
        SetDvar("bsm_t_stage", "no_bot");
        return;
    }
    SetDvar("bsm_t_stage", "bot_added");
    for (i = 0; i < 1200; i++)
    {
        if (isdefined(bot.bsm_started))
        {
            break;
        }
        wait 0.1;
    }
    if (!isdefined(bot.bsm_started))
    {
        SetDvar("bsm_t_stage", "bot_never_spawned");
        return;
    }
    SetDvar("bsm_t_stage", "bot_ready");
    mode = GetDvarInt("bsm_selftest");
    if (mode == 2)
    {
        // Idle: the bot does nothing; only its state is sampled.
        trace = "";
        prev = "";
        t0 = GetTime();
        for (i = 0; i < 80; i++)
        {
            now = "alive" + IsAlive(bot) + ",down" + (isdefined(bot.laststand) && bot.laststand) + "," + bot.sessionstate + ",hp" + bot.health;
            if (now != prev)
            {
                trace = trace + (GetTime() - t0) + "ms=" + now + " ";
                prev = now;
                SetDvar("bsm_t_idle", trace);
            }
            wait 0.5;
        }
        SetDvar("bsm_t_stage", "idle_done");
        return;
    }
    if (mode == 4)
    {
        // On demand: run the actions named in bsm_t_list whenever bsm_t_go is set to 1.
        SetDvar("bsm_t_go", "0");
        for (;;)
        {
            wait 0.25;
            if (GetDvarInt("bsm_t_go") != 1)
            {
                continue;
            }
            SetDvar("bsm_t_go", "0");
            actions = StrTok(GetDvarString("bsm_t_list"), ",");
            trace = "";
            prev = "";
            n = 0;
            foreach (a in actions)
            {
                n++;
                bot.bsm_t_ok = true;
                if (a == "spawnz")
                {
                    made = 0;
                    if (isdefined(level.zombie_spawners) && level.zombie_spawners.size > 0)
                    {
                        for (z = 0; z < 4; z++)
                        {
                            guy = zombie_utility::spawn_zombie(level.zombie_spawners[0]);
                            if (isdefined(guy))
                            {
                                made++;
                            }
                        }
                    }
                    trace = trace + "spawned=" + made + " ";
                }
                else if (a == "report")
                {
                    zs = GetAITeamArray(level.zombie_team);
                    frozen = 0;
                    sped = 0;
                    foreach (zz in zs)
                    {
                        if (isdefined(zz.bsm_frozen))
                        {
                            frozen++;
                        }
                        if (isdefined(zz.bsm_speed))
                        {
                            sped++;
                        }
                    }
                    insta = "?";
                    scalar = "?";
                    if (isdefined(level.zombie_vars) && isdefined(level.zombie_vars[bot.team]))
                    {
                        insta = level.zombie_vars[bot.team]["zombie_insta_kill"];
                        scalar = level.zombie_vars[bot.team]["zombie_point_scalar"];
                    }
                    trace = trace + "[zombies=" + zs.size + " frozen=" + frozen + " sped=" + sped + " insta=" + insta + " scalar=" + scalar + " retain=" + isdefined(bot._retain_perks) + " gravity=" + GetDvarString("bg_gravity") + " speed=" + bot.bsm_speed + " autorevive=" + bot.bsm_autorevive + " score=" + bot.score + "] ";
                }
                else if (a == "givestuff")
                {
                    bot zm_score::add_to_player_score(12000);
                    bot bsm_give_all_perks();
                    w = GetWeapon("ar_standard_upgraded");
                    bot bsm_give_weapon(w);
                    bot SetWeaponAmmoClip(w, 7);
                    trace = trace + "gave ";
                }
                else if (a == "wipe")
                {
                    held = bot GetWeaponsListPrimaries();
                    foreach (h in held)
                    {
                        bot TakeWeapon(h);
                    }
                    bot bsm_run("perks_none", 0);
                    bot bsm_run("points_zero", 0);
                    trace = trace + "wiped ";
                }
                else if (a == "saveslot1")
                {
                    trace = trace + "saved=" + bsm_save_game("1") + " ";
                }
                else if (a == "loadslot1")
                {
                    trace = trace + "loaded=" + bsm_load_game("1", bot) + " ";
                }
                else if (a == "adoptsave")
                {
                    if (isdefined(level.bsm_pending) && level.bsm_pending.size > 0)
                    {
                        level.bsm_pending[0].name = ToLower(bot.name);
                        bot.bsm_restored = undefined;
                        bot thread bsm_apply_pending();
                        trace = trace + "adopted ";
                    }
                    else
                    {
                        trace = trace + "nopending ";
                    }
                }
                else if (a == "state")
                {
                    names = "";
                    held = bot GetWeaponsListPrimaries();
                    foreach (h in held)
                    {
                        names = names + h.name + "(" + bot GetWeaponAmmoClip(h) + ") ";
                    }
                    perks = 0;
                    mp = bsm_map_perks();
                    foreach (pk in mp)
                    {
                        if (bot HasPerk(pk))
                        {
                            perks++;
                        }
                    }
                    trace = trace + "[score=" + bot.score + " perks=" + perks + "/" + mp.size + " weapons=" + names + "round=" + level.round_number + " power=" + level flag::get("power_on") + " loadedsave=" + GetDvarString("bsm_loaded_save") + "] ";
                }                else if (a == "slots")
                {
                    bot.bsm_t_ok = false;
                    bot thread bsm_selftest_slots();
                }
                else if (a == "openmenu")
                {
                    bot bsm_menu_open();
                }
                else if (a != "idle")
                {
                    bot.bsm_t_ok = false;
                    bot thread bsm_selftest_one(a, 0);
                }
                wait 0.7;
                if (!bot.bsm_t_ok)
                {
                    trace = trace + "FAILED:" + a + " ";
                }                now = "down" + (isdefined(bot.laststand) && bot.laststand) + "," + bot.sessionstate + ",hp" + bot.health;
                if (now != prev)
                {
                    trace = trace + n + "." + a + "=" + now + "@" + int(bot.origin[0]) + "/" + int(bot.origin[1]) + "/" + int(bot.origin[2]) + " ";
                    prev = now;
                }
            }
            SetDvar("bsm_t_trace", trace);
            SetDvar("bsm_t_stage", "list_done");
        }
    }
    if (mode == 3)    {
        // Quick: give every weapon of the map straight away, before anything else can happen.
        t0 = GetTime();
        given = 0;
        total = 0;
        down_at = "never";
        weapons = GetArrayKeys(level.zombie_weapons);
        foreach (weapon in weapons)
        {
            if (!isdefined(weapon) || weapon == level.weaponNone)
            {
                continue;
            }
            if (isdefined(bot.laststand) && bot.laststand)
            {
                down_at = (GetTime() - t0) + "ms";
                break;
            }
            total++;
            held_now = bot GetWeaponsListPrimaries();
            foreach (h in held_now)
            {
                bot TakeWeapon(h);
            }
            bot.bsm_t_ok = false;
            bot.bsm_t_given = undefined;
            bot thread bsm_selftest_give(weapon);
            wait 0.1;
            if (bot.bsm_t_ok && isdefined(bot.bsm_t_given) && bot HasWeapon(bot.bsm_t_given))
            {
                given++;
            }
        }
        SetDvar("bsm_t_quick", "given=" + given + "/" + total + " of " + weapons.size + " in " + (GetTime() - t0) + "ms, bot down at " + down_at);
        SetDvar("bsm_t_stage", "quick_done");
        return;
    }
    wait 3;
    // Menus: open, enter every menu, move the cursor, count the items.
    counts = "";
    bot bsm_menu_open();
    counts = counts + "main=" + bot.bsm_items.size + " ";
    menus = StrTok("player,points,weapons,give,perks,powerups,zombies,round,map,everyone,players", ",");
    foreach (m in menus)
    {
        bot bsm_menu_enter(m);
        bot bsm_menu_move(1);
        bot bsm_menu_move(-1);
        bot bsm_menu_move(-1);
        counts = counts + m + "=" + bot.bsm_items.size + " ";
        bot bsm_menu_back();
    }
    bot.bsm_target = bot;
    bot bsm_menu_enter("target");
    counts = counts + "target=" + bot.bsm_items.size;
    bot bsm_menu_back();
    bot bsm_menu_back();
    SetDvar("bsm_t_menus", counts);
    SetDvar("bsm_t_hud", "open_after_back=" + bot.bsm_open);
    SetDvar("bsm_t_stage", "menus_done");

    // Selecting through the menu itself (as the USE button does).
    bot bsm_menu_open();
    bot bsm_menu_select();
    title = bot.bsm_title;
    bot bsm_menu_select();
    SetDvar("bsm_t_select", "entered=" + title + " god=" + bot.bsm_god);
    bot bsm_menu_close();
    bot bsm_run("god", 0);

    // Simple actions, each in its own thread so one failure does not stop the rest.
    actions = StrTok("god,ammo,ignore,fast,sprint,third,third,noclip,noclip,tp_cross,revive_me,maxammo,pap,perks_all,perks_none,perks_all,killall,spawns,spawns,power,doors,all_god,all_god,all_ammo,all_ammo,all_perks,all_maxammo,all_revive,all_tp,t_perks,t_maxammo,t_revive,t_bring,t_goto,t_kick,takeall", ",");
    failed = "";
    ran = 0;
    trace = "logic=" + level flag::get("start_zombie_round_logic") + " ";
    prev = "";
    foreach (a in actions)
    {
        SetDvar("bsm_t_stage", "action_" + a);
        bot.bsm_t_ok = false;
        bot thread bsm_selftest_one(a, 0);
        wait 0.7;
        now = "alive" + IsAlive(bot) + ",down" + (isdefined(bot.laststand) && bot.laststand) + "," + bot.sessionstate + ",hp" + bot.health + ",zombies" + GetAITeamArray(level.zombie_team).size;
        if (now != prev)
        {
            trace = trace + (ran + 1) + "." + a + "=" + now + " ";
            prev = now;
            SetDvar("bsm_t_trace", trace);
        }
        if (bot.bsm_t_ok)
        {
            ran++;
        }
        else
        {
            failed = failed + a + " ";
        }
    }
    SetDvar("bsm_t_actions", "ran=" + ran + "/" + actions.size + " failed=" + failed);

    // Points.
    SetDvar("bsm_t_stage", "phase_points");
    s0 = bot.score;
    bot thread bsm_selftest_one("points", 10000);
    wait 0.5;
    s1 = bot.score;
    bot thread bsm_selftest_one("points", -1000);
    wait 0.5;
    s2 = bot.score;
    bot thread bsm_selftest_one("t_points", 10000);
    wait 0.5;
    bot thread bsm_selftest_one("all_points", 10000);
    wait 0.5;
    s3 = bot.score;
    bot thread bsm_selftest_one("points_zero", 0);
    wait 0.5;
    SetDvar("bsm_t_points", s0 + ">" + s1 + ">" + s2 + ">" + s3 + ">" + bot.score);

    // Perks actually held after "all perks".
    SetDvar("bsm_t_stage", "phase_perks");
    perks = bsm_map_perks();
    held = 0;
    foreach (perk in perks)
    {
        if (bot HasPerk(perk))
        {
            held++;
        }
    }
    bot thread bsm_selftest_one("perks_none", 0);
    wait 1;
    left = 0;
    foreach (perk in perks)
    {
        if (bot HasPerk(perk))
        {
            left++;
        }
    }
    one = "none";
    if (perks.size > 0)
    {
        bot thread bsm_selftest_one("perk", perks[0]);
        wait 1;
        one = bsm_perk_name(perks[0]) + "=" + bot HasPerk(perks[0]);
    }
    SetDvar("bsm_t_perks", "all=" + held + "/" + perks.size + " after_remove=" + left + " single:" + one);

    // Give every weapon of the map once, through the menu's own function.
    given = 0;
    total = 0;
    bad = "";
    upgradable = undefined;
    if (isdefined(level.zombie_weapons))
    {
        weapons = GetArrayKeys(level.zombie_weapons);
        foreach (weapon in weapons)
        {
            if (!isdefined(weapon) || weapon == level.weaponNone)
            {
                continue;
            }
            total++;
            SetDvar("bsm_t_stage", "give_" + weapon.name);
            held_now = bot GetWeaponsListPrimaries();
            foreach (h in held_now)
            {
                bot TakeWeapon(h);
            }
            bot.bsm_t_ok = false;
            bot.bsm_t_given = undefined;
            bot thread bsm_selftest_give(weapon);
            wait 0.4;
            if (!bot.bsm_t_ok)
            {
                bad = bad + weapon.name + ":nothread ";
            }
            else if (!isdefined(bot.bsm_t_given) || !bot HasWeapon(bot.bsm_t_given))
            {
                bad = bad + weapon.name + ":nothave ";
            }
            else
            {
                given++;
                if (!isdefined(upgradable) && bot.bsm_t_given == weapon && zm_weapons::can_upgrade_weapon(weapon))
                {
                    upgradable = weapon;
                }
            }
        }
    }
    SetDvar("bsm_t_give", "given=" + given + "/" + total);
    SetDvar("bsm_t_give_bad", bad);

    // Pack-a-Punch through the menu's own function, on every weapon that can be upgraded.
    pap_ok = 0;
    pap_total = 0;
    pap_bad = "";
    if (isdefined(level.zombie_weapons))
    {
        weapons = GetArrayKeys(level.zombie_weapons);
        foreach (weapon in weapons)
        {
            if (!isdefined(weapon) || weapon == level.weaponNone || !zm_weapons::can_upgrade_weapon(weapon))
            {
                continue;
            }
            pap_total++;
            SetDvar("bsm_t_stage", "pap_" + weapon.name);
            held_now = bot GetWeaponsListPrimaries();
            foreach (h in held_now)
            {
                bot TakeWeapon(h);
            }
            bot.bsm_t_ok = false;
            bot.bsm_t_given = undefined;
            bot thread bsm_selftest_pap(weapon);
            wait 0.4;
            if (bot.bsm_t_ok && isdefined(bot.bsm_t_given) && bot HasWeapon(bot.bsm_t_given) && zm_weapons::is_weapon_upgraded(bot.bsm_t_given))
            {
                pap_ok++;
            }
            else
            {
                pap_bad = pap_bad + weapon.name + " ";
            }
        }
    }
    SetDvar("bsm_t_pap", "upgraded=" + pap_ok + "/" + pap_total + " failed=" + pap_bad);    bot thread bsm_selftest_one("drop", 0);
    wait 0.5;

    // Drop every power-up of the map.
    dropped = 0;
    ptotal = 0;
    pbad = "";
    if (isdefined(level.zombie_powerups))
    {
        names = GetArrayKeys(level.zombie_powerups);
        foreach (name in names)
        {
            ptotal++;
            SetDvar("bsm_t_stage", "powerup_" + name);
            bot.bsm_t_ok = false;
            bot thread bsm_selftest_one("powerup", name);
            wait 0.5;
            if (bot.bsm_t_ok)
            {
                dropped++;
            }
            else
            {
                pbad = pbad + name + " ";
            }
        }
    }
    SetDvar("bsm_t_powerups", "dropped=" + dropped + "/" + ptotal + " failed=" + pbad);

    SetDvar("bsm_t_stage", "phase_rounds");
    // Rounds. First see whether the round logic is running at all with only a test client.
    for (i = 0; i < 8; i++)
    {
        if (GetAITeamArray(level.zombie_team).size > 0)
        {
            break;
        }
        wait 1;
    }
    SetDvar("bsm_t_roundstate", "logic=" + level flag::get("start_zombie_round_logic") + " blackscreen=" + level flag::get("initial_blackscreen_passed") + " spawning=" + level flag::get("spawn_zombies") + " zombies=" + GetAITeamArray(level.zombie_team).size + " total=" + level.zombie_total);
    r0 = level.round_number;
    bot thread bsm_selftest_one("round_add", 1);
    wait 6;
    r1 = level.round_number;
    bot thread bsm_selftest_one("round_add", 5);
    wait 6;
    r2 = level.round_number;
    bot thread bsm_selftest_one("round_set", 20);
    wait 6;
    SetDvar("bsm_t_round", r0 + ">" + r1 + ">" + r2 + ">" + level.round_number);
    SetDvar("bsm_t_state", "power=" + level flag::get("power_on") + " spawning=" + level flag::get("spawn_zombies") + " players=" + GetPlayers().size);
    SetDvar("bsm_t_stage", "finished");
}

bsm_selftest_one(action, arg)
{
    self bsm_run(action, arg);
    self.bsm_t_ok = true;
}

bsm_selftest_give(weapon)
{
    self.bsm_t_given = self bsm_give_weapon(weapon);
    self.bsm_t_ok = true;
}
// The menu's Pack-a-Punch works on the weapon in the player's hands. A test
// client never finishes switching weapons, so the same steps are run here on
// a weapon given directly.
bsm_selftest_pap(weapon)
{
    self GiveWeapon(weapon);
    upgraded = zm_weapons::get_upgrade_weapon(weapon);
    self TakeWeapon(weapon);
    self.bsm_t_given = self bsm_give_weapon(upgraded);
    self.bsm_t_ok = true;
}
bsm_selftest_slots()
{
    a = self ActionSlotOneButtonPressed();
    b = self ActionSlotTwoButtonPressed();
    c = self ActionSlotThreeButtonPressed();
    d = self ActionSlotFourButtonPressed();
    self.bsm_t_ok = true;
}
