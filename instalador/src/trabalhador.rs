//! O modo trabalhador: o mesmo binário, sem janela, que fala com o
//! aparelho com permissão de administrador. É o macOS que libera, com a
//! senha do usuário; a janela continua rodando como usuário comum.
//!
//! Uso: `openpod-tune --trabalhador <socket> <token> <uid> <gid>`

use crate::aparelho::canal::{self, Linha};
use crate::aparelho::real::Real;
use crate::aparelho::{Backend, Evento, Sink};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;

/// Roda o trabalhador. Devolve o código de saída.
pub fn rodar(args: &[String]) -> i32 {
    let (socket, token, uid, gid) = match argumentos(args) {
        Some(a) => a,
        None => {
            eprintln!("uso: openpod-tune --trabalhador <socket> <token> <uid> <gid>");
            return 2;
        }
    };

    // O trabalhador roda como root: a casa do root não é a casa do
    // cliente. As cópias de segurança têm de ir para a pasta do cliente.
    match casa_do_usuario(uid) {
        Some(casa) => std::env::set_var("HOME", casa),
        None => {
            eprintln!("[TRAB] não achei a casa do usuário {uid}");
            return 4;
        }
    }

    let con = match canal::conectar(&socket, &token, Duration::from_secs(60)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[TRAB] {e}");
            return 3;
        }
    };
    let (mut leitor, escritor) = con.dividir();

    // Os eventos do aparelho vão para o socket, na mesma ordem em que
    // saíram.
    let (tx, rx) = channel::<Evento>();
    let mandador = std::thread::spawn(move || {
        let mut escritor = escritor;
        while let Ok(ev) = rx.recv() {
            if escritor.enviar(&Linha::Evento { ev }).is_err() {
                break;
            }
        }
    });

    let mut aparelho = Real::com_dono(uid, gid);
    loop {
        let linha = match leitor.receber() {
            Ok(l) => l,
            Err(e) => {
                let _ = tx.send(Evento::Falhou(e));
                break;
            }
        };
        let cmd = match linha {
            Linha::Comando { cmd } => cmd,
            _ => continue,
        };
        let sink = Sink(tx.clone());
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| aparelho.run(cmd, &sink)));
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = tx.send(Evento::Falhou(e));
            }
            Err(p) => {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "erro interno".into());
                let _ = tx.send(Evento::Falhou(crate::err!(E724, "erro interno no trabalhador: {msg}")));
            }
        }
    }

    drop(tx);
    let _ = mandador.join();
    0
}

/// Os quatro argumentos do modo trabalhador.
fn argumentos(args: &[String]) -> Option<(PathBuf, String, u32, u32)> {
    let (socket, token, uid, gid) = match args {
        [s, t, u, g] => (s, t, u, g),
        _ => return None,
    };
    let uid = uid.parse::<u32>().ok()?;
    let gid = gid.parse::<u32>().ok()?;
    Some((PathBuf::from(socket), token.clone(), uid, gid))
}

/// A casa de um usuário pelo `uid` (o root não sabe a casa de ninguém).
pub fn casa_do_usuario(uid: u32) -> Option<PathBuf> {
    // SAFETY: getpwuid devolve um ponteiro para a struct do sistema; a
    // string é copiada na hora e o ponteiro não é guardado.
    unsafe {
        let pw = libc::getpwuid(uid as libc::uid_t);
        if pw.is_null() {
            return None;
        }
        let dir = (*pw).pw_dir;
        if dir.is_null() {
            return None;
        }
        let c = std::ffi::CStr::from_ptr(dir);
        Some(PathBuf::from(c.to_string_lossy().as_ref()))
    }
}

/// O comando é o mesmo que a janela manda; serve só para o teste.
#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_argumentos_sao_quatro() {
        let a: Vec<String> = ["s", "t", "10", "20"].iter().map(|s| s.to_string()).collect();
        let (socket, token, uid, gid) = argumentos(&a).unwrap();
        assert_eq!(socket, PathBuf::from("s"));
        assert_eq!(token, "t");
        assert_eq!(uid, 10);
        assert_eq!(gid, 20);
        assert!(argumentos(&a[..3]).is_none());
        assert!(argumentos(&["s".into(), "t".into(), "x".into(), "20".into()]).is_none());
    }

    #[test]
    fn a_casa_do_usuario_e_a_dele() {
        let uid = unsafe { libc::getuid() };
        let casa = casa_do_usuario(uid).expect("o sistema tem a casa do usuário");
        assert!(casa.is_absolute(), "a casa tem de ser um caminho inteiro: {casa:?}");
        assert!(casa.exists(), "a casa não existe: {casa:?}");
    }
}
