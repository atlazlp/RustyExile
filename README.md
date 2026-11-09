# poe-campaign

A Path of Exile 1 campaign overlay for Hyprland and Windows. It follows the game window, stays click-through, and turns act and gem steps on zone entry the way ExileUI does.

This is a non-commercial fan tool. It is not for sale and is not endorsed by Grinding Gear Games. See `NOTICE` for the route data, gem icons, and regex presets.

The overlay does not read game memory and does not inject anything. It tails `logs/Client.txt`.

## What it shows

Zone entry shows the step or gem whose start zone you just entered, and hides it when the next route zone is entered. Ctrl+Alt+click clears a line whose end zone has not been entered yet. The game log does not record quest turn-ins, gem pickups, or vendor buys.

Passive-tree specs imported from Path of Building are stored and are not drawn.

## First run

The first window asks you to start as Beginner, Intermediate, or Advanced.

- Beginner keeps zone tips and the act list on.
- Intermediate turns tips off and leaves the act list on.
- Advanced turns tips, the act list, and gems off.

Those switches can be changed later. The configuration window opens from the top-center hotspot while Path of Exile is focused, and from the open-config hotkey (Ctrl+Shift+O by default).

## Hotkeys

| Action | Default |
| --- | --- |
| Toggle the overlay | Ctrl+Shift+H |
| Open configuration | Ctrl+Shift+O |
| Paste the vendor regex | Ctrl+Shift+V |
| Clear a task | Ctrl+Alt+click |

On Hyprland the paste hotkey copies the regex and sends Ctrl+A, Ctrl+V to the focused game window. Global shortcuts also go through the XDG global shortcuts portal. Ctrl and Alt for the click-to-clear chord are read from the keyboard device without grabbing it, so Path of Exile still receives the keys. That needs your user in the `input` group:

```
sudo usermod -aG input "$USER"
```

Log out and back in after that. If the devices cannot be opened, the overlay prints a note and the portal hotkeys still work.

## Windows checklist

There is no Windows machine in this workspace. `cargo check --target x86_64-pc-windows-gnu` covers the build. On a Windows PC, confirm:

- The panel sits in the chosen corner of the Path of Exile window and moves, resizes, and minimizes with it.
- The panel does not stay above other applications when the game is not focused.
- Clicks pass through the panel. Ctrl+Alt+click clears the row under the cursor.
- The config button appears at the top center when the cursor is there, and that click opens the configuration window.
- Ctrl+Shift+V pastes the vendor regex into the vendor search box.

## Build

```
cargo test --workspace
cargo run -p poe-campaign
```

Linux needs `pkg-config`, wayland, xkbcommon, and a GL library for the configuration window.
