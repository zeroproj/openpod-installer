//! A StorePod: a lojinha de temas, apps e jogos para o OpenPod RockBox.
//!
//! O catálogo fica em `https://zeroproj.github.io/OpenPod/storepod/index.json`:
//!
//! ```json
//! {"formato":1,"itens":[{"id":"tema-exemplo","tipo":"tema",
//!   "nome":{"pt":"…","en":"…"},"descricao":{"pt":"…","en":"…"},
//!   "versao":"1.0","tamanho":12345,
//!   "url":"https://…/storepod/<id>-<versao>.tar.gz","sha256":"…",
//!   "print":"https://…/storepod/<id>.png","requer":"rockbox"}]}
//! ```
//!
//! O pacote de um item é um `.tar.gz` assinado com a **mesma chave Ed25519**
//! dos pacotes do instalador (a secreta fica fora do repositório):
//!
//! ```text
//! MANIFEST      formato=1, linha=storepod, id=, versao=, tipo=
//! SHA256SUMS    sha256sum de cada arquivo abaixo
//! ASSINATURA    Ed25519 (64 B em hexadecimal) de MANIFEST+SHA256SUMS
//! card/.rockbox/...   só caminhos dentro de `.rockbox/`
//! ```
//!
//! Sem assinatura válida o item é recusado.
//!
//! No aparelho: o player **ligado no OpenPod RockBox** e conectado no USB
//! (aparecendo como disco). O modo de recuperação não é usado e a flash não
//! é tocada. O que fica instalado é registrado no cartão, em
//! `.rockbox/storepod/instalados.json`, no formato
//! `{ "<id>": { "versao": "…", "arquivos": ["…"] } }`.

#![allow(dead_code)]

use crate::err;
use crate::pacote::{abrir_tar, conferir_assinatura, hex, ler_tudo, nome_entrada, sha256, unhex32};
use crate::textos::Idioma;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// O catálogo da StorePod.
pub const URL_CATALOGO: &str = "https://zeroproj.github.io/OpenPod/storepod/index.json";
/// A pasta do RockBox no cartão.
pub const PASTA_ROCKBOX: &str = ".rockbox";
/// Onde o registro do que está instalado fica no cartão.
pub const ARQUIVO_INSTALADOS: &str = ".rockbox/storepod/instalados.json";
/// O formato do catálogo (e do pacote do item) que este instalador lê.
pub const FORMATO: u32 = 1;

/// O texto de um item, nos dois idiomas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Texto {
    #[serde(default)]
    pub pt: String,
    #[serde(default)]
    pub en: String,
}

impl Texto {
    /// O texto no idioma pedido.
    pub fn em(&self, idioma: Idioma) -> &str {
        match idioma {
            Idioma::Pt => &self.pt,
            Idioma::En => &self.en,
        }
    }
}

/// Um item do catálogo: um tema, um app ou um jogo para o RockBox.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub nome: Texto,
    #[serde(default)]
    pub descricao: Texto,
    #[serde(default)]
    pub versao: String,
    #[serde(default)]
    pub tamanho: u64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    /// A imagem do item (o print), se houver.
    #[serde(default)]
    pub print: Option<String>,
    /// O que o item exige ("rockbox").
    #[serde(default)]
    pub requer: Option<String>,
    /// Item do sistema (ex.: o tema OpenPod, padrão do OpenPod RockBox): instala e atualiza, mas
    /// não se remove pela loja, senão o Rockbox fica sem o tema escolhido.
    #[serde(default)]
    pub sistema: bool,
}

impl Item {
    /// O item pode ser instalado aqui? Hoje só no RockBox.
    pub fn pode(&self) -> bool {
        match self.requer.as_deref().map(str::trim) {
            None | Some("") | Some("rockbox") => true,
            _ => false,
        }
    }
}

/// O catálogo inteiro.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalogo {
    #[serde(default)]
    pub formato: u32,
    #[serde(default)]
    pub itens: Vec<Item>,
}

/// O que a tela precisa saber do catálogo.
#[derive(Clone, Debug, PartialEq)]
pub enum Estado {
    /// ainda procurando o catálogo
    Procurando,
    /// sem conexão com a StorePod (ou o catálogo ainda não foi publicado)
    SemConexao,
    /// o catálogo veio (pode estar vazio)
    Itens(Vec<Item>),
}

impl Estado {
    /// O catálogo tem itens?
    pub fn tem_itens(&self) -> bool {
        matches!(self, Estado::Itens(v) if !v.is_empty())
    }
}

/// Lê o texto do catálogo. `None` = o site não respondeu.
pub fn estado(texto: Option<&str>) -> Estado {
    let Some(t) = texto else { return Estado::SemConexao };
    match serde_json::from_str::<Catalogo>(t) {
        Ok(c) => Estado::Itens(c.itens),
        Err(e) => {
            crate::log::linha("STORE", &format!("o catálogo da StorePod não faz sentido: {e}"));
            Estado::SemConexao
        }
    }
}

/// Busca o catálogo da StorePod. `None` = sem conexão.
pub fn buscar_catalogo() -> Option<String> {
    let mut res = match crate::site::agente().get(URL_CATALOGO).call() {
        Ok(r) => r,
        Err(e) => {
            crate::log::linha("STORE", &format!("sem conexão com a StorePod: {e}"));
            return None;
        }
    };
    if res.status().as_u16() != 200 {
        crate::log::linha("STORE", &format!("a StorePod respondeu {}", res.status()));
        return None;
    }
    match res.body_mut().with_config().limit(4 << 20).read_to_string() {
        Ok(t) => Some(t),
        Err(e) => {
            crate::log::linha("STORE", &format!("não consegui ler o catálogo da StorePod: {e}"));
            None
        }
    }
}

/// Baixa a imagem do item (o print). `None` se não deu.
pub fn baixar_print(url: &str) -> Option<Vec<u8>> {
    let mut res = crate::site::agente().get(url).call().ok()?;
    if res.status().as_u16() != 200 {
        return None;
    }
    res.body_mut().with_config().limit(8 << 20).read_to_vec().ok()
}

/// A pasta onde os pacotes da StorePod são guardados.
pub fn pasta_pacotes() -> PathBuf {
    crate::site::pasta_pacotes().join("storepod")
}

/// Baixa o pacote de um item e confere o sha256 do catálogo.
pub fn baixar(item: &Item) -> Result<PathBuf, String> {
    let quer = unhex32(item.sha256.trim())
        .ok_or_else(|| err!(E141, "o catálogo da StorePod tem um sha256 ruim para {}", item.id))?;
    let dir = pasta_pacotes();
    std::fs::create_dir_all(&dir).map_err(|e| err!(E142, "pasta {}: {e}", dir.display()))?;
    let nome = item
        .url
        .rsplit('/')
        .next()
        .filter(|n| n.ends_with(".tar.gz"))
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-{}.tar.gz", item.id, item.versao));
    let destino = dir.join(&nome);
    let mut res = crate::site::agente().get(&item.url).call().map_err(|e| err!(E143, "baixar {}: {e}", item.url))?;
    if res.status().as_u16() != 200 {
        return Err(err!(E144, "o site respondeu {} para {}", res.status(), item.url));
    }
    let dados = res
        .body_mut()
        .with_config()
        .limit(128 << 20)
        .read_to_vec()
        .map_err(|e| err!(E145, "ler {}: {e}", item.url))?;
    if sha256(&dados) != quer {
        return Err(err!(
            E146,
            "o pacote {} está corrompido (sha256 {}…, esperado {}…)",
            item.id,
            &hex(&sha256(&dados))[..8],
            &item.sha256[..8.min(item.sha256.len())]
        ));
    }
    let tmp = dir.join(format!(".{nome}.part"));
    std::fs::write(&tmp, &dados).map_err(|e| err!(E147, "gravar {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &destino).map_err(|e| err!(E148, "guardar {}: {e}", destino.display()))?;
    Ok(destino)
}

/// O pacote de um item da StorePod: assinado como os pacotes do instalador,
/// mas sem flash: só arquivos do `.rockbox/`.
#[derive(Clone, Debug)]
pub struct Pacote {
    pub id: String,
    pub versao: String,
    pub tipo: String,
    /// os arquivos, com o caminho relativo à raiz do cartão (`.rockbox/...`)
    pub arquivos: Vec<(String, Vec<u8>)>,
}

/// O caminho (no cartão) tem de ser seguro e estar dentro do `.rockbox`.
fn dentro_do_rockbox(nome: &str) -> bool {
    let Some(rel) = nome.strip_prefix("card/") else { return false };
    crate::pacote::caminho_seguro(rel) && rel.starts_with(".rockbox/")
}

/// As linhas `chave=valor` do MANIFEST.
fn meta(texto: &str) -> Vec<(String, String)> {
    texto
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

fn valor<'a>(meta: &'a [(String, String)], chave: &str) -> Option<&'a str> {
    meta.iter().find(|(k, _)| k == chave).map(|(_, v)| v.as_str())
}

impl Pacote {
    /// Abre o pacote de um item, conferindo a assinatura com a chave do
    /// OpenPod.
    pub fn abrir(caminho: &Path) -> Result<Pacote, String> {
        Self::abrir_com_chave(caminho, &crate::pacote::chave_publica())
    }

    /// Como `abrir`, mas com uma chave dada (os testes geram a sua).
    pub fn abrir_com_chave(caminho: &Path, chave: &[u8; 32]) -> Result<Pacote, String> {
        let mut a = abrir_tar(caminho)?;
        let mut manifest = None;
        let mut sums_text = None;
        let mut assinatura = None;
        let nao_tar = |e: std::io::Error| err!(E149, "não é um pacote .tar.gz: {e}");
        for e in a.entries().map_err(nao_tar)? {
            let e = e.map_err(nao_tar)?;
            match nome_entrada(&e)?.as_str() {
                "MANIFEST" => manifest = Some(ler_tudo(e, 1 << 20)?),
                "SHA256SUMS" => sums_text = Some(ler_tudo(e, 16 << 20)?),
                "ASSINATURA" => assinatura = Some(ler_tudo(e, 1 << 20)?),
                _ => break,
            }
            if manifest.is_some() && sums_text.is_some() && assinatura.is_some() {
                break;
            }
        }
        let manifest = manifest.ok_or_else(|| err!(E150, "o pacote do item não começa com o MANIFEST"))?;
        let sums_text = sums_text.ok_or_else(|| err!(E151, "o pacote do item não tem o SHA256SUMS depois do MANIFEST"))?;
        let assinatura = assinatura.ok_or_else(|| crate::pacote::ERRO_ASSINATURA.to_string())?;
        if !conferir_assinatura(chave, &manifest, &sums_text, &assinatura) {
            return Err(crate::pacote::ERRO_ASSINATURA.to_string());
        }

        let texto = String::from_utf8(manifest).map_err(|_| err!(E152, "o MANIFEST do item não é UTF-8"))?;
        let meta = meta(&texto);
        match valor(&meta, "formato").map(|f| f.parse::<u32>()) {
            Some(Ok(f)) if f == FORMATO => {}
            Some(Ok(f)) if f > FORMATO => return Err(err!(E153, "o item (formato {f}) precisa de um instalador mais novo")),
            Some(Ok(_)) => return Err(err!(E157, "o item é velho demais para este instalador")),
            _ => return Err(err!(E155, "o MANIFEST do item não diz o formato")),
        }
        match valor(&meta, "linha") {
            Some("storepod") => {}
            Some(outro) => return Err(err!(E158, "o MANIFEST do item tem uma linha desconhecida: {outro}")),
            None => return Err(err!(E159, "o MANIFEST do item não diz a linha (storepod)")),
        }
        let id = valor(&meta, "id").unwrap_or_default().to_string();
        let versao = valor(&meta, "versao").unwrap_or_default().to_string();
        let tipo = valor(&meta, "tipo").unwrap_or_default().to_string();
        if id.is_empty() || versao.is_empty() {
            return Err(err!(E160, "o MANIFEST do item não diz o id ou a versão"));
        }

        let texto_sums = String::from_utf8(sums_text).map_err(|_| err!(E161, "o SHA256SUMS do item não é UTF-8"))?;
        let mut sums = std::collections::HashMap::new();
        for linha in texto_sums.lines().filter(|l| !l.trim().is_empty()) {
            let (h, nome) = linha.split_once(' ').ok_or_else(|| err!(E162, "linha ruim no SHA256SUMS do item"))?;
            let nome = nome.trim_start_matches([' ', '*']).trim_start_matches("./");
            sums.insert(nome.to_string(), unhex32(h).ok_or_else(|| err!(E163, "hash ruim no SHA256SUMS do item"))?);
        }

        // os arquivos do item: só dentro do `.rockbox/`
        let mut a = abrir_tar(caminho)?;
        let mut arquivos: Vec<(String, Vec<u8>)> = Vec::new();
        let mut vistos = 0usize;
        let err_arq = |e: std::io::Error| err!(E164, "pacote do item: {e}");
        for e in a.entries().map_err(err_arq)? {
            let e = e.map_err(err_arq)?;
            let nome = nome_entrada(&e)?;
            match nome.as_str() {
                "MANIFEST" | "SHA256SUMS" | "ASSINATURA" => continue,
                _ => {}
            }
            let tipo_entrada = e.header().entry_type();
            if !dentro_do_rockbox(&nome) {
                return Err(err!(E165, "{nome}: o item só pode ter arquivos dentro de .rockbox/"));
            }
            if tipo_entrada.is_dir() {
                continue;
            }
            if !tipo_entrada.is_file() {
                return Err(err!(E166, "{nome}: links e arquivos especiais não são permitidos"));
            }
            let quer = *sums.get(&nome).ok_or_else(|| crate::pacote::ERRO_ASSINATURA.to_string())?;
            let dados = ler_tudo(e, 128 << 20)?;
            if sha256(&dados) != quer {
                return Err(err!(E167, "{nome}: o checksum não bate, o download está corrompido"));
            }
            let rel = nome.strip_prefix("card/").unwrap_or_default().to_string();
            arquivos.push((rel, dados));
            vistos += 1;
        }
        if vistos != sums.len() {
            return Err(err!(E168, "faltam {} arquivo(s) listados no SHA256SUMS do item", sums.len() - vistos));
        }
        Ok(Pacote { id, versao, tipo, arquivos })
    }

    /// Os arquivos do item, com o caminho do cartão.
    pub fn caminhos(&self) -> Vec<String> {
        self.arquivos.iter().map(|(c, _)| c.clone()).collect()
    }
}

/// O que o cartão tem da StorePod: `{ "<id>": { "versao": "…", "arquivos": ["…"] } }`.
pub type Registro = BTreeMap<String, Instalado>;

/// Um item instalado no cartão.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Instalado {
    #[serde(default)]
    pub versao: String,
    /// os arquivos do item no cartão (`.rockbox/...`)
    #[serde(default)]
    pub arquivos: Vec<String>,
}

/// O caminho do registro no cartão.
pub fn caminho_registro(cartao: &Path) -> PathBuf {
    cartao.join(ARQUIVO_INSTALADOS)
}

/// Lê o registro do cartão. Sem registro: um registro vazio (não é erro).
pub fn ler_registro(cartao: &Path) -> Result<Registro, String> {
    let caminho = caminho_registro(cartao);
    let texto = match std::fs::read_to_string(&caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Registro::new()),
        Err(e) => return Err(err!(E556, "ler {}: {e}", caminho.display())),
    };
    if texto.trim().is_empty() {
        return Ok(Registro::new());
    }
    serde_json::from_str(&texto).map_err(|e| err!(E557, "o registro {} não faz sentido: {e}", caminho.display()))
}

/// Grava o registro no cartão.
pub fn gravar_registro(cartao: &Path, reg: &Registro) -> Result<(), String> {
    let caminho = caminho_registro(cartao);
    if let Some(dir) = caminho.parent() {
        std::fs::create_dir_all(dir).map_err(|e| err!(E558, "{}: {e}", dir.display()))?;
    }
    let texto = serde_json::to_string_pretty(reg).map_err(|e| err!(E559, "o registro não pôde ser escrito: {e}"))?;
    let mut dados = texto.into_bytes();
    dados.push(b'\n');
    std::fs::write(&caminho, &dados).map_err(|e| err!(E560, "gravar {}: {e}", caminho.display()))
}

/// O caminho do cartão de um arquivo do `.rockbox`: tem de ser seguro e
/// estar dentro do `.rockbox`.
fn caminho_do_cartao(cartao: &Path, rel: &str) -> Result<PathBuf, String> {
    if !crate::pacote::caminho_seguro(rel) || !rel.starts_with(".rockbox/") {
        return Err(err!(E550, "caminho fora do .rockbox: {rel}"));
    }
    Ok(cartao.join(rel))
}

/// Escreve um arquivo no cartão: o erro de E/S passageiro é repetido (até
/// três vezes), como na cópia do cartão do instalador.
fn escrever(destino: &Path, dados: &[u8]) -> Result<(), String> {
    let mut tentativa = 0;
    loop {
        match std::fs::write(destino, dados) {
            Ok(()) => return Ok(()),
            Err(e) if tentativa < 3 && e.raw_os_error() == Some(5) => {
                tentativa += 1;
                crate::log::linha("STORE", &format!("{}: {e}; tentando de novo ({tentativa}/3)", destino.display()));
                std::thread::sleep(std::time::Duration::from_millis(800));
            }
            Err(e) => return Err(err!(E551, "escrever {}: {e}", destino.display())),
        }
    }
}

/// Apaga um arquivo do cartão: o link é apagado (nunca o destino) e um
/// arquivo que já sumiu não é erro.
fn apagar(destino: &Path) -> Result<(), String> {
    let meta = match std::fs::symlink_metadata(destino) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(err!(E552, "ler {}: {e}", destino.display())),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        std::fs::remove_file(destino).map_err(|e| err!(E553, "apagar {}: {e}", destino.display()))
    } else {
        std::fs::remove_dir_all(destino).map_err(|e| err!(E554, "apagar {}: {e}", destino.display()))
    }
}

/// Instala os arquivos de um item no cartão e registra o item.
pub fn instalar(cartao: &Path, pacote: &Pacote, reg: &mut Registro) -> Result<(), String> {
    let mut arquivos = Vec::new();
    for (rel, dados) in &pacote.arquivos {
        let destino = caminho_do_cartao(cartao, rel)?;
        if let Some(dir) = destino.parent() {
            std::fs::create_dir_all(dir).map_err(|e| err!(E555, "{}: {e}", dir.display()))?;
        }
        escrever(&destino, dados)?;
        arquivos.push(rel.clone());
    }
    reg.insert(pacote.id.clone(), Instalado { versao: pacote.versao.clone(), arquivos });
    gravar_registro(cartao, reg)
}

/// Apaga **só** os arquivos registrados de um item e tira o item do
/// registro. Um arquivo que já sumiu não é erro.
pub fn remover(cartao: &Path, reg: &mut Registro, id: &str) -> Result<(), String> {
    let Some(instalado) = reg.get(id).cloned() else {
        return Ok(());
    };
    for rel in &instalado.arquivos {
        let destino = caminho_do_cartao(cartao, rel)?;
        apagar(&destino)?;
    }
    reg.remove(id);
    gravar_registro(cartao, reg)
}

/// O que o botão do item faz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Botao {
    Instalar,
    Remover,
    Atualizar,
    /// item do sistema já instalado na versão do catálogo: nada a fazer
    Instalado,
}

/// O botão do item, dado o que já está instalado no cartão: se a versão do
/// catálogo é diferente da registrada, o botão vira "Atualizar".
pub fn botao(item: &Item, reg: &Registro) -> Botao {
    match reg.get(&item.id) {
        None => Botao::Instalar,
        Some(i) if i.versao == item.versao => {
            if item.sistema {
                Botao::Instalado
            } else {
                Botao::Remover
            }
        }
        Some(_) => Botao::Atualizar,
    }
}

/// O que fazer com um item.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Acao {
    Instalar,
    Remover,
    Atualizar,
}

impl Acao {
    pub fn de(botao: Botao) -> Acao {
        match botao {
            Botao::Instalar => Acao::Instalar,
            Botao::Remover => Acao::Remover,
            Botao::Atualizar => Acao::Atualizar,
            // o botão fica desativado; se chegar aqui, reinstalar é inofensivo
            Botao::Instalado => Acao::Instalar,
        }
    }
}

/// Baixa o pacote do item, confere o sha256 do catálogo e a assinatura.
pub fn baixar_e_conferir(item: &Item) -> Result<Pacote, String> {
    let caminho = baixar(item)?;
    Pacote::abrir(&caminho)
}

/// O pacote baixado é o item do catálogo? O mesmo `id` e a mesma versão:
/// um pacote nosso de outro item não entra no lugar de outro.
pub fn conferir_item(pacote: &Pacote, item: &Item) -> Result<(), String> {
    if pacote.id != item.id || pacote.versao != item.versao {
        return Err(err!(
            E169,
            "o pacote é de outro item ({} {}; o catálogo diz {} {})",
            pacote.id,
            pacote.versao,
            item.id,
            item.versao
        ));
    }
    Ok(())
}

/// Aplica a ação no cartão: instala, remove ou atualiza o item.
pub fn aplicar(cartao: &Path, item: &Item, reg: &mut Registro, acao: Acao) -> Result<(), String> {
    match acao {
        Acao::Remover => remover(cartao, reg, &item.id),
        Acao::Instalar | Acao::Atualizar => {
            let pacote = baixar_e_conferir(item)?;
            conferir_item(&pacote, item)?;
            aplicar_pacote(cartao, &pacote, reg, acao)
        }
    }
}

/// Aplica a ação com o pacote já baixado e conferido. O "Atualizar" remove
/// o que era antigo antes de instalar o novo.
pub fn aplicar_pacote(cartao: &Path, pacote: &Pacote, reg: &mut Registro, acao: Acao) -> Result<(), String> {
    match acao {
        Acao::Remover => remover(cartao, reg, &pacote.id),
        Acao::Instalar => instalar(cartao, pacote, reg),
        Acao::Atualizar => {
            remover(cartao, reg, &pacote.id)?;
            instalar(cartao, pacote, reg)
        }
    }
}

/// O tamanho do jeito que o cliente lê ("1,2 MB" em português, "1.2 MB" em
/// inglês).
pub fn tamanho_humano(bytes: u64, idioma: Idioma) -> String {
    const UNIDADES: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0usize;
    while v >= 1000.0 && i + 1 < UNIDADES.len() {
        v /= 1000.0;
        i += 1;
    }
    let numero = if i > 0 && v < 10.0 { format!("{v:.1}") } else { format!("{v:.0}") };
    let separador = if idioma == Idioma::Pt { "," } else { "." };
    format!("{} {}", numero.replace('.', separador), UNIDADES[i])
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::pacote::{ASSINATURA, ERRO_ASSINATURA};
    use std::fs::File;

    fn dir_teste(nome: &str) -> PathBuf {
        let d = std::env::current_dir().unwrap().join(".tmp").join(format!("store-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn chave_teste() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[9u8; 32])
    }

    fn assinar(chave: &ed25519_dalek::SigningKey, manifesto: &[u8], sums: &[u8]) -> String {
        use ed25519_dalek::Signer;
        let mut msg = Vec::new();
        msg.extend_from_slice(manifesto);
        msg.extend_from_slice(sums);
        crate::pacote::hex(&chave.sign(&msg).to_bytes())
    }

    /// Um pacote de item: os membros dados, com o MANIFEST, o SHA256SUMS e a
    /// ASSINATURA certainos.
    fn pacote_item(caminho: &Path, membros: &[(&str, &[u8])], manifest: &[u8], assinatura: Option<String>) {
        let sk = chave_teste();
        let sum: String = membros.iter().map(|(n, d)| format!("{}  {n}\n", crate::pacote::hex(&sha256(d)))).collect();
        let ass = assinatura.unwrap_or_else(|| assinar(&sk, manifest, sum.as_bytes()));
        let f = File::create(caminho).unwrap();
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::fast()));
        for (nome, dados) in std::iter::once(("MANIFEST", manifest)).chain(std::iter::once(("SHA256SUMS", sum.as_bytes())))
            .chain(std::iter::once((ASSINATURA, ass.as_bytes())))
            .chain(membros.iter().copied())
        {
            let mut h = tar::Header::new_gnu();
            h.set_size(dados.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, nome, dados).unwrap();
        }
        b.finish().unwrap();
    }

    fn abrir_teste(caminho: &Path) -> Result<Pacote, String> {
        Pacote::abrir_com_chave(caminho, &chave_teste().verifying_key().to_bytes())
    }

    const MANIFEST: &[u8] = b"formato=1\nlinha=storepod\nid=tema-exemplo\nversao=1.0\ntipo=tema\n";

    /// O catálogo: leitura, vazio e sem conexão.
    #[test]
    fn o_catalogo_e_lido() {
        let texto = r#"{"formato":1,"itens":[{"id":"tema-exemplo","tipo":"tema",
            "nome":{"pt":"Tema de exemplo","en":"Example theme"},
            "descricao":{"pt":"Um tema","en":"A theme"},
            "versao":"1.0","tamanho":12345,
            "url":"https://exemplo/storepod/tema-exemplo-1.0.tar.gz",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "print":"https://exemplo/storepod/tema-exemplo.png","requer":"rockbox"}]}"#;
        let Estado::Itens(itens) = estado(Some(texto)) else {
            panic!("o catálogo não veio");
        };
        assert_eq!(itens.len(), 1);
        let i = &itens[0];
        assert_eq!(i.id, "tema-exemplo");
        assert_eq!(i.tipo, "tema");
        assert_eq!(i.versao, "1.0");
        assert_eq!(i.tamanho, 12345);
        assert_eq!(i.nome.em(Idioma::Pt), "Tema de exemplo");
        assert_eq!(i.nome.em(Idioma::En), "Example theme");
        assert_eq!(i.descricao.em(Idioma::Pt), "Um tema");
        assert_eq!(i.print.as_deref(), Some("https://exemplo/storepod/tema-exemplo.png"));
        assert!(i.pode(), "o item é para o RockBox");
    }

    /// O catálogo vazio: a tela diz que ainda não tem itens (não é erro).
    #[test]
    fn o_catalogo_vazio_e_vazio() {
        let Estado::Itens(itens) = estado(Some(r#"{"formato":1,"itens":[]}"#)) else {
            panic!("o catálogo não veio");
        };
        assert!(itens.is_empty());
        assert!(!estado(Some(r#"{"formato":1,"itens":[]}"#)).tem_itens());
        // o que não é JSON é como "sem conexão": sem erro feio na tela
        assert_eq!(estado(Some("isto não é json")), Estado::SemConexao);
    }

    /// Sem conexão: o site não respondeu.
    #[test]
    fn sem_conexao_nao_e_erro() {
        assert_eq!(estado(None), Estado::SemConexao);
        assert!(!estado(None).tem_itens());
    }

    /// O pacote do item: assinado certo, assinatura errada e caminho fora
    /// do `.rockbox`.
    #[test]
    fn o_item_assinado_abre() {
        let d = dir_teste("item");
        let p = d.join("tema-exemplo-1.0.tar.gz");
        pacote_item(
            &p,
            &[("card/.rockbox/themes/exemplo.cfg", b"wps: /.rockbox/wps/x.wps\n"), ("card/.rockbox/storepod/leia-me.txt", b"oi")],
            MANIFEST,
            None,
        );
        let pkg = abrir_teste(&p).unwrap();
        assert_eq!(pkg.id, "tema-exemplo");
        assert_eq!(pkg.versao, "1.0");
        assert_eq!(pkg.tipo, "tema");
        assert_eq!(pkg.caminhos(), vec![".rockbox/themes/exemplo.cfg", ".rockbox/storepod/leia-me.txt"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A assinatura trocada: o item é recusado.
    #[test]
    fn o_item_com_assinatura_trocada_e_recusado() {
        let d = dir_teste("item-ruim");
        let p = d.join("tema-exemplo-1.0.tar.gz");
        let sk = chave_teste();
        let membros: Vec<(&str, &[u8])> = vec![("card/.rockbox/themes/exemplo.cfg", b"x")];
        let sum: String = membros.iter().map(|(n, d)| format!("{}  {n}\n", crate::pacote::hex(&sha256(d)))).collect();
        let mut ass = assinar(&sk, MANIFEST, sum.as_bytes());
        let primeiro = if &ass[..1] == "a" { "b" } else { "a" };
        ass.replace_range(..1, primeiro);
        pacote_item(&p, &membros, MANIFEST, Some(ass));
        assert_eq!(abrir_teste(&p).unwrap_err(), ERRO_ASSINATURA);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Um arquivo fora do `.rockbox` (ou com um caminho perigoso) é recusado.
    #[test]
    fn o_item_com_arquivo_fora_do_rockbox_e_recusado() {
        let d = dir_teste("item-fora");
        for nome in ["card/musica.mp3", "card/.rockbox", "flash/0x0000e000.bin"] {
            let p = d.join("fora.tar.gz");
            pacote_item(&p, &[(nome, b"x")], MANIFEST, None);
            let e = abrir_teste(&p).unwrap_err();
            assert!(e.starts_with("E165"), "{nome}: {e}");
        }
        // um caminho perigoso é recusado antes de tocar em qualquer coisa
        assert!(!dentro_do_rockbox("card/.rockbox/../fora.txt"));
        assert!(!dentro_do_rockbox("card/.rockbox"));
        assert!(!dentro_do_rockbox("card/musica.mp3"));
        assert!(!dentro_do_rockbox("card/.rockbox//x"));
        assert!(dentro_do_rockbox("card/.rockbox/themes/x.cfg"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// O MANIFEST do item: a linha tem de ser `storepod`.
    #[test]
    fn o_manifesto_do_item_e_conferido() {
        let d = dir_teste("item-manifesto");
        let p = d.join("item.tar.gz");
        pacote_item(&p, &[("card/.rockbox/x.txt", b"x")], b"formato=1\nlinha=rockbox\nid=a\nversao=1\n", None);
        assert!(abrir_teste(&p).unwrap_err().starts_with("E158"));
        pacote_item(&p, &[("card/.rockbox/x.txt", b"x")], b"formato=1\nlinha=storepod\nid=a\n", None);
        assert!(abrir_teste(&p).unwrap_err().starts_with("E160"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// O registro: instalar, remover (só os arquivos registrados), atualizar
    /// e arquivo que já sumiu.
    #[test]
    fn o_registro_do_cartao() {
        let d = dir_teste("registro");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(&cartao).unwrap();
        // o arquivo do usuário, que não é do item: tem de sobreviver
        std::fs::create_dir_all(cartao.join(".rockbox/themes")).unwrap();
        std::fs::write(cartao.join(".rockbox/themes/meu.cfg"), b"meu").unwrap();

        // cartão sem registro: registro vazio, sem erro
        let mut reg = ler_registro(&cartao).unwrap();
        assert!(reg.is_empty());

        // instalar
        let p = d.join("tema.tar.gz");
        pacote_item(
            &p,
            &[("card/.rockbox/themes/exemplo.cfg", b"exemplo"), ("card/.rockbox/storepod/dados.txt", b"dados")],
            MANIFEST,
            None,
        );
        let pkg = abrir_teste(&p).unwrap();
        instalar(&cartao, &pkg, &mut reg).unwrap();
        assert!(cartao.join(".rockbox/themes/exemplo.cfg").is_file());
        assert!(cartao.join(".rockbox/storepod/dados.txt").is_file());
        let lido = ler_registro(&cartao).unwrap();
        let inst = &lido["tema-exemplo"];
        assert_eq!(inst.versao, "1.0");
        assert_eq!(inst.arquivos, vec![".rockbox/themes/exemplo.cfg", ".rockbox/storepod/dados.txt"]);

        // o botão: instalado (mesma versão) é remover
        let item = Item { id: "tema-exemplo".into(), versao: "1.0".into(), ..Default::default() };
        assert_eq!(botao(&item, &lido), Botao::Remover);
        // versão diferente: atualizar
        let novo = Item { id: "tema-exemplo".into(), versao: "2.0".into(), ..Default::default() };
        assert_eq!(botao(&novo, &lido), Botao::Atualizar);
        // item que não está instalado: instalar
        let outro = Item { id: "outro".into(), versao: "1.0".into(), ..Default::default() };
        assert_eq!(botao(&outro, &lido), Botao::Instalar);

        // remover: só os arquivos do item; o do usuário fica
        remover(&cartao, &mut reg, "tema-exemplo").unwrap();
        assert!(!cartao.join(".rockbox/themes/exemplo.cfg").exists());
        assert!(!cartao.join(".rockbox/storepod/dados.txt").exists());
        assert!(cartao.join(".rockbox/themes/meu.cfg").is_file(), "o arquivo do usuário sobrevive");
        assert!(ler_registro(&cartao).unwrap().is_empty());

        // um arquivo que já sumiu não é erro
        let mut reg2 = Registro::new();
        reg2.insert(
            "fantasma".into(),
            Instalado { versao: "1.0".into(), arquivos: vec![".rockbox/themes/nao-existe.cfg".into()] },
        );
        remover(&cartao, &mut reg2, "fantasma").unwrap();
        assert!(ler_registro(&cartao).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Atualizar: o que era antigo sai do cartão e o novo entra, e o
    /// registro passa a dizer a versão nova.
    #[test]
    fn o_registro_atualiza_o_item() {
        let d = dir_teste("atualizar");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(&cartao).unwrap();
        let p1 = d.join("v1.tar.gz");
        pacote_item(&p1, &[("card/.rockbox/themes/v1.cfg", b"um")], MANIFEST, None);
        let v1 = abrir_teste(&p1).unwrap();
        let mut reg = Registro::new();
        instalar(&cartao, &v1, &mut reg).unwrap();
        assert!(cartao.join(".rockbox/themes/v1.cfg").is_file());

        let p2 = d.join("v2.tar.gz");
        pacote_item(
            &p2,
            &[("card/.rockbox/themes/v2.cfg", b"dois")],
            b"formato=1\nlinha=storepod\nid=tema-exemplo\nversao=2.0\ntipo=tema\n",
            None,
        );
        let v2 = abrir_teste(&p2).unwrap();
        assert_eq!(v2.versao, "2.0");
        aplicar_pacote(&cartao, &v2, &mut reg, Acao::Atualizar).unwrap();
        // o arquivo antigo sumiu, o novo está no lugar
        assert!(!cartao.join(".rockbox/themes/v1.cfg").exists());
        assert!(cartao.join(".rockbox/themes/v2.cfg").is_file());
        let lido = ler_registro(&cartao).unwrap();
        assert_eq!(lido["tema-exemplo"].versao, "2.0");
        assert_eq!(lido["tema-exemplo"].arquivos, vec![".rockbox/themes/v2.cfg"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// O caminho registrado tem de ser seguro e dentro do `.rockbox`: um
    /// registro adulterado não apaga nada de fora.
    #[test]
    fn o_registro_adulterado_nao_apaga_fora() {
        let d = dir_teste("registro-ruim");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(cartao.join(".rockbox")).unwrap();
        std::fs::write(cartao.join("musica.mp3"), b"musica").unwrap();
        let mut reg = Registro::new();
        reg.insert(
            "mal".into(),
            Instalado { versao: "1.0".into(), arquivos: vec!["../musica.mp3".into(), "/etc/passwd".into()] },
        );
        assert!(remover(&cartao, &mut reg, "mal").is_err());
        assert!(cartao.join("musica.mp3").is_file(), "a música ficou");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// O pacote baixado tem de ser o item do catálogo (mesmo id e mesma
    /// versão): um pacote nosso de outro item não entra no lugar de outro.
    #[test]
    fn o_pacote_tem_de_ser_o_item_do_catalogo() {
        let d = dir_teste("conferir-item");
        let p = d.join("item.tar.gz");
        pacote_item(&p, &[("card/.rockbox/x.txt", b"x")], MANIFEST, None);
        let pkg = abrir_teste(&p).unwrap();
        let item = Item { id: "tema-exemplo".into(), versao: "1.0".into(), ..Default::default() };
        assert!(conferir_item(&pkg, &item).is_ok());
        // outro id
        let outro = Item { id: "outro".into(), versao: "1.0".into(), ..Default::default() };
        assert!(conferir_item(&pkg, &outro).unwrap_err().starts_with("E169"));
        // outra versão
        let velho = Item { id: "tema-exemplo".into(), versao: "2.0".into(), ..Default::default() };
        assert!(conferir_item(&pkg, &velho).unwrap_err().starts_with("E169"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// O tamanho do jeito que o cliente lê.
    #[test]
    fn o_tamanho_e_humano() {
        assert_eq!(tamanho_humano(0, Idioma::Pt), "0 B");
        assert_eq!(tamanho_humano(999, Idioma::En), "999 B");
        assert_eq!(tamanho_humano(1000, Idioma::En), "1.0 KB");
        assert_eq!(tamanho_humano(12345, Idioma::Pt), "12 KB");
        assert_eq!(tamanho_humano(1_500_000, Idioma::Pt), "1,5 MB");
        assert_eq!(tamanho_humano(1_500_000, Idioma::En), "1.5 MB");
    }

    /// O gerador da StorePod faz pacotes que o instalador lê (e o índice
    /// que ele escreve é o catálogo que a tela mostra).
    #[test]
    fn o_gerador_faz_pacotes_que_o_instalador_le() {
        if std::process::Command::new("python3").arg("--version").output().is_err() {
            return;
        }
        let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let d = raiz.join(".tmp").join(format!("gerador-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let gerador = raiz.join("pacotes/gera_storepod.py");
        // um par de chaves só para o teste; a chave de verdade fica fora do repositório
        let pem = d.join("teste-ed25519.pem");
        let pubhex = d.join("teste-ed25519.pub.hex");
        let criar_chave = format!(
            "from cryptography.hazmat.primitives import serialization as s\nfrom cryptography.hazmat.primitives.asymmetric import ed25519\nk=ed25519.Ed25519PrivateKey.generate()\nopen(r'{pem}','wb').write(k.private_bytes(s.Encoding.PEM,s.PrivateFormat.PKCS8,s.NoEncryption()))\nopen(r'{pub}','w').write(k.public_key().public_bytes(s.Encoding.Raw,s.PublicFormat.Raw).hex())\n",
            pem = pem.display(),
            pub = pubhex.display()
        );
        let c = std::process::Command::new("python3").arg("-c").arg(&criar_chave).output().unwrap();
        assert!(c.status.success(), "não gerei a chave de teste: {}", String::from_utf8_lossy(&c.stderr));
        let pubkey = unhex32(std::fs::read_to_string(&pubhex).unwrap().trim()).unwrap();

        // a pasta do item: só o .rockbox/ entra no pacote
        let item = d.join("meu-item");
        std::fs::create_dir_all(item.join(".rockbox/themes")).unwrap();
        std::fs::write(item.join(".rockbox/themes/meu.cfg"), b"wps: /.rockbox/wps/meu.wps\n").unwrap();
        let saida = d.join("saida");
        let pem_s = pem.to_string_lossy().to_string();
        let saida_s = saida.to_string_lossy().to_string();
        let item_s = item.to_string_lossy().to_string();
        let rodar = |args: &[&str], deve_ok: bool| {
            let s = std::process::Command::new("python3").arg(&gerador).args(args).current_dir(&raiz).output().expect("python3");
            assert_eq!(s.status.success(), deve_ok, "gerador: {}", String::from_utf8_lossy(&s.stderr));
            s
        };
        rodar(
            &[
                "--item", &item_s, "--id", "meu-tema", "--tipo", "tema", "--versao", "1.0",
                "--nome-pt", "Meu tema", "--nome-en", "My theme",
                "--desc-pt", "Um tema", "--desc-en", "A theme",
                "--chave", &pem_s, "--saida", &saida_s,
            ],
            true,
        );
        let ps = saida.join("meu-tema-1.0.tar.gz");
        let pkg = Pacote::abrir_com_chave(&ps, &pubkey).unwrap();
        assert_eq!(pkg.id, "meu-tema");
        assert_eq!(pkg.versao, "1.0");
        assert_eq!(pkg.tipo, "tema");
        assert_eq!(pkg.caminhos(), vec![".rockbox/themes/meu.cfg"]);

        // o índice que o gerador escreve é o catálogo que a tela lê
        let indice = std::fs::read_to_string(saida.join("index.json")).unwrap();
        let Estado::Itens(itens) = estado(Some(&indice)) else {
            panic!("o índice do gerador não é um catálogo");
        };
        assert_eq!(itens.len(), 1);
        assert_eq!(itens[0].id, "meu-tema");
        assert_eq!(itens[0].nome.em(Idioma::Pt), "Meu tema");
        assert_eq!(itens[0].nome.em(Idioma::En), "My theme");
        assert_eq!(itens[0].tipo, "tema");
        assert_eq!(itens[0].requer.as_deref(), Some("rockbox"));
        assert!(itens[0].tamanho > 0);
        assert_eq!(itens[0].sha256.len(), 64);

        // um arquivo fora do .rockbox/ é recusado
        std::fs::write(item.join("musica.mp3"), b"musica").unwrap();
        let s = rodar(
            &[
                "--item", &item_s, "--id", "meu-tema", "--tipo", "tema", "--versao", "2.0",
                "--nome-pt", "Meu tema", "--nome-en", "My theme",
                "--desc-pt", "Um tema", "--desc-en", "A theme",
                "--chave", &pem_s, "--saida", &saida_s,
            ],
            false,
        );
        let erro = String::from_utf8_lossy(&s.stderr);
        assert!(erro.contains(".rockbox"), "o gerador precisa dizer o que está fora: {erro}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod testes_sistema {
    use super::*;

    #[test]
    fn item_do_sistema_nao_vira_remover() {
        let mut item: Item = serde_json::from_str(r#"{"id":"tema-openpod","versao":"1.2","sistema":true}"#).unwrap();
        let mut reg = Registro::new();
        assert_eq!(botao(&item, &reg), Botao::Instalar);
        reg.insert("tema-openpod".to_string(), Instalado { versao: "1.2".to_string(), arquivos: vec![] });
        assert_eq!(botao(&item, &reg), Botao::Instalado);
        item.versao = "1.3".to_string();
        assert_eq!(botao(&item, &reg), Botao::Atualizar);
        item.sistema = false;
        item.versao = "1.2".to_string();
        assert_eq!(botao(&item, &reg), Botao::Remover);
    }
}
