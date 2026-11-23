#!/bin/sh
# Instala a regra de USB do OpenPod Tune (uma vez so). Pede a senha do sudo.
set -eu
cd "$(dirname "$0")"
sudo cp 70-openpod.rules /etc/udev/rules.d/70-openpod.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
echo "Pronto. Desconecte e conecte o player de novo."
