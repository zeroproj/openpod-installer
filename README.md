# OpenPod Tune

Installs **OpenPod System** or **OpenPod RockBox** on players with the **Smartlink SL6801** chip (tested on the Ligenai GN-438),
on **macOS**, **Windows** and **Linux**, from a single program. It also has **StorePod**, a small store of tested themes, apps and
games for OpenPod RockBox. Download it at **<https://zeroproj.github.io/OpenPod/>**.

- It always makes a **full backup** of the flash before writing, and never writes the bootloader or the partition table.
- It only accepts packages **signed by OpenPod** (Ed25519; the public key is in `assets/openpod/chave-publica-pacotes.hex`).
- macOS asks for your password (the player's download mode needs `sudo`). Windows needs no driver. On Linux, a udev rule is
  installed once.

**Support:** <https://github.com/zeroproj/OpenPod/issues>

## Source

- `instalador/` holds the app (Rust + egui). Build it with `cd instalador && cargo build --release`, and package it with
  `empacota-macos.sh`, `empacota-windows.sh` or `empacota-linux.sh`.
- `pacotes/` holds the package and StorePod item generators. Signing needs OpenPod's private key, which is not here.
- The rest of the tree is **rockslop** by Jayden Milne, the base of the USB layer. Its original README is in `README-rockslop.md`.

## License

**GPL-2.0-or-later**, like rockslop. Credits are in `instalador/CREDITOS.md`: rockslop (Jayden Milne), smartlink_flash (Ilya
Kurdyukov), Rockbox, and the Inter font (SIL OFL 1.1).
