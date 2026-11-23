# OpenPod Tune

Um programa só, em Rust, para colocar no player GN-438 (chip SL6801):

- **OpenPod System** — o sistema de fábrica melhorado (o nosso `.up`).
- **OpenPod RockBox** — o Rockbox YP3 com o tema OpenPod128 já escolhido.

A versão vem da mais nova do site (`https://zeroproj.github.io/OpenPod/`)
ou de um pacote `.tar.gz` que você escolhe. A interface segue o idioma do
sistema (português ou inglês) e tem um seletor **PT | EN** discreto no
canto, que troca na hora. Os textos ficam numa tabela única em
`instalador/src/textos.rs`.

No topo da janela tem duas abas: **Instalar** (o fluxo de cima) e
**StorePod** (a lojinha de temas, apps e jogos do OpenPod RockBox).

## Como compilar

```sh
. "$HOME/.cargo/env"
cd instalador
cargo build --release
```

O binário fica em `instalador/target/release/openpod-tune`.

Para o Windows (de qualquer sistema, com o Rust e o MinGW
`x86_64-w64-mingw32` instalados):

```sh
cd instalador
cargo build --release --target x86_64-pc-windows-gnu
```

O executável fica em
`instalador/target/x86_64-pc-windows-gnu/release/openpod-tune.exe`.

No macOS o instalador roda como **usuário comum**. Quando o macOS prende o
player (acesso negado, driver do kernel ativo ou aparelho ocupado), a
janela avisa que o macOS vai pedir a sua senha e chama o mesmo binário no
modo trabalhador:

```sh
osascript -e 'do shell script "<instalador> --trabalhador <socket> <token> <uid> <gid>" with administrator privileges'
```

O trabalhador é o mesmo programa sem janela: ele fala com o player e
devolve os eventos para a janela por um socket Unix dentro de
`~/Library/Caches/OpenPod/` (pasta `0700`, token de 32 bytes conferido na
conexão). A senha é pedida **uma vez por sessão**; se o cliente cancelar, a
tela diz "Sem a senha o instalador não consegue falar com o player" e
oferece "Tentar de novo".

Os arquivos que o trabalhador cria (as cópias de segurança em
`~/Documents/OpenPod/backups/` e os arquivos do cartão) são deixados com o
dono do cliente (`chown` para o `uid`/`gid` recebido): nunca fica arquivo
de root na pasta do usuário.

No Linux e no Windows nada muda: o aparelho é falado direto, sem pedido de
senha.

## No Windows

O cliente **não instala driver nenhum**. O `rusb` (libusb) só abre um
aparelho com o driver WinUSB (Zadig), e o instalador não usa esse caminho:
ele fala com o player pelo **driver de disco do próprio Windows**, com SCSI
pass-through (`src/aparelho/spt.rs`), como o rockslop faz. A ROM de download
aparece como um disco, o instalador acha esse disco pelo próprio Windows
(`CM_Get_Device_ID*`, `FindFirstVolumeW`) e manda os comandos do fabricante
para o volume. Nenhum Zadig, nenhum WinUSB, nada para instalar.

O `.exe` roda **como o usuário que o abriu** (o manifesto do Windows diz
`asInvoker`): o Windows deixa um usuário comum abrir o volume de um disco
removível para ler e escrever (o disco inteiro é que exigiria administrador).
É a mesma escolha do rockslop. Se o teste com o aparelho no Windows mostrar
"acesso negado", o conserto é uma linha no manifesto
(`instalador/packaging/windows/openpod.manifest`): `level="requireAdministrator"`.

No Windows a cópia de segurança fica em
`%USERPROFILE%\Documents\OpenPod\backups\` (no macOS e no Linux,
`~/Documents/OpenPod/backups/`), e a tela "Pronto" mostra o caminho
completo.

O cartão do RockBox no Windows é achado pela letra da unidade (`E:\`, o
RockBox se anuncia com o fabricante "Rockbox" e o produto "Internal
Storage"), o cache é derrubado travando e desmontando o volume, e o cartão é
ejetado no fim.

### O aviso do SmartScreen

O Windows não assina o `.exe`, então ele mostra o **Microsoft Defender
SmartScreen**: "O Windows protegeu o seu PC". Clique em **Mais
informações** e depois em **Executar assim mesmo** ("Run anyway"). É o
caminho normal para um programa que o Windows não reconheceu; o instalador
é o do OpenPod e roda igual.

### Empacotar para o Windows

```sh
cd instalador
./empacota-windows.sh
```

Gera `dist/openpod-tune-windows.exe`. O `.exe` já leva o ícone do
OpenPod, o nome "OpenPod Tune", a versão e o manifesto (o `build.rs`
monta o recurso e o `windres` do MinGW o compila).

## Testes

```sh
cd instalador
cargo test
```

Cobrem: leitura e validação dos dois pacotes; a assinatura Ed25519 (certa,
trocada, MANIFEST alterado, arquivo extra, arquivo alterado e sem
assinatura); `expect` certo e errado; o CRC16 do `.up` conferido contra o
kit real em `assets/openpod/`; a diferença de setores; a recusa de
endereço fora da faixa; o gerador de pacotes; e o fluxo completo no
simulador (que só existe nos testes).

Da StorePod: o catálogo (leitura, vazio e sem conexão); o pacote do item
(assinado certo, assinatura trocada e arquivo fora do `.rockbox/`); o
registro do cartão (instalar, remover — só os arquivos registrados, e o
arquivo do usuário sobrevive —, atualizar e arquivo que já sumiu); o
gerador de itens (que faz pacotes que o instalador lê); e o nome novo
("OpenPod Tune") nos dois idiomas.

Da permissão de administrador (macOS): o protocolo JSON de ida e volta
(mesmos `Comando`/`Evento` do aparelho); a recusa do token errado; a
troca do aparelho dentro do `Hub`; o caminho do `chown` (o arquivo fica do
usuário); o AppleScript gerado (o `osacompile` confere a sintaxe) e o
comando de shell com caminho com espaço (os argumentos chegam inteiros).

## Pacotes `.tar.gz`

```text
MANIFEST              formato=1, linha=system|rockbox, versao=, data=,
                      titulo=, expect=OFFSET SIZE SHA256 (uma por linha)
SHA256SUMS            sha256sum de cada arquivo abaixo
ASSINATURA            Ed25519 (64 B em hexadecimal) de MANIFEST+SHA256SUMS
update.up             (system)
flash/0x0000e000.bin  (rockbox) a imagem da flash
card/...              (rockbox) o cartão: Rockbox + tema + config.cfg
```

Todo pacote é assinado com Ed25519. O instalador embute a chave pública
(`assets/openpod/chave-publica-pacotes.hex`) e **recusa antes de conectar o
player** se faltar a `ASSINATURA`, se ela não conferir, ou se houver um
arquivo fora do `SHA256SUMS`: "Este arquivo não foi feito pelo OpenPod".
Não há como ignorar.

O gerador fica em `pacotes/gera_pacotes.py` e assina com a chave secreta
indicada por `--chave` (padrão `~/.openpod/chave-pacotes-ed25519.pem`,
usando a lib `cryptography`). **A chave secreta nunca entra no repositório,
em teste ou em log.**

```sh
./pacotes/gera_pacotes.py --system assets/openpod/kit-teste-3.5-diag.up --versao 3.5
./pacotes/gera_pacotes.py --rockbox rockbox-yp3-1.0.tar.xz \
  --tema assets/openpod/OpenPod128.zip --versao 1.0
```

Cada um gera o `.tar.gz` em `pacotes/saida/` e imprime o trecho do
`index.json` do site (ou grava com `--indice arquivo`). O índice é
`https://zeroproj.github.io/OpenPod/pacotes/index.json`, com
`{"system":{...},"rockbox":{...}}`. Enquanto ele não estiver publicado, o
instalador mostra "sem conexão com o site" e oferece "Escolher arquivo…".

## A StorePod (a lojinha)

A aba **StorePod** mostra o catálogo da lojinha
(`https://zeroproj.github.io/OpenPod/storepod/index.json`):

```json
{"formato":1,"itens":[{"id":"tema-exemplo","tipo":"tema|app|jogo",
  "nome":{"pt":"…","en":"…"},"descricao":{"pt":"…","en":"…"},
  "versao":"1.0","tamanho":12345,
  "url":"https://…/storepod/<id>-<versao>.tar.gz","sha256":"…",
  "print":"https://…/storepod/<id>.png","requer":"rockbox"}]}
```

Sem conexão (ou sem o catálogo publicado), a tela mostra "Sem conexão com a
StorePod"; com o catálogo vazio, "A StorePod ainda não tem itens".

### O que é preciso para instalar

O player **ligado no OpenPod RockBox** (não no modo de recuperação) e
conectado no cabo USB: ele aparece como um disco, e a tela mostra "Conecte
o player ligado no OpenPod RockBox" até achar o disco. A flash não é
tocada.

### O pacote de um item

É um `.tar.gz` assinado com a **mesma chave Ed25519** dos pacotes (a
secreta fica fora do repositório):

```text
MANIFEST      formato=1, linha=storepod, id=, versao=, tipo=
SHA256SUMS    sha256sum de cada arquivo abaixo
ASSINATURA    Ed25519 (64 B em hexadecimal) de MANIFEST+SHA256SUMS
card/.rockbox/...   só caminhos dentro de `.rockbox/`
```

Sem assinatura válida o item é recusado ("Este arquivo não foi feito pelo
OpenPod"). O sha256 do catálogo é conferido antes da assinatura.

### O registro do cartão

O que fica instalado é registrado no cartão, em
`.rockbox/storepod/instalados.json`:

```json
{"<id>": {"versao": "1.0", "arquivos": [".rockbox/themes/exemplo.cfg"]}}
```

**Remover** apaga só os arquivos registrados daquele item (com caminho
seguro dentro do `.rockbox`, sem seguir link simbólico; um arquivo que já
sumiu não é erro). **Atualizar** (o botão vira "Atualizar" quando a versão
do catálogo é diferente da registrada) remove o que era antigo e instala o
novo.

### O gerador dos itens

```sh
./pacotes/gera_storepod.py --item temas/meu-tema --id meu-tema --tipo tema \
  --versao 1.0 --nome-pt "Meu tema" --nome-en "My theme" \
  --desc-pt "Um tema" --desc-en "A theme" --print tema.png
```

A pasta do item tem que ter um `.rockbox/` dentro: só o que está nele vai
para o pacote, e um arquivo fora do `.rockbox/` é recusado. O gerador
assina com a chave de `--chave` (padrão
`~/.openpod/chave-pacotes-ed25519.pem`), grava o `<id>-<versao>.tar.gz` e
atualiza o `index.json` da StorePod na `--saida` (pelo `id`: a versão nova
substitui a antiga).

## A tela "Escolha o sistema"

Os dois cartões (OpenPod System e OpenPod RockBox) mostram o print da tela
do player: 120×150 px, com os cantos arredondados e uma borda fina, como
uma telinha. O cartão escolhido continua com a borda azul.

O cartão do RockBox avisa, em cinza e embaixo da descrição, que a capa do
álbum ainda está em beta ("* Capa do álbum: em beta" / "* Album art: in
beta").

A versão mostrada é a que vem no MANIFEST do pacote (`versao=1.0.822b9f5e99`
para o RockBox: a versão e a compilação do Rockbox).

## O idioma do player e o "começar do zero"

Com o **OpenPod RockBox** escolhido, aparece embaixo dos cartões o cartão
**"Idioma do player"**: a lista dos idiomas que o próprio pacote traz (os
arquivos `.lng` dele), cada um com o nome no seu próprio idioma. O padrão é
o idioma do instalador (PT → Português (Brasil), EN → English (UK)) e
acompanha a troca entre PT e EN.

Logo abaixo, a caixa **"Começar do zero: apagar as configurações e os dados
do Rockbox no cartão"**, desmarcada por padrão. Marcada, o instalador pede
confirmação ("Apagar e instalar" / "Cancelar") antes de instalar e apaga
**só** a pasta `.rockbox` do cartão: configurações, temas, plugins, jogos
salvos e favoritos. As músicas e os outros arquivos do cartão ficam como
estão, e link simbólico nunca é seguido (o link é apagado, nunca o
destino). Se o cartão não tiver a pasta `.rockbox`, não há o que apagar: o
instalador segue direto e a tela não diz "Começou do zero".

## O que ele faz, em ordem

1. Copia a flash inteira do player (sempre) para
   `~/Documents/OpenPod/backups/` (no Windows,
   `%USERPROFILE%\Documents\OpenPod\backups\`), com um `.sha256` ao lado.
2. Confere o bootloader (`0x0 + 0xD000`), a tabela (`0xD000 + 0x1C`) e as
   linhas `expect` do pacote. Se algo não bater, para sem gravar nada.
3. Grava só os setores que mudam, cada um conferido no aparelho.
4. Reinicia. No RockBox, copia os arquivos do cartão e relê para conferir.
   Com o "começar do zero" marcado, a pasta `.rockbox` do cartão é apagada
   antes da cópia. O pacote leva o modelo `.rockbox/config-openpod.cfg`; o
   instalador só cria `.rockbox/config.cfg` quando o cartão ainda não tem
   um, com a linha `lang:` no idioma escolhido. Se já existir, as
   configurações do usuário ficam como estão e a tela avisa "suas
   configurações foram mantidas".

**Nunca grava abaixo de `0xE000` nem a partir de `0x1FC000`.** O setor
`0xD000` (a tabela) nunca é tocado; o payload do aparelho também recusa.

Com bateria abaixo de 3,3 V o instalador recusa a gravação: carregue o
player antes.

Se a gravação, a conferência ou o reinício falhar, o instalador não mostra
"Pronto". A tela de erro mostra o caminho da cópia de segurança e um botão
**Restaurar o original** em destaque.

## Restaurar o original

O link "Restaurar o original" usa a cópia de segurança mais nova deste
computador e regrava só a parte gravável da flash. Só são aceitas cópias da
pasta do próprio instalador (`~/Documents/OpenPod/backups/`), com o
`.sha256` conferindo e a base até `0xE000` (bootloader e tabela) igual à do
player conectado. Uma cópia de outro modelo é recusada antes de gravar
qualquer setor. As cópias ficam em `~/Documents/OpenPod/backups/`.

## Empacotar para macOS

```sh
cd instalador
./empacota-macos.sh
```

Gera `dist/OpenPod Tune.app` (assinatura ad hoc) e
`dist/openpod-tune-macos.dmg`.
