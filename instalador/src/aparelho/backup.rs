//! Cópias de segurança da flash inteira, em `~/Documents/OpenPod/backups/`,
//! uma por (aparelho, data), com um arquivo `.sha256` ao lado.
//!
//!   openpod-yp3-20261122-143000.bin
//!   openpod-yp3-20261122-143000.bin.sha256
//!
//! Adaptado do rockslop de Jayden Milne (GPL-2.0-or-later),
//! `src/device/backup.rs`. Veja CREDITOS.md.

use crate::err;
use crate::pacote::{hex, sha256, unhex32};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A pasta do OpenPod para o usuário. O instalador guarda as cópias de
/// segurança ali, onde o cliente as acha sozinho.
pub fn pasta() -> PathBuf {
    let base = if cfg!(windows) { std::env::var_os("USERPROFILE").map(PathBuf::from) } else { std::env::var_os("HOME").map(PathBuf::from) };
    base.unwrap_or_else(|| PathBuf::from(".")).join("Documents").join("OpenPod")
}

pub fn dir() -> PathBuf {
    pasta().join("backups")
}

/// Não vale uma dependência só para a hora local: as cópias são carimbadas
/// em UTC.
pub fn carimbo_agora() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

pub fn sidecar(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".sha256");
    PathBuf::from(s)
}

fn ler_soma(path: &Path) -> Option<[u8; 32]> {
    let s = std::fs::read_to_string(sidecar(path)).ok()?;
    unhex32(s.split_whitespace().next()?)
}

/// Grava a flash inteira e o `.sha256` ao lado. A cópia é relida do disco
/// antes de valer.
pub fn salvar(imagem: &[u8]) -> Result<PathBuf, String> {
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| err!(E410, "pasta de cópias {}: {e}", d.display()))?;
    let nome = format!("openpod-yp3-{}.bin", carimbo_agora());
    let path = d.join(&nome);
    let tmp = d.join(format!(".{nome}.part"));
    std::fs::write(&tmp, imagem).map_err(|e| err!(E411, "gravar a cópia: {e}"))?;
    if !std::fs::read(&tmp).map(|r| r == imagem).unwrap_or(false) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err!(E412, "a cópia não voltou igual do disco"));
    }
    std::fs::rename(&tmp, &path).map_err(|e| err!(E413, "cópia: {e}"))?;
    std::fs::write(sidecar(&path), format!("{}  {nome}\n", hex(&sha256(imagem)))).map_err(|e| err!(E414, "checksum da cópia: {e}"))?;
    Ok(path)
}

/// As cópias em disco, da mais nova para a mais velha.
pub fn listar() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| {
            let p = e.ok()?.path();
            let nome = p.file_name()?.to_str()?;
            (nome.starts_with("openpod-yp3-") && nome.ends_with(".bin")).then_some(p)
        })
        .collect();
    v.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    v
}

/// A cópia mais nova, se houver.
pub fn mais_nova() -> Option<PathBuf> {
    listar().into_iter().next()
}

/// Lê uma cópia de segurança do instalador e confere o `.sha256`. Só aceita
/// arquivos da pasta de cópias: restaurar é sempre a nossa própria cópia.
pub fn carregar(path: &Path, tamanho: usize) -> Result<Vec<u8>, String> {
    if !path.starts_with(dir()) {
        return Err(err!(E427, "{} não está na pasta de cópias do instalador", path.display()));
    }
    let base = dir().canonicalize().map_err(|e| err!(E430, "pasta de cópias {}: {e}", dir().display()))?;
    let p = path.canonicalize().map_err(|e| err!(E432, "{}: {e}", path.display()))?;
    if !p.starts_with(&base) {
        return Err(err!(E434, "{} não está na pasta de cópias do instalador", path.display()));
    }
    let dados = std::fs::read(&p).map_err(|e| err!(E421, "{}: {e}", p.display()))?;
    if dados.len() != tamanho {
        return Err(err!(E422, "{} tem {} bytes; a flash inteira tem {tamanho}", p.display(), dados.len()));
    }
    let quer = ler_soma(&p).ok_or_else(|| err!(E433, "{}: falta o arquivo .sha256", p.display()))?;
    if sha256(&dados) != quer {
        return Err(err!(E423, "{} está danificado (o checksum não bate)", p.display()));
    }
    Ok(dados)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carimbo_tem_o_formato_certo() {
        let s = carimbo_agora();
        assert_eq!(s.len(), 15);
        assert!(s.starts_with("20"));
    }

    #[test]
    fn a_copia_volta_igual_do_disco() {
        // A pasta de cópias é a do usuário; o teste usa um caminho de fora
        // para provar que ele é recusado antes de qualquer leitura.
        let dados = vec![0x5au8; 4096];
        let p = std::env::current_dir().unwrap().join(".tmp").join(format!("backup-{}.bin", std::process::id()));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &dados).unwrap();
        std::fs::write(sidecar(&p), format!("{}  x\n", hex(&sha256(&dados)))).unwrap();
        assert_eq!(carregar(&p, 4096).unwrap_err()[..4].to_string(), "E427");
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(sidecar(&p));
    }
}
