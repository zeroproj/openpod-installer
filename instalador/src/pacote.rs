//! Pacotes do OpenPod: um `.tar.gz` simples, que qualquer ferramenta de
//! arquivo abre.
//!
//! ```text
//! MANIFEST              linhas chave=valor, primeiro no arquivo
//! SHA256SUMS            formato sha256sum, uma linha por arquivo abaixo
//! ASSINATURA            Ed25519 (64 B em hexadecimal) de MANIFEST+SHA256SUMS
//! update.up             (linha=system) o `.up` do sistema
//! flash/0x0000e000.bin  (linha=rockbox) a imagem da flash no endereço do nome
//! card/.rockbox/...     (linha=rockbox) a árvore do cartão, copiada na raiz
//! ```
//!
//! Chaves do MANIFEST: `formato=1`, `linha=system|rockbox`, `versao`,
//! `data`, `titulo`, e quantas linhas `expect=OFFSET SIZE SHA256`: a flash
//! do aparelho precisa bater com elas antes de qualquer gravação.
//!
//! Só entra firmware assinado pela chave do OpenPod: a assinatura cobre o
//! MANIFEST e o SHA256SUMS, que por sua vez cobrem todos os outros arquivos.

#![allow(dead_code)]

use crate::err;
use ed25519_dalek::{Signature, VerifyingKey};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

/// O formato deste instalador lê.
pub const FORMATO: u32 = 1;
/// Cabeçalho do `.up` e o CRC16 do resto.
pub const UP_CABECALHO: usize = 0x100;
pub const UP_CRC16: std::ops::Range<usize> = 0x14..0x16;
pub const UP_TAMANHO: usize = 0x10;

/// O arquivo com a assinatura Ed25519, e a mensagem quando ela não confere.
pub const ASSINATURA: &str = "ASSINATURA";
pub const ERRO_ASSINATURA: &str = "Este arquivo não foi feito pelo OpenPod";
/// Onde os idiomas do RockBox moram no cartão do pacote.
pub const PASTA_LANG: &str = "card/.rockbox/langs/";
/// A extensão do arquivo de idioma do RockBox.
pub const EXT_LANG: &str = ".lng";
const CHAVE_PUBLICA: &str = include_str!("../../assets/openpod/chave-publica-pacotes.hex");

/// A chave pública do OpenPod, embutida no instalador.
pub fn chave_publica() -> [u8; 32] {
    unhex32(CHAVE_PUBLICA.trim()).expect("chave pública dos pacotes inválida")
}

fn assinatura_bytes(bytes: &[u8]) -> Option<[u8; 64]> {
    let s = std::str::from_utf8(bytes).ok()?.trim();
    if s.len() != 128 {
        return None;
    }
    let mut out = [0u8; 64];
    for i in 0..64 {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

fn conferir_assinatura(chave: &[u8; 32], manifesto: &[u8], sums: &[u8], assinatura: &[u8]) -> bool {
    let Ok(chave) = VerifyingKey::from_bytes(chave) else { return false };
    let Some(bytes) = assinatura_bytes(assinatura) else { return false };
    let mut mensagem = Vec::with_capacity(manifesto.len() + sums.len());
    mensagem.extend_from_slice(manifesto);
    mensagem.extend_from_slice(sums);
    chave.verify_strict(&mensagem, &Signature::from_bytes(&bytes)).is_ok()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tipo {
    System,
    Rockbox,
}

impl Tipo {
    pub fn chave(self) -> &'static str {
        match self {
            Tipo::System => "system",
            Tipo::Rockbox => "rockbox",
        }
    }
    pub fn nome(self) -> &'static str {
        match self {
            Tipo::System => "OpenPod System",
            Tipo::Rockbox => "OpenPod RockBox",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Expect {
    pub offset: u32,
    pub size: u32,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct Arquivo {
    /// relativo à raiz do cartão, separado por `/`
    pub caminho: String,
    pub tamanho: u64,
}

#[derive(Clone, Debug)]
pub struct Pacote {
    pub caminho: PathBuf,
    pub tipo: Tipo,
    pub versao: String,
    pub data: String,
    pub titulo: String,
    pub expect: Vec<Expect>,
    meta: Vec<(String, String)>,
    sums: HashMap<String, [u8; 32]>,
    /// Os bytes assinados, guardados para conferir de novo na verificação.
    manifesto: Vec<u8>,
    sums_bytes: Vec<u8>,
    assinatura: Vec<u8>,
    chave: [u8; 32],
    pub flash: Vec<(u32, u32)>,
    pub arquivos: Vec<Arquivo>,
    pub pastas: Vec<String>,
    total: u64,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn num(s: &str) -> Option<u64> {
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

fn erro_abrir(caminho: &Path, e: std::io::Error) -> String {
    err!(E101, "abrir {}: {e}", caminho.display())
}
fn erro_pacote(e: std::io::Error) -> String {
    err!(E103, "pacote: {e}")
}
fn caminho_inseguro(nome: &str) -> String {
    err!(E130, "caminho inseguro no pacote: {nome}")
}
fn nao_listado(_nome: &str) -> String {
    ERRO_ASSINATURA.to_string()
}
fn membro_inesperado(nome: &str) -> String {
    err!(E134, "membro inesperado no pacote: {nome}")
}
fn sem_imagem(texto: &str) -> String {
    err!(E136, "{texto}")
}
fn mudou(nome: &str) -> String {
    err!(E140, "{nome}: o checksum não bate")
}
fn sem_linha() -> String {
    err!(E115, "o MANIFEST não diz a linha (system ou rockbox)")
}

/// Um caminho de cartão em que podemos escrever: relativo, sem `..`, sem
/// bytes estranhos.
pub fn caminho_seguro(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && !p.chars().any(|c| c.is_control())
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// O nome de um arquivo de idioma serve? Tem de ser um nome simples (sem
/// caminho: o `lang:` do Rockbox é lido como um caminho) e terminar em
/// `.lng`.
pub fn nome_de_lang_ok(nome: &str) -> bool {
    !nome.is_empty()
        && nome.len() <= 64
        && nome.ends_with(EXT_LANG)
        && !nome.contains('/')
        && !nome.contains('\\')
        && !nome.contains(':')
        && !nome.starts_with('.')
        && !nome.chars().any(|c| c.is_control())
}

/// CRC16-CCITT (XMODEM), o do cabeçalho do `.up`.
pub fn crc16(dados: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &b in dados {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// O payload de um `.up` (a imagem desde o endereço 0), conferido: o
/// tamanho do cabeçalho e o CRC16.
pub fn ler_up(dados: &[u8]) -> Result<Vec<u8>, String> {
    if dados.len() < UP_CABECALHO + 4 {
        return Err(err!(E180, "o arquivo .up é curto demais"));
    }
    if &dados[..6] != b"CONFIG" {
        return Err(err!(E181, "o arquivo não é um .up (sem o cabeçalho CONFIG)"));
    }
    let tamanho = u32::from_le_bytes(dados[UP_TAMANHO..UP_TAMANHO + 4].try_into().unwrap()) as usize;
    let payload = &dados[UP_CABECALHO..];
    if tamanho != payload.len() {
        return Err(err!(E182, "o .up diz {} bytes de payload, mas tem {}", tamanho, payload.len()));
    }
    let quer = u16::from_le_bytes(dados[UP_CRC16.clone()].try_into().unwrap());
    let tem = crc16(payload);
    if quer != tem {
        return Err(err!(E183, "o .up está corrompido (CRC16 {tem:04x}, esperado {quer:04x})"));
    }
    Ok(payload.to_vec())
}

type ArquivoTar = tar::Archive<GzDecoder<BufReader<File>>>;

fn abrir_tar(caminho: &Path) -> Result<ArquivoTar, String> {
    let f = File::open(caminho).map_err(|e| erro_abrir(caminho, e))?;
    Ok(tar::Archive::new(GzDecoder::new(BufReader::with_capacity(1 << 16, f))))
}

fn nome_entrada(e: &tar::Entry<impl Read>) -> Result<String, String> {
    let bruto = e.path_bytes();
    let s = std::str::from_utf8(&bruto).map_err(|_| err!(E104, "o pacote tem um caminho que não é UTF-8"))?;
    let s = s.strip_prefix("./").unwrap_or(s);
    Ok(s.trim_end_matches('/').to_string())
}

fn ler_tudo(mut r: impl Read, limite: u64) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    r.by_ref().take(limite + 1).read_to_end(&mut v).map_err(erro_pacote)?;
    if v.len() as u64 > limite {
        return Err(err!(E105, "um arquivo do pacote é grande demais"));
    }
    Ok(v)
}

fn bombear(mut r: impl Read, mut fim: impl FnMut(&[u8]) -> Result<(), String>) -> Result<([u8; 32], u64), String> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut n = 0u64;
    loop {
        let k = r.read(&mut buf).map_err(erro_pacote)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        fim(&buf[..k])?;
        n += k as u64;
    }
    Ok((h.finalize().into(), n))
}

fn get<'a>(meta: &'a [(String, String)], chave: &str) -> Option<&'a str> {
    meta.iter().find(|(k, _)| k == chave).map(|(_, v)| v.as_str())
}

fn parse_manifest(texto: &str) -> Result<(Vec<(String, String)>, Vec<Expect>), String> {
    let (mut meta, mut expect) = (Vec::new(), Vec::new());
    for (n, linha) in texto.lines().enumerate() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with('#') {
            continue;
        }
        let ruim = || err!(E112, "MANIFEST linha {}: {linha:?}", n + 1);
        let (k, v) = linha.split_once('=').ok_or_else(ruim)?;
        let (k, v) = (k.trim(), v.trim());
        if k == "expect" {
            let f: Vec<&str> = v.split_whitespace().collect();
            if f.len() != 3 {
                return Err(ruim());
            }
            let offset = num(f[0]).ok_or_else(ruim)?;
            let size = num(f[1]).ok_or_else(ruim)?;
            if offset > u32::MAX as u64 || size > u32::MAX as u64 {
                return Err(ruim());
            }
            expect.push(Expect { offset: offset as u32, size: size as u32, sha256: unhex32(f[2]).ok_or_else(ruim)? });
        } else {
            meta.push((k.to_string(), v.to_string()));
        }
    }
    let formato = match get(&meta, "formato").map(|f| f.parse::<u32>()) {
        Some(Ok(f)) if f == FORMATO => f,
        Some(Ok(f)) if f > FORMATO => return Err(err!(E113, "o pacote (formato {f}) precisa de um instalador mais novo")),
        Some(Ok(f)) => return Err(err!(E116, "o pacote (formato {f}) é velho demais para este instalador")),
        _ => return Err(err!(E114, "o MANIFEST não diz o formato")),
    };
    let _ = formato;
    match get(&meta, "linha") {
        Some("system") | Some("rockbox") => {}
        Some(other) => return Err(err!(E118, "o MANIFEST tem uma linha desconhecida: {other}")),
        None => return Err(sem_linha()),
    }
    Ok((meta, expect))
}

impl Pacote {
    /// Lê o MANIFEST, o SHA256SUMS e a ASSINATURA (os primeiros arquivos
    /// do pacote) e confere a assinatura com a chave do OpenPod.
    pub fn abrir(caminho: &Path) -> Result<Pacote, String> {
        Self::abrir_com_chave(caminho, &chave_publica())
    }

    /// Como `abrir`, mas com uma chave dada (os testes geram a sua).
    pub fn abrir_com_chave(caminho: &Path, chave: &[u8; 32]) -> Result<Pacote, String> {
        let total = std::fs::metadata(caminho).map_err(|e| erro_abrir(caminho, e))?.len();
        let mut a = abrir_tar(caminho)?;
        let mut manifest = None;
        let mut sums_text = None;
        let mut assinatura = None;
        let nao_tar = |e: std::io::Error| err!(E102, "não é um pacote .tar.gz: {e}");
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
        let manifest = manifest.ok_or_else(|| err!(E110, "o pacote não começa com o MANIFEST"))?;
        let sums_text = sums_text.ok_or_else(|| err!(E120, "o pacote não tem o SHA256SUMS depois do MANIFEST"))?;
        let assinatura = assinatura.ok_or_else(|| ERRO_ASSINATURA.to_string())?;
        if !conferir_assinatura(chave, &manifest, &sums_text, &assinatura) {
            return Err(ERRO_ASSINATURA.to_string());
        }
        let texto = String::from_utf8(manifest.clone()).map_err(|_| err!(E111, "o MANIFEST não é UTF-8"))?;
        let (meta, expect) = parse_manifest(&texto)?;
        let tipo = match get(&meta, "linha") {
            Some("system") => Tipo::System,
            Some("rockbox") => Tipo::Rockbox,
            _ => return Err(sem_linha()),
        };
        let mut p = Pacote {
            caminho: caminho.to_path_buf(),
            tipo,
            versao: get(&meta, "versao").unwrap_or("?").to_string(),
            data: get(&meta, "data").unwrap_or("").to_string(),
            titulo: get(&meta, "titulo").unwrap_or(tipo.nome()).to_string(),
            expect,
            meta,
            sums: HashMap::new(),
            manifesto: manifest,
            sums_bytes: sums_text.clone(),
            assinatura,
            chave: *chave,
            flash: Vec::new(),
            arquivos: Vec::new(),
            pastas: Vec::new(),
            total,
        };
        let sums = String::from_utf8(sums_text).map_err(|_| err!(E121, "o SHA256SUMS não é UTF-8"))?;
        for linha in sums.lines().filter(|l| !l.trim().is_empty()) {
            let (h, nome) = linha.split_once(' ').ok_or_else(|| err!(E122, "linha ruim no SHA256SUMS"))?;
            let nome = nome.trim_start_matches([' ', '*']).trim_start_matches("./");
            p.sums.insert(nome.to_string(), unhex32(h).ok_or_else(|| err!(E123, "hash ruim no SHA256SUMS"))?);
        }
        Ok(p)
    }

    pub fn get(&self, chave: &str) -> Option<&str> {
        get(&self.meta, chave)
    }

    /// Os idiomas do RockBox que o pacote traz: os arquivos
    /// `card/.rockbox/langs/*.lng`, pelo nome do arquivo. A lista vem do
    /// SHA256SUMS (que está assinado): ela existe antes de descomprimir
    /// o pacote inteiro, e nada fixo no código.
    pub fn linguas(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .sums
            .keys()
            .filter_map(|k| k.strip_prefix(PASTA_LANG))
            .filter(|n| nome_de_lang_ok(n))
            .map(|n| n.to_string())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// O arquivo de idioma é um dos do pacote? O nome tem de ser simples
    /// (sem caminho) e estar na lista de idiomas do pacote.
    pub fn tem_lang(&self, arquivo: &str) -> bool {
        nome_de_lang_ok(arquivo) && self.sums.contains_key(&format!("{PASTA_LANG}{arquivo}"))
    }

    pub fn bytes_flash(&self) -> u64 {
        self.flash.iter().map(|(_, n)| *n as u64).sum()
    }

    pub fn bytes_cartao(&self) -> u64 {
        self.arquivos.iter().map(|f| f.tamanho).sum()
    }

    /// Descomprime tudo uma vez: cada arquivo precisa estar no SHA256SUMS e
    /// bater com ele, e todo arquivo listado precisa existir. Preenche
    /// `flash`, `arquivos` e `pastas`.
    pub fn verificar(&mut self, mut progresso: impl FnMut(f32, &str)) -> Result<(), String> {
        let f = File::open(&self.caminho).map_err(|e| erro_abrir(&self.caminho, e))?;
        let total = self.total.max(1);
        let contado = Contando { interno: BufReader::with_capacity(1 << 16, f), n: std::rc::Rc::new(std::cell::Cell::new(0)) };
        let pos = contado.n.clone();
        let mut a = tar::Archive::new(GzDecoder::new(contado));
        let (mut flash, mut arquivos, mut pastas) = (Vec::new(), Vec::new(), Vec::new());
        let (mut manifesto, mut sums_bytes, mut assinatura) = (None, None, None);
        let mut vistos = 0usize;
        let err_arq = erro_pacote;
        for e in a.entries().map_err(err_arq)? {
            let e = e.map_err(err_arq)?;
            let nome = nome_entrada(&e)?;
            let tipo = e.header().entry_type();
            match nome.as_str() {
                "MANIFEST" => {
                    manifesto = Some(ler_tudo(e, 1 << 20)?);
                    continue;
                }
                "SHA256SUMS" => {
                    sums_bytes = Some(ler_tudo(e, 16 << 20)?);
                    continue;
                }
                ASSINATURA => {
                    assinatura = Some(ler_tudo(e, 1 << 20)?);
                    continue;
                }
                _ => {}
            }
            if nome.is_empty() || nome == "flash" || nome == "card" {
                continue;
            }
            if tipo.is_dir() {
                if let Some(rel) = nome.strip_prefix("card/") {
                    if !caminho_seguro(rel) {
                        return Err(caminho_inseguro(&nome));
                    }
                    pastas.push(rel.to_string());
                }
                continue;
            }
            if !tipo.is_file() {
                return Err(err!(E131, "{nome}: links e arquivos especiais não são permitidos"));
            }
            let quer = *self.sums.get(&nome).ok_or_else(|| nao_listado(&nome))?;
            progresso(pos.get() as f32 / total as f32, &nome);
            let (tem, tamanho) = bombear(e, |_| Ok(()))?;
            if tem != quer {
                return Err(err!(E133, "{nome}: o checksum não bate, o download está corrompido"));
            }
            vistos += 1;
            if let Some(off) = offset_da_flash(&nome) {
                flash.push((off, tamanho as u32));
            } else if nome == "update.up" {
                if self.tipo != Tipo::System {
                    return Err(membro_inesperado(&nome));
                }
            } else if let Some(rel) = nome.strip_prefix("card/") {
                if !caminho_seguro(rel) {
                    return Err(caminho_inseguro(&nome));
                }
                arquivos.push(Arquivo { caminho: rel.to_string(), tamanho });
            } else {
                return Err(membro_inesperado(&nome));
            }
        }
        if manifesto.as_deref() != Some(&self.manifesto[..])
            || sums_bytes.as_deref() != Some(&self.sums_bytes[..])
            || assinatura.as_deref() != Some(&self.assinatura[..])
            || !conferir_assinatura(&self.chave, &self.manifesto, &self.sums_bytes, &self.assinatura)
        {
            return Err(ERRO_ASSINATURA.to_string());
        }
        if vistos != self.sums.len() {
            return Err(err!(E135, "faltam {} arquivo(s) listados no SHA256SUMS", self.sums.len() - vistos));
        }
        match self.tipo {
            Tipo::System if self.sums.get("update.up").is_none() => return Err(sem_imagem("o pacote do sistema não tem o update.up")),
            Tipo::Rockbox if flash.is_empty() => return Err(sem_imagem("o pacote do RockBox não tem a imagem da flash")),
            _ => {}
        }
        progresso(1.0, "");
        self.flash = flash;
        self.arquivos = arquivos;
        self.pastas = pastas;
        Ok(())
    }

    /// A imagem que vai para a flash: (endereço, bytes). No sistema, só a
    /// parte de 0xE000 em diante do payload do `.up`; no RockBox, o
    /// `flash/0x0000e000.bin`.
    pub fn ler_flash(&self) -> Result<Vec<(u32, Vec<u8>)>, String> {
        let mut a = abrir_tar(&self.caminho)?;
        let mut out = Vec::new();
        let err_arq = erro_pacote;
        for e in a.entries().map_err(err_arq)? {
            let e = e.map_err(err_arq)?;
            let nome = nome_entrada(&e)?;
            if nome.starts_with("card") {
                break;
            }
            if nome == "MANIFEST" || nome == "SHA256SUMS" || nome == ASSINATURA || nome.is_empty() || nome == "flash" || nome == "card" {
                continue;
            }
            let quer = *self.sums.get(&nome).ok_or_else(|| nao_listado(&nome))?;
            if nome == "update.up" {
                let dados = ler_tudo(e, 64 << 20)?;
                if sha256(&dados) != quer {
                    return Err(mudou(&nome));
                }
                if self.tipo != Tipo::System {
                    return Err(membro_inesperado(&nome));
                }
                let payload = ler_up(&dados)?;
                if payload.len() <= 0xE000 {
                    return Err(err!(E184, "o .up não tem imagem a partir de 0xE000"));
                }
                out.push((0xE000, payload[0xE000..].to_vec()));
            } else if let Some(off) = offset_da_flash(&nome) {
                let dados = ler_tudo(e, 64 << 20)?;
                if sha256(&dados) != quer {
                    return Err(mudou(&nome));
                }
                if dados.is_empty() {
                    return Err(err!(E186, "a imagem da flash está vazia"));
                }
                out.push((off, dados));
            }
        }
        if out.is_empty() {
            return Err(sem_imagem("o pacote não tem imagem para gravar"));
        }
        if out.len() != 1 || out[0].0 != 0xE000 {
            return Err(err!(E185, "o pacote precisa gravar uma imagem em 0xE000"));
        }
        Ok(out)
    }

    /// Os arquivos do cartão, lidos e conferidos.
    pub fn ler_cartao(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        let mut a = abrir_tar(&self.caminho)?;
        let mut out = Vec::new();
        let err_arq = erro_pacote;
        for e in a.entries().map_err(err_arq)? {
            let e = e.map_err(err_arq)?;
            let nome = nome_entrada(&e)?;
            let Some(rel) = nome.strip_prefix("card/") else { continue };
            if !e.header().entry_type().is_file() {
                continue;
            }
            if !caminho_seguro(rel) {
                return Err(caminho_inseguro(&nome));
            }
            let quer = *self.sums.get(&nome).ok_or_else(|| nao_listado(&nome))?;
            let dados = ler_tudo(e, 128 << 20)?;
            if sha256(&dados) != quer {
                return Err(mudou(&nome));
            }
            out.push((rel.to_string(), dados));
        }
        Ok(out)
    }
}

struct Contando<R> {
    interno: R,
    n: std::rc::Rc<std::cell::Cell<u64>>,
}

impl<R: Read> Read for Contando<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let k = self.interno.read(buf)?;
        self.n.set(self.n.get() + k as u64);
        Ok(k)
    }
}

fn offset_da_flash(nome: &str) -> Option<u32> {
    let stem = nome.strip_prefix("flash/")?.strip_suffix(".bin")?;
    num(stem).and_then(|v| u32::try_from(v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/openpod/kit-teste-3.5-diag.up")).unwrap()
    }

    #[test]
    fn o_up_do_kit_real_abre() {
        let dados = kit();
        let payload = ler_up(&dados).unwrap();
        assert_eq!(payload.len(), 0x1af000);
        assert_eq!(u16::from_le_bytes(dados[UP_CRC16].try_into().unwrap()), 0x9112);
        assert_eq!(crc16(&payload), 0x9112);
    }

    #[test]
    fn um_up_corrompido_e_recusado() {
        let mut dados = kit();
        let i = UP_CABECALHO + 1234;
        dados[i] ^= 0xff;
        assert!(ler_up(&dados).unwrap_err().starts_with("E183"));
        let mut curto = kit();
        curto.truncate(UP_CABECALHO + 2);
        assert!(ler_up(&curto).unwrap_err().starts_with("E180"));
        let mut magico = kit();
        magico[0] = b'X';
        assert!(ler_up(&magico).unwrap_err().starts_with("E181"));
    }

    fn tar_gz(caminho: &Path, membros: &[(&str, &[u8])]) {
        let f = File::create(caminho).unwrap();
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::fast()));
        for (nome, dados) in membros {
            let mut h = tar::Header::new_gnu();
            h.set_size(dados.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, nome, *dados).unwrap();
        }
        b.finish().unwrap();
    }

    fn sums(membros: &[(&str, &[u8])]) -> String {
        membros.iter().map(|(n, d)| format!("{}  {n}\n", hex(&sha256(d)))).collect()
    }

    fn dir_teste(nome: &str) -> PathBuf {
        let d = std::env::current_dir().unwrap().join(".tmp").join(format!("pacote-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn chave_teste() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    fn assinar(chave: &ed25519_dalek::SigningKey, manifesto: &[u8], sums: &[u8]) -> String {
        use ed25519_dalek::Signer;
        let mut msg = Vec::new();
        msg.extend_from_slice(manifesto);
        msg.extend_from_slice(sums);
        hex(&chave.sign(&msg).to_bytes())
    }

    fn abrir_teste(caminho: &Path) -> Result<Pacote, String> {
        Pacote::abrir_com_chave(caminho, &chave_teste().verifying_key().to_bytes())
    }

    #[test]
    fn pacote_do_sistema_e_do_rockbox() {
        let d = dir_teste("dois");
        let sk = chave_teste();
        let up = kit();
        let manifest = b"formato=1\nlinha=system\nversao=3.5\ndata=10/12/2026\ntitulo=OpenPod System\nexpect=0 53248 47fe9ad6074d83b8d2a0619bdfb66a021bd94ba4e86abdfaeda574952ed038af\n";
        let sum = sums(&[("update.up", &up)]);
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let membros: Vec<(&str, &[u8])> =
            vec![("MANIFEST", manifest), ("SHA256SUMS", sum.as_bytes()), ("ASSINATURA", ass.as_bytes()), ("update.up", &up)];
        let ps = d.join("sistema.tar.gz");
        tar_gz(&ps, &membros);
        let mut p = abrir_teste(&ps).unwrap();
        assert_eq!(p.tipo, Tipo::System);
        assert_eq!(p.versao, "3.5");
        assert_eq!(p.expect.len(), 1);
        p.verificar(|_, _| {}).unwrap();
        let f = p.ler_flash().unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].0, 0xE000);
        assert_eq!(f[0].1.len(), 0x1af000 - 0xE000);
        assert!(p.ler_cartao().unwrap().is_empty());

        let img = vec![0x5au8; 0x2000];
        let manifest = b"formato=1\nlinha=rockbox\nversao=1.0.822b9f5e99\ndata=22/11/2026\ntitulo=OpenPod RockBox\n";
        let sum = sums(&[("flash/0x0000e000.bin", &img), ("card/.rockbox/config.cfg", b"theme: openpod128\n")]);
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let membros: Vec<(&str, &[u8])> = vec![
            ("MANIFEST", manifest),
            ("SHA256SUMS", sum.as_bytes()),
            ("ASSINATURA", ass.as_bytes()),
            ("flash/0x0000e000.bin", &img),
            ("card/.rockbox/config.cfg", b"theme: openpod128\n"),
        ];
        let pr = d.join("rockbox.tar.gz");
        tar_gz(&pr, &membros);
        let mut p = abrir_teste(&pr).unwrap();
        assert_eq!(p.tipo, Tipo::Rockbox);
        // a versão do cartão é a do MANIFEST: 1.0.<compilação do rockbox>
        assert_eq!(p.versao, "1.0.822b9f5e99");
        p.verificar(|_, _| {}).unwrap();
        assert_eq!(p.ler_flash().unwrap()[0].0, 0xE000);
        assert_eq!(p.ler_cartao().unwrap()[0].0, ".rockbox/config.cfg");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn um_pacote_sem_checksum_ou_com_membro_a_mais_e_recusado() {
        let d = dir_teste("ruins");
        let sk = chave_teste();
        let img = vec![0u8; 16];
        let manifest = b"formato=1\nlinha=rockbox\nversao=1\ndata=x\ntitulo=y\n";
        // listado mas ausente
        let sum = format!("{}  flash/0x0000e000.bin\n", hex(&sha256(&img)));
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let membros: Vec<(&str, &[u8])> = vec![("MANIFEST", manifest), ("SHA256SUMS", sum.as_bytes()), ("ASSINATURA", ass.as_bytes())];
        let p1 = d.join("faltando.tar.gz");
        tar_gz(&p1, &membros);
        let mut a = abrir_teste(&p1).unwrap();
        assert!(a.verificar(|_, _| {}).unwrap_err().starts_with("E135"));
        // presente mas fora do SHA256SUMS: parece de fora, não é nosso
        let sum = String::new();
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let membros: Vec<(&str, &[u8])> =
            vec![("MANIFEST", manifest), ("SHA256SUMS", sum.as_bytes()), ("ASSINATURA", ass.as_bytes()), ("flash/0x0000e000.bin", &img)];
        let p2 = d.join("sobrando.tar.gz");
        tar_gz(&p2, &membros);
        let mut a = abrir_teste(&p2).unwrap();
        assert_eq!(a.verificar(|_, _| {}).unwrap_err(), ERRO_ASSINATURA);
        // checksum errado
        let sum = format!("{}  flash/0x0000e000.bin\n", hex(&sha256(b"outra")));
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let membros: Vec<(&str, &[u8])> =
            vec![("MANIFEST", manifest), ("SHA256SUMS", sum.as_bytes()), ("ASSINATURA", ass.as_bytes()), ("flash/0x0000e000.bin", &img)];
        let p3 = d.join("errado.tar.gz");
        tar_gz(&p3, &membros);
        let mut a = abrir_teste(&p3).unwrap();
        assert!(a.verificar(|_, _| {}).unwrap_err().starts_with("E133"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn so_aceita_pacote_assinado_pelo_openpod() {
        let d = dir_teste("assinatura");
        let sk = chave_teste();
        let img = vec![0u8; 16];
        let manifest = b"formato=1\nlinha=rockbox\nversao=1\ndata=x\ntitulo=y\n";
        let sum = format!("{}  flash/0x0000e000.bin\n", hex(&sha256(&img)));
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let bom = vec![
            ("MANIFEST", manifest as &[u8]),
            ("SHA256SUMS", sum.as_bytes()),
            ("ASSINATURA", ass.as_bytes()),
            ("flash/0x0000e000.bin", img.as_slice()),
        ];
        let p = d.join("bom.tar.gz");
        tar_gz(&p, &bom);
        assert!(abrir_teste(&p).is_ok());

        // assinatura trocada
        let mut ruim = ass.clone();
        let primeiro = if &ruim[..1] == "a" { "b" } else { "a" };
        ruim.replace_range(..1, primeiro);
        let membros: Vec<(&str, &[u8])> =
            vec![("MANIFEST", manifest), ("SHA256SUMS", sum.as_bytes()), ("ASSINATURA", ruim.as_bytes()), ("flash/0x0000e000.bin", &img)];
        let p = d.join("trocada.tar.gz");
        tar_gz(&p, &membros);
        assert_eq!(abrir_teste(&p).unwrap_err(), ERRO_ASSINATURA);

        // MANIFEST alterado depois de assinar
        let outro_manifest = b"formato=1\nlinha=rockbox\nversao=2\ndata=x\ntitulo=y\n";
        let membros: Vec<(&str, &[u8])> = vec![
            ("MANIFEST", outro_manifest),
            ("SHA256SUMS", sum.as_bytes()),
            ("ASSINATURA", ass.as_bytes()),
            ("flash/0x0000e000.bin", &img),
        ];
        let p = d.join("manifesto.tar.gz");
        tar_gz(&p, &membros);
        assert_eq!(abrir_teste(&p).unwrap_err(), ERRO_ASSINATURA);

        // sem ASSINATURA
        let membros: Vec<(&str, &[u8])> = vec![("MANIFEST", manifest), ("SHA256SUMS", sum.as_bytes()), ("flash/0x0000e000.bin", &img)];
        let p = d.join("sem.tar.gz");
        tar_gz(&p, &membros);
        assert_eq!(abrir_teste(&p).unwrap_err(), ERRO_ASSINATURA);

        // arquivo extra fora do SHA256SUMS
        let membros: Vec<(&str, &[u8])> = vec![
            ("MANIFEST", manifest),
            ("SHA256SUMS", sum.as_bytes()),
            ("ASSINATURA", ass.as_bytes()),
            ("flash/0x0000e000.bin", &img),
            ("card/estranho.txt", b"oi"),
        ];
        let p = d.join("extra.tar.gz");
        tar_gz(&p, &membros);
        let mut a = abrir_teste(&p).unwrap();
        assert_eq!(a.verificar(|_, _| {}).unwrap_err(), ERRO_ASSINATURA);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn o_gerador_faz_pacotes_que_o_instalador_le() {
        if std::process::Command::new("python3").arg("--version").output().is_err() {
            return;
        }
        let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let d = raiz.join(".tmp").join(format!("gerador-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let gerador = raiz.join("pacotes/gera_pacotes.py");
        let kit = raiz.join("assets/openpod/kit-teste-3.5-diag.up");
        let tema = raiz.join("assets/openpod/OpenPod128.zip");
        let saida = d.to_string_lossy().to_string();
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
        let pem_s = pem.to_string_lossy().to_string();
        let rodar = |args: &[&str]| {
            let s = std::process::Command::new("python3").arg(&gerador).args(args).current_dir(&raiz).output().expect("python3");
            assert!(s.status.success(), "gerador falhou: {}", String::from_utf8_lossy(&s.stderr));
        };
        let kit_s = kit.to_string_lossy().to_string();
        rodar(&["--system", &kit_s, "--versao", "3.5", "--data", "10/12/2026", "--chave", &pem_s, "--saida", &saida]);
        let ps = d.join("openpod-system-3.5.tar.gz");
        let mut p = Pacote::abrir_com_chave(&ps, &pubkey).unwrap();
        assert_eq!(p.tipo, Tipo::System);
        assert_eq!(p.versao, "3.5");
        assert_eq!(p.expect.len(), 2);
        p.verificar(|_, _| {}).unwrap();
        let f = p.ler_flash().unwrap();
        assert_eq!(f[0].0, 0xE000);
        assert_eq!(f[0].1.len(), 0x1af000 - 0xE000);

        // um rockbox-yp3 sintético só para exercitar o gerador
        let entrada = d.join("rockbox-yp3-teste.tar.xz");
        let criar = format!(
            "import tarfile,io\nwith tarfile.open(r'{entrada}','w:xz') as t:\n for n,d in [('flash/0x0000e000.bin',bytes([0x5a])*4096),('card/.rockbox/rocks/x.rock',b'x'),('card/.rockbox/config.cfg',b'volume: -10\\n')]:\n  i=tarfile.TarInfo(n); i.size=len(d); t.addfile(i,io.BytesIO(d))\n",
            entrada = entrada.display()
        );
        let s = std::process::Command::new("python3").arg("-c").arg(&criar).output().unwrap();
        assert!(s.status.success());
        let entrada_s = entrada.to_string_lossy().to_string();
        let tema_s = tema.to_string_lossy().to_string();
        rodar(&[
            "--rockbox",
            &entrada_s,
            "--tema",
            &tema_s,
            "--versao",
            "1.0",
            "--data",
            "22/11/2026",
            "--chave",
            &pem_s,
            "--saida",
            &saida,
        ]);
        let pr = d.join("openpod-rockbox-1.0.tar.gz");
        let mut p = Pacote::abrir_com_chave(&pr, &pubkey).unwrap();
        assert_eq!(p.tipo, Tipo::Rockbox);
        p.verificar(|_, _| {}).unwrap();
        assert_eq!(p.ler_flash().unwrap()[0].0, 0xE000);
        let cartao = p.ler_cartao().unwrap();
        let modelo = cartao.iter().find(|(n, _)| n == ".rockbox/config-openpod.cfg").expect("config-openpod.cfg");
        let texto = String::from_utf8_lossy(&modelo.1);
        // o modelo são as diretivas reais do tema, não a chave inexistente "theme:"
        for chave in [
            "wps: /.rockbox/wps/OpenPod128.wps",
            "sbs: /.rockbox/wps/OpenPod128.sbs",
            "font: /.rockbox/fonts/12-Inter-Medium.fnt",
            "iconset: /.rockbox/icons/OpenPod128.bmp",
            "viewers iconset: -",
            "statusbar: off",
            "scrollbar: off",
            "show icons: on",
            "foreground color: FFFFFF",
            "background color: 000000",
            "selector type: bar (color)",
            "line selector start color: 000000",
            "line selector text color: FFFFFF",
            "filetype colours: -",
            "backdrop: -",
            "ui viewport: -",
        ] {
            assert!(texto.lines().any(|l| l.trim() == chave), "o modelo não tem {chave:?}:\n{texto}");
        }
        assert!(!texto.contains("theme:"), "o Rockbox ignora a opção theme:\n{texto}");
        assert!(texto.contains("lang: /.rockbox/langs/portugues-brasileiro.lng"));
        assert!(cartao.iter().any(|(n, _)| n == ".rockbox/wps/OpenPod128.wps"), "o tema tem de ficar em .rockbox/");
        assert!(cartao.iter().all(|(n, _)| n != ".rockbox/config.cfg"), "o config.cfg do usuário não vai no pacote");
        assert!(cartao.iter().all(|(n, _)| !n.starts_with("rockbox/")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn caminhos() {
        assert!(caminho_seguro(".rockbox/codecs/mpa.codec"));
        assert!(!caminho_seguro("../etc/passwd"));
        assert!(!caminho_seguro("/abs"));
        assert!(!caminho_seguro("a//b"));
        assert!(!caminho_seguro("C:/x"));
        assert!(!caminho_seguro("a/./b"));
    }

    /// O pacote assinado é que diz os idiomas: a lista sai dos arquivos
    /// `card/.rockbox/langs/*.lng`, e o nome escolhido tem de estar nela.
    #[test]
    fn a_lista_de_idiomas_vem_do_pacote() {
        let d = dir_teste("langs");
        let sk = chave_teste();
        let img = vec![0x5au8; 16];
        let manifest = b"formato=1\nlinha=rockbox\nversao=1.0\ndata=22/11/2026\ntitulo=OpenPod RockBox\n";
        let arquivos: Vec<(&str, &[u8])> = vec![
            ("flash/0x0000e000.bin", img.as_slice()),
            ("card/.rockbox/langs/english.lng", b"english".as_slice()),
            ("card/.rockbox/langs/portugues-brasileiro.lng", b"portugues".as_slice()),
            ("card/.rockbox/langs/deutsch.lng", b"deutsch".as_slice()),
        ];
        let sum = sums(&arquivos);
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let mut membros: Vec<(&str, &[u8])> = vec![
            ("MANIFEST", manifest),
            ("SHA256SUMS", sum.as_bytes()),
            ("ASSINATURA", ass.as_bytes()),
        ];
        membros.extend(arquivos.iter().copied());
        let caminho = d.join("rockbox.tar.gz");
        tar_gz(&caminho, &membros);
        let mut pkg = abrir_teste(&caminho).unwrap();
        assert_eq!(pkg.linguas(), vec!["deutsch.lng", "english.lng", "portugues-brasileiro.lng"]);
        // o nome escolhido tem de estar na lista do pacote
        assert!(pkg.tem_lang("english.lng"));
        assert!(pkg.tem_lang("portugues-brasileiro.lng"));
        assert!(!pkg.tem_lang("francais.lng"), "o pacote não traz esse idioma");
        // e nunca um caminho
        assert!(!pkg.tem_lang("../etc/passwd.lng"));
        assert!(!pkg.tem_lang("english.lng/../x.lng"));
        assert!(!pkg.tem_lang("/etc/passwd.lng"));
        assert!(!pkg.tem_lang(".escondido.lng"));
        assert!(!pkg.tem_lang("english"));
        // a lista continua a mesma depois de conferir o pacote inteiro
        pkg.verificar(|_, _| {}).unwrap();
        assert_eq!(pkg.linguas().len(), 3);
        assert!(pkg.tem_lang("deutsch.lng"));

        // um pacote sem idiomas: lista vazia, e nenhum nome serve
        let sum = format!("{}  flash/0x0000e000.bin\n", hex(&sha256(&img)));
        let ass = assinar(&sk, manifest, sum.as_bytes());
        let membros: Vec<(&str, &[u8])> = vec![
            ("MANIFEST", manifest),
            ("SHA256SUMS", sum.as_bytes()),
            ("ASSINATURA", ass.as_bytes()),
            ("flash/0x0000e000.bin", img.as_slice()),
        ];
        let caminho = d.join("sem-lang.tar.gz");
        tar_gz(&caminho, &membros);
        let pkg = abrir_teste(&caminho).unwrap();
        assert!(pkg.linguas().is_empty());
        assert!(!pkg.tem_lang("english.lng"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nome_de_lang_so_eh_nome_de_arquivo() {
        assert!(nome_de_lang_ok("english.lng"));
        assert!(!nome_de_lang_ok("english"));
        assert!(!nome_de_lang_ok(""));
        assert!(!nome_de_lang_ok("../english.lng"));
        assert!(!nome_de_lang_ok("a/b.lng"));
        assert!(!nome_de_lang_ok("a\\b.lng"));
        assert!(!nome_de_lang_ok("C:b.lng"));
        assert!(!nome_de_lang_ok(".hidden.lng"));
        assert!(!nome_de_lang_ok("english.lng\n"));
    }

    #[test]
    fn o_manifesto_diz_o_formato_e_a_linha() {
        let m = |t: &str| parse_manifest(t);
        assert!(m("formato=1\nlinha=system\nversao=x\n").is_ok());
        assert!(m(&format!("formato={}\nlinha=system\n", FORMATO + 1)).unwrap_err().starts_with("E113"));
        assert!(m("formato=0\nlinha=system\n").unwrap_err().starts_with("E116"));
        assert!(m("linha=system\n").unwrap_err().starts_with("E114"));
        assert!(m("formato=1\n").unwrap_err().starts_with("E115"));
        assert!(m("formato=1\nlinha=outra\n").unwrap_err().starts_with("E118"));
        assert!(m("formato=1\nlinha=system\ncoisa_nova=1\n").is_ok());
    }
}
