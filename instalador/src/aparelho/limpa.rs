//! "Começar do zero" no RockBox: apagar a pasta `.rockbox` do cartão.
//!
//! O alvo é sempre o diretório `.rockbox`, filho direto da raiz montada do
//! cartão. Três regras, sem exceção:
//! - o alvo é conferido com `canonicalize`: tem de ser mesmo a `.rockbox`
//!   da raiz do cartão, nada acima nem fora dela;
//! - link simbólico não é seguido: o link é apagado, nunca o destino;
//! - nada fora da pasta é apagado (as músicas do usuário ficam).
//!
//! Sem a pasta no cartão não é erro: não se apaga nada e o instalador
//! segue direto para a cópia.

#![allow(dead_code)]

use crate::err;
use std::path::Path;

/// A pasta do RockBox no cartão.
pub const PASTA: &str = ".rockbox";

/// Não consegui ler alguma coisa do cartão.
fn erro_leitura(caminho: &Path, e: std::io::Error) -> String {
    err!(E525, "não consegui ler {}: {e}", caminho.display())
}

/// Não consegui apagar alguma coisa do cartão.
fn erro_apagar(caminho: &Path, e: std::io::Error) -> String {
    err!(E526, "não consegui apagar {}: {e}", caminho.display())
}

/// Apaga a pasta `.rockbox` do cartão. Devolve `true` quando apagou
/// alguma coisa e `false` quando a pasta não existia.
pub fn apagar_rockbox(montado: &Path) -> Result<bool, String> {
    // a raiz real do cartão (o ponto de montagem pode ser um link)
    let raiz = montado.canonicalize().map_err(|e| err!(E524, "não consegui ler a raiz do cartão {}: {e}", montado.display()))?;
    // o alvo: a `.rockbox` dessa raiz
    let alvo = raiz.join(PASTA);
    let meta = match std::fs::symlink_metadata(&alvo) {
        Ok(m) => m,
        // sem a pasta: não há o que apagar (e não é erro)
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(erro_leitura(&alvo, e)),
    };
    if meta.file_type().is_symlink() {
        // a `.rockbox` é um link: o link é apagado, o destino fica
        std::fs::remove_file(&alvo).map_err(|e| erro_apagar(&alvo, e))?;
        return Ok(true);
    }
    if !meta.is_dir() {
        // um arquivo comum com esse nome: apaga o arquivo, nada mais
        std::fs::remove_file(&alvo).map_err(|e| erro_apagar(&alvo, e))?;
        return Ok(true);
    }
    // a pasta é mesmo a `.rockbox` da raiz do cartão?
    let resolvido = alvo.canonicalize().map_err(|e| erro_leitura(&alvo, e))?;
    if resolvido.parent() != Some(raiz.as_path()) || resolvido.file_name().map(|n| n != PASTA).unwrap_or(true) {
        return Err(err!(E527, "{} não é a pasta .rockbox do cartão; nada foi apagado", alvo.display()));
    }
    apagar_arvore(&alvo)?;
    Ok(true)
}

/// Apaga uma árvore sem seguir link simbólico: um link é apagado, nunca o
/// destino.
fn apagar_arvore(caminho: &Path) -> Result<(), String> {
    // macOS: o `._x` (AppleDouble) some sozinho quando o `x` e apagado; o que ja
    // sumiu nao e erro (E525 no aparelho em 2026-11-22)
    let meta = match std::fs::symlink_metadata(caminho) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(erro_leitura(caminho, e)),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        match std::fs::remove_file(caminho) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(erro_apagar(caminho, e)),
        }
        return Ok(());
    }
    let entradas = std::fs::read_dir(caminho).map_err(|e| erro_leitura(caminho, e))?;
    for entrada in entradas {
        let entrada = entrada.map_err(|e| erro_leitura(caminho, e))?;
        apagar_arvore(&entrada.path())?;
    }
    std::fs::remove_dir(caminho).map_err(|e| erro_apagar(caminho, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir_teste(nome: &str) -> PathBuf {
        let d = std::env::current_dir().unwrap().join(".tmp").join(format!("limpa-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A árvore de mentira: músicas fora da pasta, um link apontando para
    /// fora e arquivos dentro da pasta.
    #[test]
    fn apaga_so_a_pasta_do_rockbox() {
        let d = dir_teste("arvore");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(cartao.join(".rockbox/rocks")).unwrap();
        std::fs::create_dir_all(cartao.join("Musicas")).unwrap();
        // a música do usuário, fora da pasta
        std::fs::write(cartao.join("Musicas/musica.mp3"), b"musica").unwrap();
        // o destino de fora do link
        let fora = d.join("Fora");
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("segredo.txt"), b"segredo").unwrap();
        // dentro da pasta: o config e um plugin
        std::fs::write(cartao.join(".rockbox/config.cfg"), b"lang: x").unwrap();
        std::fs::write(cartao.join(".rockbox/rocks/x.rock"), b"rock").unwrap();
        // um link dentro da pasta apontando para fora
        #[cfg(unix)]
        std::os::unix::fs::symlink(&fora, cartao.join(".rockbox/link")).unwrap();

        assert!(apagar_rockbox(&cartao).unwrap(), "a pasta foi apagada");
        assert!(!cartao.join(".rockbox").exists(), "a pasta .rockbox sumiu");
        // a música do usuário continua
        assert!(cartao.join("Musicas/musica.mp3").is_file(), "a música ficou");
        // o destino do link continua: o link é que foi apagado
        assert!(fora.join("segredo.txt").is_file(), "o destino do link não pode ser apagado");
        #[cfg(unix)]
        assert!(!cartao.join(".rockbox/link").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Sem a pasta `.rockbox` no cartão: nada é apagado e não dá erro.
    #[test]
    fn sem_a_pasta_nao_acontece_nada() {
        let d = dir_teste("sem-pasta");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(&cartao).unwrap();
        std::fs::write(cartao.join("musica.mp3"), b"musica").unwrap();
        assert!(!apagar_rockbox(&cartao).unwrap(), "não havia o que apagar");
        assert!(cartao.join("musica.mp3").is_file(), "a música ficou");
        assert!(cartao.join(".rockbox").exists() == false);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A `.rockbox` é um link para fora: o link é apagado, o destino fica.
    #[cfg(unix)]
    #[test]
    fn o_link_da_pasta_e_apagado_sem_seu_destino() {
        let d = dir_teste("link-da-pasta");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(&cartao).unwrap();
        let fora = d.join("Fora");
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("segredo.txt"), b"segredo").unwrap();
        std::os::unix::fs::symlink(&fora, cartao.join(".rockbox")).unwrap();
        assert!(apagar_rockbox(&cartao).unwrap());
        assert!(!cartao.join(".rockbox").exists(), "o link sumiu");
        assert!(fora.join("segredo.txt").is_file(), "o destino do link não pode ser apagado");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Um link dentro da pasta que aponta para a pasta vizinha: nem a
    /// vizinha nem o link são tocados de um jeito errado.
    #[cfg(unix)]
    #[test]
    fn o_link_para_a_pasta_vizinha_nao_leva_nada() {
        let d = dir_teste("link-vizinho");
        let cartao = d.join("Cartao");
        std::fs::create_dir_all(cartao.join(".rockbox")).unwrap();
        std::fs::create_dir_all(cartao.join("Vizinha")).unwrap();
        std::fs::write(cartao.join("Vizinha/importante.txt"), b"importante").unwrap();
        std::os::unix::fs::symlink(cartao.join("Vizinha"), cartao.join(".rockbox/vizinha")).unwrap();
        assert!(apagar_rockbox(&cartao).unwrap());
        assert!(cartao.join("Vizinha/importante.txt").is_file(), "a pasta vizinha ficou");
        assert!(!cartao.join(".rockbox").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
