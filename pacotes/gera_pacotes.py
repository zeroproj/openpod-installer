#!/usr/bin/env python3
"""Gera os pacotes .tar.gz do OpenPod e o trecho do index.json.

  ./pacotes/gera_pacotes.py --system assets/openpod/kit-teste-3.5-diag.up --versao 3.5
  ./pacotes/gera_pacotes.py --rockbox rockbox-yp3-1.0.tar.xz \
      --tema assets/openpod/OpenPod128.zip --versao 1.0

Saída: openpod-system-<v>.tar.gz e openpod-rockbox-<v>.tar.gz, cada um com
MANIFEST, SHA256SUMS e os arquivos. O trecho do índice vai para a saída
padrão (ou para --indice <arquivo>).
"""

import argparse
import datetime as dt
import hashlib
import io
import json
import struct
import sys
import tarfile
import zipfile
from pathlib import Path

BOOT_SHA256 = "47fe9ad6074d83b8d2a0619bdfb66a021bd94ba4e86abdfaeda574952ed038af"
TABELA_GN438 = "ca23eb0ca75fbaa89fd8fbabe086e5a4d1ad8e6750e9f273d96eae092b9e9983"
URL_BASE = "https://zeroproj.github.io/OpenPod/pacotes/"
UP_CABECALHO = 0x100


def sha256(dados: bytes) -> str:
    return hashlib.sha256(dados).hexdigest()


def crc16(dados: bytes) -> int:
    crc = 0xFFFF
    for b in dados:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def ler_up(caminho: Path) -> bytes:
    dados = caminho.read_bytes()
    if dados[:6] != b"CONFIG":
        raise SystemExit(f"{caminho}: não é um .up (sem CONFIG)")
    tamanho = struct.unpack_from("<I", dados, 0x10)[0]
    payload = dados[UP_CABECALHO:]
    if tamanho != len(payload):
        raise SystemExit(f"{caminho}: o cabeçalho diz {tamanho}, o arquivo tem {len(payload)}")
    quer = struct.unpack_from("<H", dados, 0x14)[0]
    tem = crc16(payload)
    if quer != tem:
        raise SystemExit(f"{caminho}: CRC16 {tem:04x} != {quer:04x}")
    return payload


def manifest(linha: str, versao: str, data: str, titulo: str) -> bytes:
    texto = (
        "formato=1\n"
        f"linha={linha}\n"
        f"versao={versao}\n"
        f"data={data}\n"
        f"titulo={titulo}\n"
        f"expect=0 53248 {BOOT_SHA256}\n"
        f"expect=53248 28 {TABELA_GN438}\n"
    )
    return texto.encode()


def assinar(manifesto: bytes, sums: bytes, chave: str) -> str:
    """A assinatura Ed25519 (hex) de MANIFEST+SHA256SUMS, com a chave dada."""
    from pathlib import Path as P

    try:
        from cryptography.hazmat.primitives import serialization
        from cryptography.hazmat.primitives.asymmetric import ed25519
    except Exception as e:  # noqa: BLE001
        raise SystemExit(f"preciso da lib cryptography para assinar: {e}")
    caminho = P(chave).expanduser()
    if not caminho.is_file():
        raise SystemExit(f"chave secreta não encontrada: {caminho}")
    priv = serialization.load_pem_private_key(caminho.read_bytes(), password=None)
    if not isinstance(priv, ed25519.Ed25519PrivateKey):
        raise SystemExit(f"{caminho}: não é uma chave Ed25519")
    return priv.sign(manifesto + sums).hex()


def montar(destino: Path, membros: list[tuple[str, bytes]], chave: str) -> str:
    """Escreve MANIFEST, SHA256SUMS, ASSINATURA e os membros num .tar.gz; devolve o sha256."""
    manifesto = dict(membros)
    if "MANIFEST" not in manifesto:
        raise SystemExit("faltou o MANIFEST")
    arquivos = [(n, d) for n, d in membros if n not in ("MANIFEST", "SHA256SUMS", "ASSINATURA")]
    sums = "".join(f"{sha256(d)}  {n}\n" for n, d in arquivos).encode()
    assinatura = assinar(manifesto["MANIFEST"], sums, chave).encode() + b"\n"
    ordem = [("MANIFEST", manifesto["MANIFEST"]), ("SHA256SUMS", sums), ("ASSINATURA", assinatura)] + arquivos
    destino.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(destino, "w:gz", format=tarfile.GNU_FORMAT) as tar:
        for nome, dados in ordem:
            info = tarfile.TarInfo(nome)
            info.size = len(dados)
            info.mode = 0o644
            info.mtime = int(dt.datetime.now().timestamp())
            tar.addfile(info, io.BytesIO(dados))
    return sha256(destino.read_bytes())


def gera_sistema(args) -> dict:
    payload = ler_up(Path(args.system))
    versao = args.versao
    data = args.data
    nome = f"openpod-system-{versao}.tar.gz"
    destino = Path(args.saida) / nome
    membros = [
        ("MANIFEST", manifest("system", versao, data, "OpenPod System")),
        ("update.up", Path(args.system).read_bytes()),
    ]
    soma = montar(destino, membros, args.chave)
    print(f"gerado {destino} ({destino.stat().st_size} bytes)", file=sys.stderr)
    print(f"payload do .up: {len(payload)} bytes, CRC16 ok", file=sys.stderr)
    return {"system": {"versao": versao, "data": data, "url": URL_BASE + nome, "sha256": soma}}


def nome_limpo(nome: str) -> str:
    """Tira só um `./` do começo; `.rockbox` tem de continuar `.rockbox`."""
    return nome[2:] if nome.startswith("./") else nome


def modelo_config(cfg_tema: bytes) -> bytes:
    """O modelo do config.cfg: as diretivas do tema, sem "theme:" e com o
    idioma padrão (o instalador troca pelo idioma escolhido)."""
    linhas = []
    for linha in cfg_tema.decode("utf-8").splitlines():
        chave = linha.split(":", 1)[0].strip().lower()
        if not linha.strip() or linha.lstrip().startswith("#"):
            continue
        if chave in ("theme", "lang"):
            continue
        linhas.append(linha.strip())
    if not linhas:
        raise SystemExit("o OpenPod128.cfg não tem diretivas de tema")
    texto = "\n".join(linhas) + "\n"
    return (texto + "lang: /.rockbox/langs/portugues-brasileiro.lng\n").encode()


def ler_tar_xz(caminho: Path) -> dict[str, bytes]:
    with tarfile.open(caminho, "r:xz") as tar:
        return {nome_limpo(m.name): tar.extractfile(m).read() for m in tar.getmembers() if m.isfile()}


TEMAS_ENXUTO = {"OpenPod128", "cabbiev2", "rockbox_failsafe", "rockbox_default_icons"}
SEM_PLUGINS = (".rockbox/rocks/games/", ".rockbox/rocks/apps/", ".rockbox/rocks/demos/")


def enxugar(cartao: dict) -> dict:
    """RockBox 1.2 (decisor, D41): so os temas OpenPod e os do proprio Rockbox, e sem jogos,
    apps e demos, que estavam instaveis. Ficam os codecs, os idiomas, as fontes e os
    visualizadores; o viewers.config perde as linhas dos plugins tirados."""
    tirar_temas = set()
    for nome in cartao:
        if nome.startswith(".rockbox/themes/") and nome.endswith(".cfg"):
            t = nome[len(".rockbox/themes/"):-4]
            if t not in TEMAS_ENXUTO:
                tirar_temas.add(t)
    out = {}
    for nome, dados in cartao.items():
        if nome.startswith(SEM_PLUGINS):
            continue
        base = nome.split("/")
        if len(base) >= 3 and base[1] in ("themes", "wps", "backdrops", "icons"):
            raiz = base[2].split(".")[0]
            if raiz in tirar_temas:
                continue
        out[nome] = dados
    vc = out.get(".rockbox/viewers.config")
    if vc is not None:
        linhas = [l for l in vc.decode("utf-8", "replace").splitlines()
                  if not any(f"/{d}/" in l for d in ("games", "apps", "demos"))]
        out[".rockbox/viewers.config"] = ("\n".join(linhas) + "\n").encode()
    print(f"enxuto: {len(cartao) - len(out)} arquivos fora; temas tirados: {len(tirar_temas)}")
    return out


def gera_rockbox(args) -> dict:
    entrada = ler_tar_xz(Path(args.rockbox))
    flash_nome = next((n for n in entrada if n.startswith("flash/") and n.endswith(".bin")), None)
    if flash_nome is None:
        raise SystemExit(f"{args.rockbox}: sem flash/0x0000e000.bin")
    flash = entrada[flash_nome]
    cartao: dict[str, bytes] = {}
    for nome, dados in entrada.items():
        if nome.startswith("card/"):
            cartao[nome[len("card/"):]] = dados
    # o tema OpenPod128 já vem com caminhos .rockbox/...
    with zipfile.ZipFile(args.tema) as z:
        for info in z.infolist():
            if info.is_dir():
                continue
            cartao[nome_limpo(info.filename)] = z.read(info)
    # O config.cfg do usuário nunca vai no pacote: o instalador cria um quando
    # o cartão ainda não tem (ou aplica só o tema num já existente), a partir
    # deste modelo. O modelo são as diretivas reais do OpenPod128.cfg: o
    # Rockbox não tem a opção "theme:", ela é ignorada ao carregar o config.
    cfg_tema = cartao.get(".rockbox/themes/OpenPod128.cfg")
    if cfg_tema is None:
        raise SystemExit(f"{args.tema}: sem .rockbox/themes/OpenPod128.cfg")
    cartao.pop(".rockbox/config.cfg", None)
    cartao[".rockbox/config-openpod.cfg"] = modelo_config(cfg_tema)
    if getattr(args, "enxuto", False):
        cartao = enxugar(cartao)
    versao = args.versao
    data = args.data
    nome = f"openpod-rockbox-{versao}.tar.gz"
    destino = Path(args.saida) / nome
    membros = [
        ("MANIFEST", manifest("rockbox", versao, data, "OpenPod RockBox")),
        (flash_nome, flash),
    ] + [(f"card/{n}", d) for n, d in sorted(cartao.items())]
    soma = montar(destino, membros, args.chave)
    print(f"gerado {destino} ({destino.stat().st_size} bytes)", file=sys.stderr)
    print(f"{len(cartao)} arquivos de cartão (com o tema OpenPod128)", file=sys.stderr)
    return {"rockbox": {"versao": versao, "data": data, "url": URL_BASE + nome, "sha256": soma}}


def main() -> int:
    p = argparse.ArgumentParser(description="Gera os pacotes .tar.gz do OpenPod.")
    p.add_argument("--system", help="o kit .up do OpenPod System")
    p.add_argument("--rockbox", help="o rockbox-yp3-*.tar.xz")
    p.add_argument("--enxuto", action="store_true", help="RockBox 1.2+: so os temas OpenPod/Rockbox, sem jogos, apps e demos")
    p.add_argument("--tema", default="assets/openpod/OpenPod128.zip", help="o tema OpenPod128 (zip)")
    p.add_argument("--versao", required=True)
    p.add_argument("--chave", default="~/.openpod/chave-pacotes-ed25519.pem", help="chave secreta Ed25519 (PEM)")
    p.add_argument("--data", default=dt.date.today().strftime("%d/%m/%Y"))
    p.add_argument("--saida", default="pacotes/saida")
    p.add_argument("--indice", help="arquivo onde escrever o trecho do index.json")
    args = p.parse_args()
    if bool(args.system) == bool(args.rockbox):
        p.error("escolha --system <kit.up> ou --rockbox <rockbox.tar.xz> (um só)")

    indice = gera_sistema(args) if args.system else gera_rockbox(args)
    texto = json.dumps(indice, ensure_ascii=False, indent=2) + "\n"
    if args.indice:
        Path(args.indice).write_text(texto, encoding="utf-8")
        print(f"índice em {args.indice}", file=sys.stderr)
    else:
        print(texto, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
