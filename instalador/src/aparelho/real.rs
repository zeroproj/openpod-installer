//! O aparelho de verdade: acha o player no USB e faz backup, conferência,
//! gravação e os arquivos do cartão.
//!
//! Um serviço é: identificar o player no modo de download, copiar a flash
//! inteira (sempre), conferir o bootloader, a tabela e as linhas `expect` do
//! pacote, gravar só os setores que mudam (nunca abaixo de 0xE000 nem a
//! partir de 0x1FC000), conferir o CRC de cada setor no aparelho, reiniciar,
//! e no RockBox copiar os arquivos do cartão.
//!
//! Adaptado do rockslop de Jayden Milne (GPL-2.0-or-later),
//! `src/device/real.rs`. Veja CREDITOS.md.

use super::backup;
use super::cartao as plano_cartao;
use super::dono;
use super::regras;
use super::sdcard::{self, Look};
use super::usb;
use super::yp3::{self, crc32, Yp3};
use super::*;
use crate::err;
use crate::pacote::{self, Pacote, Tipo};
use crate::textos::Idioma;
use std::path::Path;
use std::time::{Duration, Instant};

const ROCKBOX_VID: u16 = 0x1209;

/// O aparelho de verdade.
pub struct Real {
    /// Quando é `Some`, os arquivos criados são do dono do usuário: o
    /// trabalhador elevado roda como root (uid, gid do cliente).
    dono: Option<(u32, u32)>,
}

impl Real {
    pub fn new() -> Real {
        Real { dono: None }
    }

    /// O mesmo aparelho, mas com os arquivos do usuário (o trabalhador
    /// elevado).
    pub fn com_dono(uid: u32, gid: u32) -> Real {
        Real { dono: Some((uid, gid)) }
    }

    /// Deixa o arquivo com o dono do usuário (quando elevado).
    fn ajustar_dono(&self, caminho: &Path) -> Result<(), String> {
        match self.dono {
            Some((uid, gid)) => dono::ajustar(caminho, uid, gid),
            None => Ok(()),
        }
    }

    /// O mesmo, mas sem falhar: alguns cartões não têm dono (FAT).
    fn ajustar_dono_leve(&self, caminho: &Path) {
        if let Err(e) = self.ajustar_dono(caminho) {
            crate::log::linha("DONO", &format!("{}: {e}", caminho.display()));
        }
    }
}

fn vistos_agora() -> Vec<Visto> {
    let mut out = Vec::new();
    for (v, p, ok) in usb::list() {
        let modo = if v == yp3::VID && p == yp3::PID_ROM {
            Modo::Rom
        } else if v == yp3::VID && p == yp3::PID_STOCK {
            Modo::Stock
        } else if v == ROCKBOX_VID && p == yp3::PID_ROCKBOX {
            Modo::Rockbox
        } else {
            continue;
        };
        let s = Visto { modo, acesso: ok };
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn esperar_modo(modos: &[Modo], timeout: Duration) -> Option<Modo> {
    let t0 = Instant::now();
    let mut estavel = (None, 0);
    while t0.elapsed() < timeout {
        let m = vistos_agora().into_iter().find(|s| modos.contains(&s.modo)).map(|s| s.modo);
        estavel = if m.is_some() && m == estavel.0 { (m, estavel.1 + 1) } else { (m, 1) };
        if m.is_some() && estavel.1 >= 3 {
            return m;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    None
}

fn mv_texto(mv: u32) -> String {
    format!("{}.{:02} V", mv / 1000, mv % 1000 / 10)
}

fn hex_curto(h: &[u8; 32]) -> String {
    let s = pacote::hex(h);
    format!("{}..{}", &s[..8], &s[56..])
}

impl Backend for Real {
    fn scan(&mut self, _ev: &Sink) -> Vec<Visto> {
        vistos_agora()
    }

    fn run(&mut self, cmd: Comando, ev: &Sink) -> Result<(), String> {
        match cmd {
            Comando::Assistir(_) => Ok(()),
            Comando::Procurar => {
                ev.send(Evento::Presenca(vistos_agora()));
                Ok(())
            }
            Comando::Conferir => {
                let mut y = self.conectar(ev)?;
                self.conferir_aparelho(&mut y, ev)?;
                ev.send(Evento::Pronto);
                Ok(())
            }
            Comando::Gravar { pacote, idioma, lang_rockbox, limpar_rockbox } => {
                self.gravar(&pacote, idioma, lang_rockbox.as_deref(), limpar_rockbox, ev)
            }
            Comando::Restaurar { imagem } => self.restaurar(&imagem, ev),
        }
    }
}

impl Real {
    fn conectar(&self, ev: &Sink) -> Result<Yp3, String> {
        ev.fase(Fase::Connect);
        ev.info(format!("conectando em {:04x}:{:04x}", yp3::VID, yp3::PID_ROM));
        Yp3::open()
    }

    /// A identidade do player: tudo tem de bater antes de gravar qualquer
    /// coisa. Devolve a bateria em mV.
    fn conferir_aparelho(&self, y: &mut Yp3, ev: &Sink) -> Result<u32, String> {
        let inq = y.inquiry()?;
        if !inq.contains("SMTLINK") {
            ev.conferencia("USB", format!("301a:2800 {inq}"), Some(false));
            return Err(err!(E260, "a conferência do USB falhou: {inq}"));
        }
        ev.conferencia("USB", format!("301a:2800 {inq}"), Some(true));
        let serial = y.serial.clone().unwrap_or_default();
        ev.conferencia("CHIP", format!("SL6801 (serial {serial})"), Some(true));
        let info = y.info()?;
        ev.conferencia("PAYLOAD", "serviço @0x806000 ativo", Some(true));
        if info[8] != yp3::CPUID {
            ev.conferencia("CPU", format!("CPUID {:08x}", info[8]), Some(false));
            return Err(err!(E262, "a conferência do processador falhou: {:08x}", info[8]));
        }
        ev.conferencia("CPU", format!("CORTEX-M4 r0p1 ({:08x})", info[8]), Some(true));
        let j = y.jedec()?;
        if j != yp3::JEDEC {
            let v = format!("JEDEC {:02x} {:02x} {:02x}", j[0], j[1], j[2]);
            ev.conferencia("FLASH", v.clone(), Some(false));
            return Err(err!(E263, "a conferência da flash falhou: {v}"));
        }
        ev.conferencia("FLASH", format!("P25Q16H 2 MiB ({:02x} {:02x} {:02x})", j[0], j[1], j[2]), Some(true));
        let (mv, carga, parou) = y.battery_guard()?;
        let what = if parou {
            "CARGA PARADA"
        } else if carga {
            "CARREGANDO"
        } else {
            "SEM CARGA"
        };
        ev.conferencia("BATERIA", format!("{}  {what}", mv_texto(mv)), Some(mv > 3300));
        ev.send(Evento::Bateria(mv));
        if mv < 3300 {
            return Err(err!(E270, "bateria baixa demais para gravar com segurança ({}); carregue o player", mv_texto(mv)));
        }
        Ok(mv)
    }

    /// Copia a flash inteira (sempre) para a pasta de cópias.
    fn dump(&self, y: &mut Yp3, ev: &Sink) -> Result<(Vec<u8>, std::path::PathBuf), String> {
        ev.fase(Fase::Backup);
        let n = FLASH / SETOR;
        ev.info(format!("copiando a flash inteira ({} KiB)", FLASH / 1024));
        let crcs = y.flash_crcs(0, n)?;
        let mut img = Vec::with_capacity(FLASH as usize);
        for i in 0..n {
            let addr = i * SETOR;
            let dados = y.read_sector(addr)?;
            if crc32(&dados) != crcs[i as usize] {
                let outra = y.read_sector(addr)?;
                if crc32(&outra) != crcs[i as usize] {
                    return Err(err!(E401, "o setor 0x{addr:06x} lê de formas diferentes"));
                }
                img.extend_from_slice(&outra);
            } else {
                img.extend_from_slice(&dados);
            }
            if i % 8 == 0 || i + 1 == n {
                ev.progresso((i + 1) as f32 / n as f32);
            }
        }
        let bk = backup::salvar(&img)?;
        // o trabalhador roda como root: a cópia tem de ficar com o dono
        // do cliente, senão ele não consegue mexer nela depois
        self.ajustar_dono(&backup::pasta())?;
        self.ajustar_dono(&backup::dir())?;
        self.ajustar_dono(&bk)?;
        self.ajustar_dono(&backup::sidecar(&bk))?;
        ev.ok(format!("cópia de segurança: {}", bk.display()));
        ev.send(Evento::Copia(bk.clone()));
        Ok((img, bk))
    }

    /// As conferências que o pacote pede, mais o bootloader e a tabela.
    fn conferir_flash(&self, tipo: Tipo, cur: &[u8], pkg: &Pacote, ev: &Sink) -> Result<(), String> {
        ev.fase(Fase::Verify);
        let fail = |k: &str, v: String, e: String| {
            ev.conferencia(k, v, Some(false));
            e
        };
        if cur.len() < FLASH as usize {
            return Err(err!(E304, "a flash copiada está curta demais"));
        }
        // bootloader 0x0 + 0xD000
        let boot = pacote::sha256(&cur[..0xD000]);
        if pacote::hex(&boot) != BOOT_SHA256 {
            return Err(fail(
                "BOOTLOADER",
                format!("0x000000+0d000 {}", hex_curto(&boot)),
                err!(E301, "o bootloader deste player não é o esperado; nada foi gravado"),
            ));
        }
        ev.conferencia("BOOTLOADER", format!("0x000000+0d000 {} CONHECIDO", hex_curto(&boot)), Some(true));
        // tabela 0xD000 + 0x1C
        let tabela = pacote::sha256(&cur[0xD000..0xD000 + 0x1C]);
        let h = pacote::hex(&tabela);
        let aceita = h == TABELA_GN438 || (tipo == Tipo::Rockbox && h == TABELA_AUTOR);
        if !aceita {
            return Err(fail(
                "TABELA",
                format!("0xD000+01c {}", hex_curto(&tabela)),
                err!(E302, "a tabela de partições deste player não é a do GN-438; nada foi gravado"),
            ));
        }
        let nome = if h == TABELA_GN438 { "GN-438" } else { "player do autor" };
        ev.conferencia("TABELA", format!("0xD000+01c {nome} CONHECIDA"), Some(true));
        for e in &pkg.expect {
            let (a, b) = (e.offset as usize, e.offset as usize + e.size as usize);
            if b > cur.len() || pacote::sha256(&cur[a..b]) != e.sha256 {
                return Err(fail(
                    "PACOTE",
                    format!("0x{:06x}+{:x} DIFERENTE", e.offset, e.size),
                    err!(E303, "a flash deste player não é a que o pacote espera em 0x{:06x}; nada foi gravado", e.offset),
                ));
            }
            ev.conferencia("PACOTE", format!("0x{:06x}+{:x} CONFERE", e.offset, e.size), Some(true));
        }
        Ok(())
    }

    fn programar(&self, y: &mut Yp3, cur: &[u8], quer: &[u8], ev: &Sink) -> Result<usize, String> {
        let ss = SETOR as usize;
        let todo = regras::setores_diferentes(cur, quer);
        ev.fase(Fase::Write);
        ev.info(format!("{} de {} setores mudam", todo.len(), (GRAVAVEL.1 - GRAVAVEL.0) / SETOR));
        for (k, &i) in todo.iter().enumerate() {
            let addr = i * SETOR;
            let dados = &quer[i as usize * ss..(i as usize + 1) * ss];
            y.write_sector(addr, dados)?;
            ev.progresso((k + 1) as f32 / todo.len().max(1) as f32);
            if k % 32 == 0 {
                ev.dim(format!("0x{addr:06x} apagado, gravado e conferido"));
            }
        }
        ev.fase(Fase::Check);
        ev.info("conferindo o CRC de cada setor no aparelho");
        let (lo, hi) = (GRAVAVEL.0 / SETOR, GRAVAVEL.1 / SETOR);
        let crcs = y.flash_crcs(lo, hi - lo)?;
        for (k, c) in crcs.iter().enumerate() {
            let i = lo + k as u32;
            if *c != crc32(&quer[i as usize * ss..(i as usize + 1) * ss]) {
                return Err(err!(E321, "o CRC do setor 0x{:06x} não confere", i * SETOR));
            }
            if k % 32 == 0 {
                ev.progresso((k + 1) as f32 / (hi - lo) as f32);
            }
        }
        let soma = pacote::sha256(&quer[GRAVAVEL.0 as usize..GRAVAVEL.1 as usize]);
        ev.ok(format!("flash conferida: sha256 {}", hex_curto(&soma)));
        Ok(todo.len())
    }

    fn reiniciar(&self, mut y: Yp3, tipo: Tipo, ev: &Sink) -> Result<(), String> {
        ev.fase(Fase::Reboot);
        y.boot()?;
        let alvo = if tipo == Tipo::Rockbox { Modo::Rockbox } else { Modo::Stock };
        ev.aviso(if tipo == Tipo::Rockbox { "reiniciando no RockBox" } else { "reiniciando no sistema" });
        std::thread::sleep(Duration::from_millis(800));
        match esperar_modo(&[alvo, Modo::Rom], Duration::from_secs(45)) {
            Some(Modo::Rom) => Err(err!(E331, "o player voltou no modo de download: o novo sistema não iniciou")),
            Some(_) => {
                ev.ok("o player está de pé");
                Ok(())
            }
            None => Err(err!(E436, "o player não reiniciou no tempo; use Restaurar o original com a cópia de segurança")),
        }
    }

    /// O idioma escolhido tem de ser um dos arquivos do pacote: um nome de
    /// fora (ou um caminho) é recusado antes de gravar qualquer coisa.
    fn validar_lang(pkg: &Pacote, lang: Option<&str>) -> Result<Option<String>, String> {
        let Some(nome) = lang else { return Ok(None) };
        if !pkg.tem_lang(nome) {
            return Err(err!(E137, "o idioma {nome} não está no pacote"));
        }
        Ok(Some(nome.to_string()))
    }

    fn gravar(&mut self, caminho: &Path, idioma: Idioma, lang: Option<&str>, limpar: bool, ev: &Sink) -> Result<(), String> {
        let mut pkg = Pacote::abrir(caminho)?;
        ev.aviso(format!("{} {} · {}", pkg.tipo.nome(), pkg.versao, pkg.data));
        pkg.verificar(|f, _| ev.progresso(f))?;
        ev.ok("pacote conferido: os checksums batem");
        let lang = Self::validar_lang(&pkg, lang)?;
        if let Some(nome) = &lang {
            ev.info(format!("idioma do player: {nome}"));
        }

        let mut y = self.conectar(ev)?;
        self.conferir_aparelho(&mut y, ev)?;
        let (cur, bk) = self.dump(&mut y, ev)?;
        self.conferir_flash(pkg.tipo, &cur, &pkg, ev)?;

        let segs = pkg.ler_flash()?;
        let quer = regras::compor(&cur, &segs)?;
        let escritos = self.programar(&mut y, &cur, &quer, ev)?;
        ev.dim(format!("{escritos} setores regravados"));

        let precisa_cartao = pkg.tipo == Tipo::Rockbox;
        if precisa_cartao {
            // a cópia só se faz depois de reiniciar, com o Rockbox no USB
            self.reiniciar(y, pkg.tipo, ev)?;
            let arquivos = pkg.ler_cartao()?;
            let (n, bytes, config_mantida, comecou_do_zero) = self.cartao(&arquivos, idioma, lang.as_deref(), limpar, ev)?;
            ev.fase(Fase::Done);
            ev.send(Evento::Feito(Resultado {
                backup: Some(bk),
                arquivos: n,
                bytes,
                cartao: true,
                restaurado: false,
                config_mantida,
                comecou_do_zero,
            }));
        } else {
            self.reiniciar(y, pkg.tipo, ev)?;
            ev.fase(Fase::Done);
            ev.send(Evento::Feito(Resultado {
                backup: Some(bk),
                arquivos: 0,
                bytes: 0,
                cartao: false,
                restaurado: false,
                config_mantida: false,
                comecou_do_zero: false,
            }));
        }
        Ok(())
    }

    fn restaurar(&mut self, imagem: &Path, ev: &Sink) -> Result<(), String> {
        let mut y = self.conectar(ev)?;
        self.conferir_aparelho(&mut y, ev)?;
        let (cur, _bk) = self.dump(&mut y, ev)?;
        ev.fase(Fase::Verify);
        let img = backup::carregar(imagem, FLASH as usize)?;
        regras::conferir_restauro(&cur, &img)?;
        ev.ok(format!("{} carregada; o checksum confere", imagem.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
        let (a, b) = (GRAVAVEL.0 as usize, GRAVAVEL.1 as usize);
        let mut quer = cur.clone();
        quer[a..b].copy_from_slice(&img[a..b]);
        self.programar(&mut y, &cur, &quer, ev)?;
        self.reiniciar(y, Tipo::System, ev)?;
        ev.fase(Fase::Done);
        ev.send(Evento::Feito(Resultado { restaurado: true, ..Default::default() }));
        Ok(())
    }

    /// Copia a árvore do cartão, relê o que escreveu e ejeta.
    ///
    /// Com `limpar`, a pasta `.rockbox` do cartão é apagada antes (só ela,
    /// e sem seguir link simbólico); aí o `config.cfg` sai do modelo.
    fn cartao(
        &self,
        arquivos: &[(String, Vec<u8>)],
        idioma: Idioma,
        lang: Option<&str>,
        limpar: bool,
        ev: &Sink,
    ) -> Result<(usize, u64, bool, bool), String> {
        ev.fase(Fase::Sync);
        ev.info("esperando o disco USB do RockBox");
        let mut avisou = false;
        let mut sem_midia = false;
        let mut card = sdcard::wait_card(Duration::from_secs(600), |look, el| {
            let ausente = matches!(look, Look::Absent) && el > 25.0;
            if ausente != avisou {
                avisou = ausente;
                ev.send(Evento::PrecisaDisco(ausente));
            }
            let sem = matches!(look, Look::NoMedia);
            if sem != sem_midia {
                sem_midia = sem;
                ev.send(Evento::PrecisaCartao(sem));
            }
            true
        })?;
        if avisou {
            ev.send(Evento::PrecisaDisco(false));
        }
        if sem_midia {
            ev.send(Evento::PrecisaCartao(false));
        }
        ev.ok(format!("cartão em {}", card.mount.display()));
        // "começar do zero": apaga só a pasta .rockbox do cartão (sem ela
        // não há o que apagar e o instalador segue direto)
        let mut comecou_do_zero = false;
        if limpar {
            comecou_do_zero = super::limpa::apagar_rockbox(&card.mount)?;
            if comecou_do_zero {
                ev.ok("pasta .rockbox apagada: o Rockbox começa do zero");
            } else {
                ev.info("não havia .rockbox; nada a apagar");
            }
        }
        // macOS: o Spotlight indexando o cartao disputa o acesso com o Rockbox e ja deu
        // "Input/output error" no meio da copia (2026-11-22). Esta marca o desliga no cartao.
        if cfg!(target_os = "macos") {
            let _ = std::fs::write(card.mount.join(".metadata_never_index"), b"");
        }
        // o config vem depois da limpeza: apagada a pasta, sai do modelo
        let cfg_no_cartao = card.mount.join(plano_cartao::CONFIG);
        let existente = if cfg_no_cartao.is_file() {
            Some(std::fs::read(&cfg_no_cartao).map_err(|e| err!(E522, "ler {}: {e}", cfg_no_cartao.display()))?)
        } else {
            None
        };
        let plano = plano_cartao::planejar(arquivos.to_vec(), existente, idioma, lang);
        let arquivos = &plano.copiar;
        let total: u64 = arquivos.iter().map(|(_, d)| d.len() as u64).sum();
        let mut feito = 0u64;
        for (rel, dados) in arquivos {
            let p = card.mount.join(rel);
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).map_err(|e| err!(E520, "{}: {e}", dir.display()))?;
            }
            ev.send(Evento::Arquivo(rel.clone(), dados.len() as u64));
            // erro de E/S passageiro no cartao: tenta de novo ate 3 vezes
            let mut tentativa = 0;
            loop {
                match std::fs::write(&p, dados) {
                    Ok(()) => break,
                    Err(e) if tentativa < 3 && e.raw_os_error() == Some(5) => {
                        tentativa += 1;
                        crate::log::linha("CARD", &format!("{}: {e}; tentando de novo ({tentativa}/3)", p.display()));
                        std::thread::sleep(std::time::Duration::from_millis(800));
                    }
                    Err(e) => return Err(err!(E521, "escrever {}: {e}", p.display())),
                }
            }
            // o cartão é do cliente: o arquivo não pode ficar com o dono do
            // root (quando o trabalho é feito pelo trabalhador elevado)
            self.ajustar_dono_leve(p.parent().unwrap_or(&p));
            self.ajustar_dono_leve(&p);
            feito += dados.len() as u64;
            ev.progresso(feito as f32 / total.max(1) as f32 * 0.7);
        }
        sdcard::drop_cache(&mut card)?;
        ev.info("relendo os arquivos do cartão");
        for (rel, dados) in arquivos {
            let lido = sdcard::read_back(&card.mount.join(rel)).map_err(|e| err!(E540, "{rel}: {e}"))?;
            if pacote::sha256(&lido) != pacote::sha256(dados) {
                return Err(err!(E541, "{rel} não voltou igual do cartão"));
            }
            feito += dados.len() as u64;
            ev.progresso(feito as f32 / (total.max(1) * 2) as f32);
        }
        ev.ok(format!("{} arquivos conferidos no cartão", arquivos.len()));
        ev.fase(Fase::Eject);
        match sdcard::eject(&card) {
            Ok(()) => ev.ok("cartão ejetado"),
            Err(e) => ev.aviso(format!("não consegui ejetar o cartão ({e}); ejete antes de desconectar")),
        }
        Ok((arquivos.len(), total, plano.config_mantida, comecou_do_zero))
    }
}
