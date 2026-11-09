# RustyExile

A Path of Exile 1 campaign overlay for Hyprland and Windows. It follows the game window, stays click-through, and turns act and gem steps on zone entry the way ExileUI does.

This is a non-commercial fan tool. It is not for sale and is not endorsed by Grinding Gear Games. See `NOTICE` for the route data, gem icons, and regex presets.

The overlay does not read game memory and does not inject anything. It tails `logs/Client.txt`.

## Setup

Linux, in this folder:

```
./installLinux.sh
```

On Windows, open Git Bash in this folder and run:

```
bash installWindows.bash
```

The script asks before it installs anything. It installs Rust when Rust is missing, installs the libraries the build needs, and builds the release program. On Linux it can add an app-menu shortcut and add your user to the `input` group. On Windows it can add a Start Menu shortcut. Start Path of Exile, then start RustyExile.

## What it shows

Zone entry shows the step or gem whose start zone you just entered, and hides it when the next route zone is entered. Ctrl+Alt+click clears a line whose end zone has not been entered yet. The game log does not record quest turn-ins, gem pickups, or vendor buys.

Passive-tree specs imported from Path of Building are stored and are not drawn.

## First run

The first window asks you to start as Beginner, Intermediate, or Advanced.

- Beginner keeps zone tips, the act list, and landmarks on.
- Intermediate turns tips off and leaves the act list and landmarks on.
- Advanced turns tips and the act list off. Gems from the build stay on. Landmarks stay on.

Those switches can be changed later. The configuration window opens from the top-center hotspot while Path of Exile is focused, and from the open-config hotkey (Ctrl+Shift+O by default).

## Hotkeys

| Action | Default |
| --- | --- |
| Toggle the overlay | Ctrl+Shift+H |
| Open configuration | Ctrl+Shift+O |
| Paste the vendor regex | Ctrl+Shift+V |
| Clear a task | Ctrl+Alt+click |

On Hyprland the paste hotkey copies the regex and sends Ctrl+A, Ctrl+V to the focused game window. Global shortcuts also go through the XDG global shortcuts portal. Ctrl and Alt for the click-to-clear chord are read from the keyboard device without grabbing it, so Path of Exile still receives the keys. That needs your user in the `input` group. `installLinux.sh` offers to add you. Log out and back in afterward. If the devices cannot be opened, the overlay prints a note and the portal hotkeys still work.

## Credits

Thank you to [Grinding Gear Games](https://www.grindinggear.com/) for making [Path of Exile](https://www.pathofexile.com/). RustyExile is a fan tool. It is not affiliated with or endorsed by Grinding Gear Games.

These projects were the inspiration for how the overlay follows the campaign:

- [Exile-UI](https://github.com/Lailloken/Exile-UI), for a campaign overlay that advances as you enter each zone.
- [poe-leveling](https://poe-leveling.com/), for the leveling notes this route is built around.
- [Path of Exile Wiki](https://www.poewiki.net/), for the campaign reference, including the [landmarks guide](https://www.poewiki.net/wiki/Guide:Campaign_landmarks).
- [Path of Building Community](https://github.com/PathOfBuildingCommunity/PathOfBuilding), the community fork, for the passive-tree imports.

Asset licenses and where the sprites and regex presets come from are listed in `NOTICE`.
