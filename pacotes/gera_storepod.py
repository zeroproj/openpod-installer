#!/usr/bin/env python3
"""Gera o pacote de um item da StorePod e atualiza o index.json.

  ./pacotes/gera_storepod.py --item temas/meu-tema --id meu-tema --tipo tema \
      --versao 1.0 --nome-pt "Meu tema" --nome-en "My theme" \
      --desc-pt "Um tema para o OpenPod" --desc-en "A theme for the OpenPod" \
      --print tema.png

A pasta do item (`--item`) tem que ter um `.rockbox/` dentro: só o que está
nesse `.rockbox/` vai para o pacote, e o que estiver fora é recusado.

O pacote é um `.tar.gz` assinado com a MESMA chave Ed25519 dos pacotes do
instalador (a secreta fica fora do repositório):

  MANIFEST      formato=1, linha=storepod, id=, versao=, tipo=
  SHA256SUMS    sha256sum de cada arquivo abaixo
  ASSINATURA    Ed25519 (64 B em hexadecimal) de MANIFEST+SHA256SUMS
  card/.rockbox/...   os arquivos do item

Saída: `<id>-<versao>.tar.gz` e o `index.json` da StorePod na pasta
`--saida` (o item com o mesmo `id` é substituído pela versão nova).
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gera_pacotes import montar  # noqa: E402  (o mesmo assinatura dos pacotes)

URL_BASE = "https://zeroproj.github.io/OpenPod/storepod/"
TIPOS = ("tema", "app", "jogo")
PASTA_ROCKBOX = ".rockbox"
# o barulho do macOS não vai no pacote
RUIDO = (".DS_Store",)


def nome_seguro(nome: str) -> bool:
    """Um id ou uma versão serve para um nome de arquivo e para uma URL?"""
    if not nome or nome.startswith(".") or len(nome) > 64:
        return False
    return all(c.isalnum() or c in "-_." for c in nome)


def arquivos_do_item(pasta: Path) -> list[tuple[str, bytes]]:
    """Os arquivos do item: os de dentro do `.rockbox/`, e só eles.

    Devolve [(caminho no tar, conteúdo)], com o caminho `card/.rockbox/...`.
    """
    if not pasta.is_dir():
        raise SystemExit(f"{pasta}: não é uma pasta")
    rockbox = pasta / PASTA_ROCKBOX
    if not rockbox.is_dir():
        raise SystemExit(f"{pasta}: sem a pasta {PASTA_ROCKBOX}/ (é onde os arquivos do item vão)")
    fora = []
    dentro = []
    for caminho in sorted(pasta.rglob("*")):
        if not caminho.is_file():
            continue
        rel = caminho.relative_to(pasta)
        partes = rel.parts
        if not partes or partes[0] != PASTA_ROCKBOX:
            if caminho.name in RUIDO or caminho.name.startswith("._"):
                continue
            fora.append(str(rel))
            continue
        if caminho.name in RUIDO or caminho.name.startswith("._"):
            continue
        dentro.append((rel.as_posix(), caminho.read_bytes()))
    if fora:
        raise SystemExit(
            f"{pasta}: arquivo fora do {PASTA_ROCKBOX}/: {', '.join(fora)} (a StorePod só aceita arquivos do .rockbox/)"
        )
    if not dentro:
        raise SystemExit(f"{pasta}: o {PASTA_ROCKBOX}/ está vazio")
    return [(f"card/{nome}", dados) for nome, dados in dentro]


def manifest(id: str, versao: str, tipo: str) -> bytes:
    texto = f"formato=1\nlinha=storepod\nid={id}\nversao={versao}\ntipo={tipo}\n"
    return texto.encode()


def le_indice(caminho: Path) -> dict:
    if not caminho.is_file():
        return {"formato": 1, "itens": []}
    try:
        dados = json.loads(caminho.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        raise SystemExit(f"{caminho}: o índice não faz sentido: {e}") from e
    if not isinstance(dados, dict):
        raise SystemExit(f"{caminho}: o índice não é um objeto")
    dados.setdefault("formato", 1)
    itens = dados.get("itens")
    if not isinstance(itens, list):
        dados["itens"] = []
    return dados


def main() -> int:
    p = argparse.ArgumentParser(description="Gera o pacote de um item da StorePod.")
    p.add_argument("--item", required=True, help="a pasta do item (com .rockbox/ dentro)")
    p.add_argument("--id", required=True, help="o id do item (o mesmo em todas as versões)")
    p.add_argument("--tipo", required=True, choices=TIPOS, help="tema, app ou jogo")
    p.add_argument("--versao", required=True, help="a versão do item")
    p.add_argument("--nome-pt", required=True, help="o nome em português")
    p.add_argument("--nome-en", required=True, help="o nome em inglês")
    p.add_argument("--desc-pt", required=True, help="a descrição curta em português")
    p.add_argument("--desc-en", required=True, help="a descrição curta em inglês")
    p.add_argument("--print", help="a imagem do item (png)")
    p.add_argument("--requer", default="rockbox", help="o que o item exige (padrão: rockbox)")
    p.add_argument("--chave", default="~/.openpod/chave-pacotes-ed25519.pem", help="chave secreta Ed25519 (PEM)")
    p.add_argument("--saida", default="pacotes/saida/storepod", help="onde o pacote e o índice são gravados")
    p.add_argument("--url-base", default=URL_BASE, help="o começo da URL dos itens")
    args = p.parse_args()

    if not nome_seguro(args.id):
        raise SystemExit(f"--id {args.id!r} não serve: use só letras, números, '-', '_' e '.'")
    if not nome_seguro(args.versao):
        raise SystemExit(f"--versao {args.versao!r} não serve: use só letras, números, '-', '_' e '.'")

    pasta_item = Path(args.item)
    saida = Path(args.saida)
    saida.mkdir(parents=True, exist_ok=True)

    membros = arquivos_do_item(pasta_item)
    nome_pacote = f"{args.id}-{args.versao}.tar.gz"
    destino = saida / nome_pacote
    todos = [("MANIFEST", manifest(args.id, args.versao, args.tipo))] + membros
    soma = montar(destino, todos, args.chave)
    tamanho = destino.stat().st_size

    print_item = None
    if args.print:
        origem = Path(args.print)
        if not origem.is_file():
            raise SystemExit(f"{origem}: não achei a imagem")
        destino_print = saida / f"{args.id}.png"
        destino_print.write_bytes(origem.read_bytes())
        print_item = f"{args.url_base}{args.id}.png"

    item = {
        "id": args.id,
        "tipo": args.tipo,
        "nome": {"pt": args.nome_pt, "en": args.nome_en},
        "descricao": {"pt": args.desc_pt, "en": args.desc_en},
        "versao": args.versao,
        "tamanho": tamanho,
        "url": f"{args.url_base}{nome_pacote}",
        "sha256": soma,
        "requer": args.requer,
    }
    if print_item:
        item["print"] = print_item

    indice = le_indice(saida / "index.json")
    itens = [i for i in indice["itens"] if not (isinstance(i, dict) and i.get("id") == args.id)]
    itens.append(item)
    indice["itens"] = sorted(itens, key=lambda i: i.get("id", ""))
    indice["formato"] = 1
    (saida / "index.json").write_text(json.dumps(indice, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"gerado {destino} ({tamanho} bytes, {len(membros)} arquivos)", file=sys.stderr)
    print(f"sha256 {soma}", file=sys.stderr)
    print(f"índice em {saida / 'index.json'}", file=sys.stderr)
    print(f"publicar em {args.url_base}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
