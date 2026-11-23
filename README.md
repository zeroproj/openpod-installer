# OpenPod Installer / Instalador OpenPod

Installs **OpenPod System** or **OpenPod RockBox** on players with the **Smartlink SL6801** chip (tested on the Ligenai GN-438),
on **macOS** and **Windows**, from a single executable. Download it at **<https://zeroproj.github.io/OpenPod/>**.

*Instala o **OpenPod System** ou o **OpenPod RockBox** em players com chip **Smartlink SL6801** (testado no Ligenai GN-438), no
**macOS** e no **Windows**, com um executável só. Baixe em <https://zeroproj.github.io/OpenPod/>.*

- Always makes a **full backup** of the flash before writing; never writes the bootloader or the partition table.
- Only accepts packages **signed by OpenPod** (Ed25519; the public key is in `assets/openpod/chave-publica-pacotes.hex`).
- macOS asks for your password (the player's download mode needs `sudo`); Windows needs no driver.

**Support / Suporte:** <https://github.com/zeroproj/OpenPod/issues>

## Source / Código

- `instalador/`: the installer (Rust + egui). Build: `cd instalador && cargo build --release`; packages: `empacota-macos.sh`,
  `empacota-windows.sh`.
- `pacotes/gera_pacotes.py`: builds the `.tar.gz` packages (signing needs OpenPod's private key, which is not here).
- The rest of the tree is **rockslop** by Jayden Milne (original README in `README-rockslop.md`), the base of the USB layer.

## License / Licença

**GPL-2.0-or-later**, like rockslop, which it derives from. Credits are in `instalador/CREDITOS.md`: rockslop (Jayden Milne),
smartlink_flash (Ilya Kurdyukov), Rockbox, the Inter font (SIL OFL 1.1). The OpenPod System firmware itself is not part of
this repository.
