//! Aparelho simulado: um player de mentira, para `--demo` e as capturas.
//! Nada aqui toca no USB.
//!
//! Adaptado do rockslop de Jayden Milne (GPL-2.0-or-later),
//! `src/device/sim.rs`. Veja CREDITOS.md.

use super::*;
use crate::pacote::{Pacote, Tipo};
use std::time::{Duration, Instant};

pub struct Sim {
    assistindo: Option<Instant>,
    estado: Option<Modo>,
    velocidade: f32,
}

impl Sim {
    pub fn new(velocidade: f32) -> Sim {
        Sim { assistindo: None, estado: None, velocidade: velocidade.max(0.05) }
    }

    fn nap(&self, ms: u64) {
        std::thread::sleep(Duration::from_micros((ms as f32 * 1000.0 / self.velocidade) as u64));
    }

    fn aparecer(&mut self) -> bool {
        let Some(t) = self.assistindo else { return false };
        if self.estado.is_some() {
            return true;
        }
        if t.elapsed().as_secs_f32() * self.velocidade > 1.5 {
            self.estado = Some(Modo::Rom);
            true
        } else {
            false
        }
    }

    fn identidade(&self, ev: &Sink) {
        for (k, v) in [
            ("USB", "301a:2800 SMTLINK DEVICE 2.00"),
            ("CHIP", "SL6801 (serial 20201111000001)"),
            ("PAYLOAD", "serviço @0x806000 ativo"),
            ("CPU", "CORTEX-M4 r0p1 (410fc241)"),
            ("FLASH", "P25Q16H 2 MiB (85 60 15)"),
            ("BATERIA", "4.04 V  SEM CARGA"),
        ] {
            self.nap(120);
            ev.conferencia(k, v, Some(true));
        }
    }
}

impl Backend for Sim {
    fn scan(&mut self, _ev: &Sink) -> Vec<Visto> {
        if self.aparecer() {
            vec![Visto { modo: self.estado.unwrap_or(Modo::Rom), acesso: true }]
        } else {
            Vec::new()
        }
    }

    fn run(&mut self, cmd: Comando, ev: &Sink) -> Result<(), String> {
        match cmd {
            Comando::Assistir(on) => {
                if on {
                    self.assistindo = Some(Instant::now());
                    self.estado = None;
                } else {
                    self.assistindo = None;
                    self.estado = None;
                }
                Ok(())
            }
            Comando::Procurar => {
                let v = self.scan(ev);
                ev.send(Evento::Presenca(v));
                Ok(())
            }
            Comando::Conferir => {
                ev.fase(Fase::Connect);
                self.nap(300);
                self.identidade(ev);
                ev.send(Evento::Bateria(4040));
                ev.send(Evento::Pronto);
                Ok(())
            }
            Comando::Gravar { pacote, .. } => self.gravar(&pacote, ev),
            Comando::Restaurar { imagem } => self.restaurar(&imagem, ev),
        }
    }
}

impl Sim {
    fn gravar(&mut self, caminho: &std::path::Path, ev: &Sink) -> Result<(), String> {
        let mut pkg = Pacote::abrir(caminho).ok();
        let tipo = pkg.as_ref().map(|p| p.tipo).unwrap_or(Tipo::Rockbox);
        ev.fase(Fase::Connect);
        ev.info("conectando no modo de download");
        self.nap(400);
        self.identidade(ev);

        ev.fase(Fase::Backup);
        let n = 512;
        for i in 0..n {
            if i % 16 == 0 {
                self.nap(40);
                ev.progresso(i as f32 / n as f32);
            }
        }
        let bk = backup::dir().join("openpod-yp3-20261122-143000.bin");
        ev.ok(format!("cópia de segurança: {}", bk.display()));
        ev.send(Evento::Copia(bk.clone()));

        ev.fase(Fase::Verify);
        for (k, v) in [
            ("BOOTLOADER", "0x000000+0d000 47fe9ad6..ed038af CONHECIDO"),
            ("TABELA", "0xD000+01c GN-438 CONHECIDA"),
            ("PACOTE", "0x000000+0d000 CONFERE"),
            ("PACOTE", "0x00d000+01c CONFERE"),
        ] {
            self.nap(100);
            ev.conferencia(k, v, Some(true));
        }

        if let Some(p) = &mut pkg {
            p.verificar(|f, _| ev.progresso(f))?;
        }
        ev.fase(Fase::Write);
        let setores = if tipo == Tipo::System { 150 } else { 168 };
        ev.info(format!("{setores} setores mudam"));
        for k in 0..setores {
            self.nap(35);
            ev.progresso(k as f32 / setores as f32);
        }
        ev.fase(Fase::Check);
        self.nap(500);
        ev.ok("flash conferida: sha256 1a2b3c4d..9f8e7d6c");
        ev.fase(Fase::Reboot);
        ev.aviso(if tipo == Tipo::Rockbox { "reiniciando no RockBox" } else { "reiniciando no sistema" });
        self.estado = None;
        self.nap(900);

        let mut resultado = Resultado { backup: Some(bk), ..Default::default() };
        if tipo == Tipo::Rockbox {
            ev.fase(Fase::Sync);
            ev.info("esperando o disco USB do RockBox");
            self.nap(500);
            ev.ok("cartão em /Volumes/OPENPOD");
            let arquivos = pkg.as_ref().and_then(|p| p.ler_cartao().ok()).filter(|v| !v.is_empty()).unwrap_or_else(cartao_demo);
            let total: u64 = arquivos.iter().map(|(_, d)| d.len() as u64).sum();
            let mut feito = 0u64;
            for (rel, dados) in &arquivos {
                ev.send(Evento::Arquivo(rel.clone(), dados.len() as u64));
                self.nap(30);
                feito += dados.len() as u64;
                ev.progresso(feito as f32 / total.max(1) as f32 * 0.7);
            }
            ev.ok(format!("{} arquivos escritos", arquivos.len()));
            self.nap(300);
            ev.info("relendo os arquivos do cartão");
            for (rel, dados) in &arquivos {
                feito += dados.len() as u64;
                ev.progresso(feito as f32 / (total.max(1) * 2) as f32);
                let _ = rel;
                let _ = dados;
                self.nap(8);
            }
            ev.ok("todos os arquivos conferem no cartão");
            ev.fase(Fase::Eject);
            self.nap(200);
            resultado.arquivos = arquivos.len();
            resultado.bytes = total;
            resultado.cartao = true;
        }
        ev.fase(Fase::Done);
        ev.send(Evento::Feito(resultado));
        Ok(())
    }

    fn restaurar(&mut self, imagem: &std::path::Path, ev: &Sink) -> Result<(), String> {
        ev.fase(Fase::Connect);
        self.nap(300);
        ev.fase(Fase::Backup);
        ev.ok(format!("cópia antes de restaurar: {}", backup::dir().join("openpod-yp3-restauro.bin").display()));
        ev.fase(Fase::Verify);
        ev.ok(format!("{} carregada; o checksum confere", imagem.display()));
        ev.fase(Fase::Write);
        for k in 0..120 {
            self.nap(20);
            ev.progresso(k as f32 / 120.0);
        }
        ev.fase(Fase::Reboot);
        self.nap(600);
        ev.fase(Fase::Done);
        ev.send(Evento::Feito(Resultado { restaurado: true, ..Default::default() }));
        Ok(())
    }
}

fn cartao_demo() -> Vec<(String, Vec<u8>)> {
    vec![
        (".rockbox/config.cfg".into(), b"theme: openpod128\nlang: /.rockbox/langs/portugues-brasileiro.lng\n".to_vec()),
        (".rockbox/themes/OpenPod128.cfg".into(), vec![0u8; 320]),
        (".rockbox/rocks/apps/clock.rock".into(), vec![0u8; 18_000]),
        (".rockbox/codecs/mpa.codec".into(), vec![0u8; 74_000]),
    ]
}
