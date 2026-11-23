# Créditos

O Instalador OpenPod é GPL-2.0-or-later. Ele fica de pé sobre o trabalho de
outras pessoas; os arquivos que adaptam código de fora dizem isso no
cabeçalho.

## De onde veio

- **rockslop** por Jayden Milne (GPL-2.0-or-later)
  A camada do aparelho: o protocolo USB da ROM do SL6801, o payload de
  serviço, a cópia da flash, a gravação por setor, o cartão microSD e o
  simulador. Copiada para `src/aparelho/` e adaptada ao OpenPod.
  Dela veio também o SCSI pass-through do Windows (`src/aparelho/spt.rs`),
  que deixa o instalador falar com o player pelo driver de disco do
  próprio Windows, sem WinUSB e sem Zadig.
- **smartlink_flash** por Ilya Kurdyukov <https://github.com/ilyakurdyukov/smartlink_flash>
  O protocolo da ROM SmartLink SL6801 (USB `301a:2800`, mass storage BOT
  com CDBs de fabricante `0xC0 | cmd`). O `src/aparelho/yp3.rs` segue esse
  dumper; os comandos de flash da própria ROM não são usados.
- **rockbox-yp3** por Jayden Milne
  O payload `payloads/yp3/svc.bin` (adaptado do `yp3dev`) que grava setores
  e recusa qualquer coisa fora da faixa segura.
- **Inter** por Rasmus Andersson (SIL Open Font License 1.1)
  A fonte da interface (`assets/Inter.ttf`); o aviso vai em
  `assets/Inter-OFL.txt` e dentro do `.app`.
- **Rockbox** (GPL-2.0-or-later) <https://www.rockbox.org>
  O firmware que o OpenPod RockBox instala; o tema OpenPod128
  (`assets/OpenPod128.zip`) é feito para ele.
- **OpenPod** por Jayden Milne e colaboradores
  A marca, o logo, o modelo visual (`docs/openpod/ideia_instalador.png`) e
  o kit de teste `.up`.
- **ed25519-dalek** por isis agora lovecruft e Henry de Valence (BSD-3-Clause)
  A verificação da assinatura Ed25519 dos pacotes, com a chave pública do
  OpenPod embutida.
- **cryptography** (Python) por The Python Cryptographic Authority
  Usada só pelo gerador `pacotes/gera_pacotes.py` para assinar os pacotes
  com a chave secreta que fica fora do repositório.

## Licença

Este programa é software livre: você pode redistribuí-lo e/ou modificá-lo
sob os termos da GNU General Public License, versão 2 ou (a seu critério)
qualquer versão posterior. Veja <https://www.gnu.org/licenses/>.
