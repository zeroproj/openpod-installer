//! O dono dos arquivos: o trabalhador elevado roda como root, mas tudo o
//! que ele cria na pasta do usuário (as cópias de segurança, os arquivos do
//! cartão) precisa ficar com o dono do usuário, senão o cliente não
//! consegue mexer nos arquivos depois.

use std::path::Path;

/// Deixa o arquivo (ou a pasta) com o dono `<uid>:<gid>`.
///
/// Quem não é root só consegue mudar o dono para si mesmo; por isso o
/// trabalhador recebe o `uid` e o `gid` do usuário quando a janela o
/// inicia.
#[cfg(unix)]
pub fn ajustar(caminho: &Path, uid: u32, gid: u32) -> Result<(), String> {
    std::os::unix::fs::chown(caminho, Some(uid), Some(gid))
        .map_err(|e| crate::err!(E705, "o dono de {} não pôde ser mudado: {e}", caminho.display()))
}

/// Fora do Unix não há `chown`: o trabalhador elevado é coisa do macOS.
#[cfg(not(unix))]
pub fn ajustar(_caminho: &Path, _uid: u32, _gid: u32) -> Result<(), String> {
    Ok(())
}

/// O usuário dona do arquivo, como o sistema enxerga.
#[cfg(unix)]
pub fn dono(caminho: &Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(caminho).ok()?;
    Some((m.uid(), m.gid()))
}

#[cfg(not(unix))]
pub fn dono(_caminho: &Path) -> Option<(u32, u32)> {
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_arquivo_fica_do_usuario() {
        let dir = std::env::current_dir().unwrap().join(".tmp").join(format!("dono-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let arquivo = dir.join("copia.bin");
        std::fs::write(&arquivo, b"openpod").unwrap();
        let (uid, gid) = dono(&arquivo).expect("o arquivo tem dono");
        // O caminho do chown: o arquivo criado pelo trabalhador tem de ficar
        // com o dono do usuário.
        ajustar(&arquivo, uid, gid).unwrap();
        assert_eq!(dono(&arquivo), Some((uid, gid)), "o dono mudou");
        assert_eq!(std::fs::read(&arquivo).unwrap(), b"openpod");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arquivo_que_nao_existe_da_erro() {
        let dir = std::env::current_dir().unwrap().join(".tmp").join(format!("dono-vazio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let e = ajustar(&dir.join("nao-existe.bin"), 0, 0).unwrap_err();
        assert!(e.starts_with("E705"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
