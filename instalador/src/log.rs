//! Um diário simples no terminal, para acompanhar o que o aparelho faz.

use crate::aparelho::{Evento, Nivel};

pub fn linha(rotulo: &str, texto: &str) {
    eprintln!("[{rotulo}] {texto}");
    // tambem num arquivo, porque aberto pelo Finder o .app nao tem terminal
    if let Some(dir) = pasta_diario() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("janela.log")) {
            let _ = writeln!(f, "[{rotulo}] {texto}");
        }
    }
}

fn pasta_diario() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = if cfg!(target_os = "macos") {
        std::path::PathBuf::from(home).join("Library").join("Caches").join("OpenPod")
    } else {
        std::path::PathBuf::from(home).join(".cache").join("openpod")
    };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn evento(e: &Evento) {
    match e {
        Evento::Log(n, s) => {
            let r = match n {
                Nivel::Info => "INFO",
                Nivel::Ok => "OK",
                Nivel::Warn => "AVISO",
                Nivel::Err => "ERRO",
                Nivel::Dim => "....",
            };
            linha(r, s);
        }
        Evento::Falhou(s) => linha("FALHOU", s),
        Evento::Copia(p) => linha("COPIA", &p.display().to_string()),
        Evento::Feito(r) => linha(
            "FEITO",
            &format!(
                "backup={:?} arquivos={} bytes={} cartao={} restaurado={} comecou_do_zero={}",
                r.backup, r.arquivos, r.bytes, r.cartao, r.restaurado, r.comecou_do_zero
            ),
        ),
        Evento::Presenca(v) => {
            if !v.is_empty() {
                linha("USB", &format!("{v:?}"));
            }
        }
        _ => {}
    }
}
