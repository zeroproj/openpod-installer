//! A janela: as quatro telas do modelo (escolha, conectar, instalando,
//! pronto) e o estado entre elas.

use crate::aparelho::cartao;
use crate::aparelho::real::Real;
use crate::aparelho::{backup, Backend, Comando, Evento, Fase, Hub, Modo, Resultado, Visto};
use crate::estilo::{self, EstadoPasso};
use crate::pacote::{Pacote, Tipo};
use crate::site;
use crate::textos::{self, t, Idioma};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tela {
    Escolha,
    Conectar,
    Instalando,
    Pronto,
}

#[derive(Clone)]
enum Fonte {
    Site,
    Arquivo(PathBuf),
}

/// Um idioma do RockBox: o arquivo do pacote (o nome do `.lng`) e o nome
/// que aparece na lista.
#[derive(Clone)]
struct IdiomaItem {
    arquivo: String,
    nome: String,
}

pub struct App {
    idioma: Idioma,
    tela: Tela,
    tipo: Tipo,
    fonte: Fonte,
    hub: Hub,
    indice: Option<site::Indice>,
    indice_rx: Option<Receiver<Result<site::Indice, String>>>,
    baixando: Option<Receiver<Result<PathBuf, String>>>,
    pacote: Option<PathBuf>,
    /// versão e data do pacote escolhido à mão, para o cartão
    arquivo: Option<(Tipo, String, String)>,
    /// os idiomas do RockBox do pacote escolhido (o nome do arquivo .lng)
    linguas: Vec<String>,
    /// o idioma que o usuário escolheu na lista (None = o do instalador)
    lang_rockbox: Option<String>,
    /// "começar do zero": apagar a pasta .rockbox do cartão antes de instalar
    limpar_rockbox: bool,
    /// o aviso de "começar do zero" está na tela (esperando a confirmação)
    confirmado_limpeza: bool,
    restaurar: Option<PathBuf>,
    erro: Option<String>,
    encontrado: Option<Visto>,
    bateria_mv: Option<u32>,
    conferindo: bool,
    conferir_falhou: bool,
    pronto: bool,
    fase: Fase,
    progresso: f32,
    aviso_cartao: bool,
    aviso_disco: bool,
    /// a cópia de segurança feita neste trabalho (para a recuperação)
    copia: Option<PathBuf>,
    /// a fase em que o último trabalho falhou
    falha_fase: Option<Fase>,
    resultado: Resultado,
    logo: Option<egui::TextureHandle>,
    /// o print da tela do OpenPod System (na tela de escolha)
    tela_system: Option<egui::TextureHandle>,
    /// o print do menu do OpenPod RockBox (na tela de escolha)
    tela_rockbox: Option<egui::TextureHandle>,
    /// o último comando de verdade (para refazer depois da permissão)
    ultimo_comando: Option<Comando>,
    /// o pedido de senha já foi feito (não se pede de novo sozinho)
    admin_pediu: bool,
    /// o erro da tela é o do macOS (o "Tentar de novo" pede a senha)
    erro_admin: bool,
    /// o trabalhador elevado (o macOS pede a senha)
    #[cfg(target_os = "macos")]
    elevacao: crate::elevacao::Elevacao,
    /// a troca do aparelho (o trabalhador elevado) foi feita?
    troca_rx: Option<Receiver<Result<(), String>>>,
}

/// Falhou depois do backup, na gravação, na conferência ou no reinício:
/// a saída segura é restaurar a cópia, não tentar gravar de novo.
fn ha_recuperacao(copia: bool, falha: Option<Fase>) -> bool {
    copia && matches!(falha, Some(Fase::Write) | Some(Fase::Check) | Some(Fase::Verify) | Some(Fase::Reboot))
}

/// Abre o pacote e devolve o que a tela precisa; a assinatura é conferida
/// aqui, antes de qualquer conexão com o player. A lista de idiomas do
/// RockBox vem do próprio pacote (dos arquivos `.lng` que ele traz).
fn conferir_pacote(caminho: &std::path::Path) -> Result<(Tipo, String, String, Vec<String>), String> {
    let pkg = Pacote::abrir(caminho)?;
    let linguas = pkg.linguas();
    Ok((pkg.tipo, pkg.versao, pkg.data, linguas))
}

impl App {
    pub fn novo(cc: &eframe::CreationContext<'_>) -> App {
        estilo::instalar_fontes(&cc.egui_ctx);
        estilo::aplicar_tema(&cc.egui_ctx);
        let dono = std::env::var("OPENPOD_DONO").ok().and_then(|v| {
            let (u, g) = v.split_once(':')?;
            Some((u.parse::<u32>().ok()?, g.parse::<u32>().ok()?))
        });
        let backend: Box<dyn Backend> = match dono {
            Some((uid, gid)) => Box::new(Real::com_dono(uid, gid)),
            None => Box::new(Real::new()),
        };
        let mut app = App::com_backend(Idioma::detectar(), backend);
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title(t(app.idioma, "titulo_app").to_string()));
        app.logo = carregar_logo(&cc.egui_ctx);
        app.tela_system = carregar_textura(&cc.egui_ctx, estilo::TELA_SYSTEM, "tela-system");
        app.tela_rockbox = carregar_textura(&cc.egui_ctx, estilo::TELA_ROCKBOX, "tela-rockbox");
        app.buscar_indice();
        app
    }

    /// O app sem janela: só o estado (os testes medem a tela assim).
    fn com_backend(idioma: Idioma, backend: Box<dyn Backend>) -> App {
        App {
            idioma,
            tela: Tela::Escolha,
            tipo: Tipo::System,
            fonte: Fonte::Site,
            hub: Hub::iniciar(backend),
            indice: None,
            indice_rx: None,
            baixando: None,
            pacote: None,
            arquivo: None,
            linguas: Vec::new(),
            lang_rockbox: None,
            limpar_rockbox: false,
            confirmado_limpeza: false,
            restaurar: None,
            erro: None,
            encontrado: None,
            bateria_mv: None,
            conferindo: false,
            conferir_falhou: false,
            pronto: false,
            fase: Fase::Connect,
            progresso: 0.0,
            aviso_cartao: false,
            aviso_disco: false,
            copia: None,
            falha_fase: None,
            resultado: Resultado::default(),
            logo: None,
            tela_system: None,
            tela_rockbox: None,
            ultimo_comando: None,
            admin_pediu: false,
            erro_admin: false,
            #[cfg(target_os = "macos")]
            elevacao: crate::elevacao::Elevacao::novo(),
            troca_rx: None,
        }
    }

    fn buscar_indice(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(site::buscar());
        });
        self.indice_rx = Some(rx);
    }

    fn t(&self, chave: &str) -> &'static str {
        t(self.idioma, chave)
    }

    /// A lista de idiomas do pacote, pelo nome que aparece (alfabética).
    fn idiomas(&self) -> Vec<IdiomaItem> {
        let mut v: Vec<IdiomaItem> = self.linguas.iter().map(|a| IdiomaItem { arquivo: a.clone(), nome: textos::nome_idioma(a) }).collect();
        v.sort_by(|a, b| a.nome.cmp(&b.nome));
        v
    }

    /// O arquivo de idioma que vai para o cartão: o que o usuário escolheu,
    /// se ainda estiver na lista do pacote; senão o do instalador, se
    /// estiver; senão o primeiro da lista.
    fn arquivo_de_idioma(&self, lista: &[IdiomaItem]) -> String {
        if let Some(a) = &self.lang_rockbox {
            if lista.iter().any(|i| &i.arquivo == a) {
                return a.clone();
            }
        }
        let padrao = cartao::arquivo_do_idioma(self.idioma);
        if lista.iter().any(|i| i.arquivo == padrao) {
            return padrao.to_string();
        }
        lista.first().map(|i| i.arquivo.clone()).unwrap_or_default()
    }

    /// O idioma escolhido que não está na lista do pacote novo é
    /// esquecido: o padrão do instalador volta.
    fn esquecer_idioma_fora_da_lista(&mut self) {
        if let Some(a) = &self.lang_rockbox {
            if !self.linguas.iter().any(|l| l == a) {
                self.lang_rockbox = None;
            }
        }
    }

    fn mostrar_erro(&mut self, e: String) {
        crate::log::linha("ERRO", &e);
        self.erro_admin = false;
        self.erro = Some(textos::erro(self.idioma, &e));
    }

    /// Manda um comando ao aparelho e guarda o último de verdade (para
    /// refazer depois da permissão).
    fn enviar(&mut self, c: Comando) {
        if !matches!(c, Comando::Assistir(_) | Comando::Procurar) {
            self.ultimo_comando = Some(c.clone());
        }
        self.hub.enviar(c);
    }

    /// O macOS está prendendo o player (acesso negado, driver do kernel
    /// ativo ou aparelho ocupado)?
    #[cfg(target_os = "macos")]
    fn precisa_de_admin(&self, e: &str) -> bool {
        self.elevacao.precisa(e)
    }

    #[cfg(not(target_os = "macos"))]
    fn precisa_de_admin(&self, _e: &str) -> bool {
        false
    }

    /// Pede a senha do macOS (uma vez por sessão).
    fn pedir_admin(&mut self) {
        if self.admin_pediu {
            return;
        }
        self.admin_pediu = true;
        #[cfg(target_os = "macos")]
        self.elevacao.pedir();
    }

    /// O macOS está mostrando o pedido de senha agora?
    #[cfg(target_os = "macos")]
    fn admin_pendente(&self) -> bool {
        self.elevacao.pendente()
    }

    #[cfg(not(target_os = "macos"))]
    fn admin_pendente(&self) -> bool {
        false
    }

    /// O trabalhador elevado já está de pé?
    #[cfg(target_os = "macos")]
    fn admin_ativo(&self) -> bool {
        self.elevacao.ativo()
    }

    #[cfg(not(target_os = "macos"))]
    fn admin_ativo(&self) -> bool {
        false
    }

    fn ir_para_conectar(&mut self) {
        self.tela = Tela::Conectar;
        self.erro = None;
        self.erro_admin = false;
        self.encontrado = None;
        self.bateria_mv = None;
        self.conferindo = false;
        self.conferir_falhou = false;
        self.pronto = false;
        self.aviso_cartao = false;
        self.aviso_disco = false;
        self.copia = None;
        self.falha_fase = None;
        self.enviar(Comando::Assistir(true));
    }

    fn voltar(&mut self) {
        self.tela = Tela::Escolha;
        self.erro = None;
        self.erro_admin = false;
        self.confirmado_limpeza = false;
        self.encontrado = None;
        self.bateria_mv = None;
        self.conferir_falhou = false;
        self.pronto = false;
        self.aviso_cartao = false;
        self.aviso_disco = false;
        self.copia = None;
        self.falha_fase = None;
        self.restaurar = None;
        self.enviar(Comando::Assistir(false));
    }

    fn escolher_arquivo(&mut self) {
        // sem filtro: o macOS nao casa .tar.gz; quem decide e a assinatura
        let escolha = rfd::FileDialog::new().pick_file();
        if let Some(p) = escolha {
            match conferir_pacote(&p) {
                Ok((tipo, versao, data, linguas)) => {
                    self.tipo = tipo;
                    self.arquivo = Some((tipo, versao, data));
                    self.linguas = linguas;
                    self.pacote = None;
                    self.esquecer_idioma_fora_da_lista();
                    self.fonte = Fonte::Arquivo(p);
                    self.erro = None;
                }
                Err(e) => self.mostrar_erro(e),
            }
        }
    }

    fn continuar(&mut self) {
        self.erro = None;
        self.restaurar = None;
        if self.pacote.is_some() {
            // o pacote já está aqui (baixado do site): não baixa de novo
            self.ir_para_conectar();
            return;
        }
        match self.fonte.clone() {
            Fonte::Arquivo(p) => {
                self.pacote = Some(p);
                self.ir_para_conectar();
            }
            Fonte::Site => {
                if let Some(v) = self.indice.as_ref().and_then(|i| i.de(self.tipo).cloned()) {
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(site::baixar(&v));
                    });
                    self.baixando = Some(rx);
                } else {
                    self.erro = Some(self.t("sem_site").to_string());
                }
            }
        }
    }

    fn restaurar_original(&mut self) {
        match backup::mais_nova() {
            Some(p) => {
                self.restaurar = Some(p);
                self.pacote = None;
                self.ir_para_conectar();
            }
            None => self.erro = Some(self.t("restaurar_sem_copia").to_string()),
        }
    }

    /// O botão "Instalar". Com "começar do zero" marcado, primeiro vem o
    /// aviso: nada é apagado sem a confirmação.
    fn instalar(&mut self) {
        if self.limpar_rockbox && self.tipo == Tipo::Rockbox && !self.confirmado_limpeza {
            self.confirmado_limpeza = true;
            return;
        }
        self.confirmado_limpeza = false;
        self.comeca_instalacao();
    }

    /// O trabalho de verdade (depois do aviso, quando foi preciso).
    fn comeca_instalacao(&mut self) {
        self.erro = None;
        self.tela = Tela::Instalando;
        self.fase = Fase::Connect;
        self.progresso = 0.0;
        self.aviso_cartao = false;
        self.aviso_disco = false;
        self.copia = None;
        self.falha_fase = None;
        if let Some(imagem) = self.restaurar.clone() {
            self.enviar(Comando::Restaurar { imagem });
        } else if let Some(p) = self.pacote.clone() {
            self.enviar(self.comando_gravar(p));
        }
    }

    /// O comando de gravação, com o idioma e o "começar do zero".
    fn comando_gravar(&self, pacote: PathBuf) -> Comando {
        Comando::Gravar {
            pacote,
            idioma: self.idioma,
            lang_rockbox: self.lang_para_gravar(),
            limpar_rockbox: self.limpar_rockbox && self.tipo == Tipo::Rockbox,
        }
    }

    /// O arquivo de idioma que vai para o cartão (o escolhido na lista).
    /// Sem lista (o pacote do sistema, ou um pacote sem idiomas) fica
    /// None: o aparelho usa o padrão do instalador.
    fn lang_para_gravar(&self) -> Option<String> {
        if self.tipo != Tipo::Rockbox || self.linguas.is_empty() {
            return None;
        }
        let lista = self.idiomas();
        Some(self.arquivo_de_idioma(&lista))
    }

    /// Restaura uma cópia específica, a partir da tela de erro.
    fn restaurar_com(&mut self, caminho: PathBuf) {
        self.erro = None;
        self.tela = Tela::Instalando;
        self.fase = Fase::Connect;
        self.progresso = 0.0;
        self.aviso_cartao = false;
        self.aviso_disco = false;
        self.falha_fase = None;
        self.pacote = None;
        self.restaurar = Some(caminho.clone());
        self.enviar(Comando::Restaurar { imagem: caminho });
    }

    /// Refaz o trabalho que falhou por falta de permissão (o aviso de
    /// "começar do zero" já foi dado: não se pergunta de novo).
    fn reenviar(&mut self) {
        match self.ultimo_comando.clone() {
            Some(Comando::Gravar { .. }) | Some(Comando::Restaurar { .. }) => self.comeca_instalacao(),
            Some(c) => {
                self.conferindo = true;
                self.enviar(c);
            }
            None => {}
        }
    }

    /// Falhou depois do backup, na gravação, na conferência ou no reinício:
    /// a saída segura é restaurar a cópia, não tentar gravar de novo.
    fn tem_recuperacao(&self) -> bool {
        ha_recuperacao(self.copia.is_some(), self.falha_fase)
    }

    fn tratar_eventos(&mut self) {
        for e in self.hub.poll() {
            match e {
                Evento::Bateria(mv) => self.bateria_mv = Some(mv),
                Evento::Fase(f) => {
                    self.fase = f;
                    self.progresso = 0.0;
                }
                Evento::Progresso(p) => self.progresso = p,
                Evento::Pronto => {
                    self.conferindo = false;
                    self.conferir_falhou = false;
                    self.pronto = self.bateria_mv.map(|m| m >= 3300).unwrap_or(true);
                }
                Evento::Feito(r) => {
                    self.resultado = r;
                    self.falha_fase = None;
                    self.tela = Tela::Pronto;
                }
                Evento::Falhou(e) => {
                    self.falha_fase = Some(self.fase);
                    self.tela = Tela::Conectar;
                    self.conferindo = false;
                    self.conferir_falhou = true;
                    self.pronto = false;
                    self.enviar(Comando::Assistir(true));
                    if self.precisa_de_admin(&e) && !self.admin_ativo() {
                        // o macOS está prendendo o player: ele pede a
                        // senha e o trabalho é refeito pelo trabalhador
                        self.pedir_admin();
                    } else {
                        self.mostrar_erro(e);
                    }
                }
                Evento::Presenca(_) | Evento::Log(_, _) | Evento::Conferencia(_, _, _) => {}
                Evento::Copia(p) => self.copia = Some(p),
                Evento::PrecisaCartao(on) => self.aviso_cartao = on,
                Evento::PrecisaDisco(on) => self.aviso_disco = on,
                Evento::Arquivo(_, _) => {}
            }
        }
        if let Some(rx) = self.indice_rx.take() {
            match rx.try_recv() {
                Ok(Ok(i)) => self.indice = Some(i),
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => self.indice_rx = Some(rx),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        if let Some(rx) = self.baixando.take() {
            match rx.try_recv() {
                Ok(Ok(p)) => match conferir_pacote(&p) {
                    Ok((tipo, versao, data, linguas)) => {
                        self.tipo = tipo;
                        self.arquivo = Some((tipo, versao, data));
                        self.linguas = linguas;
                        self.esquecer_idioma_fora_da_lista();
                        self.pacote = Some(p);
                        self.ir_para_conectar();
                    }
                    Err(e) => {
                        self.pacote = None;
                        self.mostrar_erro(e);
                    }
                },
                Ok(Err(e)) => self.mostrar_erro(e),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.baixando = Some(rx),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.erro = Some(self.t("sem_site").to_string()),
            }
        }
        self.tratar_elevacao();
        if let Some(rx) = self.troca_rx.take() {
            match rx.try_recv() {
                Ok(Ok(())) => self.reenviar(),
                Ok(Err(e)) => self.mostrar_erro(e),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.troca_rx = Some(rx),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        if self.tela == Tela::Conectar && !self.hub.ocupado {
            let achou = self.hub.vistos.iter().find(|v| v.modo == Modo::Rom).copied();
            match achou {
                Some(v) if v.acesso => {
                    self.encontrado = Some(v);
                    if !self.conferindo && !self.pronto && !self.conferir_falhou {
                        self.conferindo = true;
                        self.enviar(Comando::Conferir);
                    }
                }
                Some(_) => {
                    // o player está ligado, mas este programa não
                    // consegue abrir: no macOS é o driver do kernel
                    // segurando, e só o root consegue soltar
                    self.encontrado = None;
                    self.bateria_mv = None;
                    self.conferir_falhou = false;
                    self.pronto = false;
                    self.pedir_admin();
                }
                None => {
                    self.encontrado = None;
                    self.bateria_mv = None;
                    self.conferir_falhou = false;
                    self.pronto = false;
                }
            }
        }
    }

    /// O macOS pediu a senha: o trabalhador elevado entra no lugar do
    /// USB direto e o trabalho é refeito.
    #[cfg(target_os = "macos")]
    fn tratar_elevacao(&mut self) {
        let Some(r) = self.elevacao.resultado() else { return };
        match r {
            Ok(canal) => {
                crate::log::linha("ADMIN", "o trabalhador elevado está de pé");
                self.erro = None;
                self.troca_rx = Some(self.hub.trocar_backend(Box::new(canal)));
            }
            Err(crate::elevacao::Erro::Senha) => {
                self.erro_admin = true;
                self.erro = Some(self.t("admin_sem_senha").to_string());
            }
            Err(crate::elevacao::Erro::Outra(e)) => {
                self.mostrar_erro(e);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn tratar_elevacao(&mut self) {}
}

fn carregar_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    carregar_textura(ctx, estilo::LOGO, "logo")
}

/// Uma imagem embutida vira textura da janela.
fn carregar_textura(ctx: &egui::Context, bytes: &[u8], nome: &str) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let cor = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(nome, cor, egui::TextureOptions::LINEAR))
}

fn porcento(mv: u32) -> u32 {
    (((mv.saturating_sub(3300)) as f32 / 900.0) * 100.0).clamp(0.0, 100.0) as u32
}

fn progresso_geral(_tipo: Tipo, fase: Fase, p: f32) -> f32 {
    match fase {
        Fase::Connect => p * 0.05,
        Fase::Backup => 0.05 + p * 0.30,
        Fase::Verify => 0.35 + p * 0.10,
        Fase::Write => 0.45 + p * 0.35,
        Fase::Check => 0.80 + p * 0.12,
        Fase::Reboot => 0.92,
        Fase::Sync => 0.92 + p * 0.06,
        Fase::Eject => 0.98,
        Fase::Done => 1.0,
    }
    .min(1.0)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.tratar_eventos();

        egui::CentralPanel::default().frame(egui::Frame::new().fill(estilo::FUNDO)).show(ui, |ui| {
            let margem = ((ui.available_width() - 760.0) / 2.0).max(26.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(margem);
                    ui.vertical(|ui| {
                        ui.set_width(760.0);
                        ui.add_space(22.0);
                        self.cabecalho(ui);
                        ui.add_space(24.0);
                        match self.tela {
                            Tela::Escolha => self.tela_escolha(ui),
                            Tela::Conectar => self.tela_conectar(ui),
                            Tela::Instalando => self.tela_instalando(ui),
                            Tela::Pronto => self.tela_pronto(ui),
                        }
                        ui.add_space(26.0);
                        ui.add(egui::Hyperlink::from_label_and_url(
                            estilo::link("zeroproj.github.io/OpenPod"),
                            "https://zeroproj.github.io/OpenPod/",
                        ));
                        ui.add_space(18.0);
                    });
                });
            });
        });

        // o aviso de "começar do zero" vem por cima da tela
        self.aviso_apagar(ui);
    }
}

impl App {
    fn cabecalho(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(tex) = &self.logo {
                ui.add(egui::Image::new((tex.id(), egui::vec2(30.0, 30.0))));
            } else {
                let (r, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                ui.painter().rect_filled(r, egui::CornerRadius::same(8), estilo::TEXTO);
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new("OpenPod").size(16.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let en = self.idioma == Idioma::En;
                if ui
                    .add(
                        egui::Label::new(egui::RichText::new(self.t("idioma_en")).size(12.5).color(if en {
                            estilo::AZUL
                        } else {
                            estilo::CINZA_CLARO
                        }))
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    self.idioma = Idioma::En;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(self.t("titulo_app").to_string()));
                }
                ui.label(egui::RichText::new("|").size(12.0).color(estilo::BORDA));
                let pt = self.idioma == Idioma::Pt;
                if ui
                    .add(
                        egui::Label::new(egui::RichText::new(self.t("idioma_pt")).size(12.5).color(if pt {
                            estilo::AZUL
                        } else {
                            estilo::CINZA_CLARO
                        }))
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    self.idioma = Idioma::Pt;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(self.t("titulo_app").to_string()));
                }
            });
        });
    }

    fn cartao_escolha(&mut self, ui: &mut egui::Ui, tipo: Tipo) {
        let sel = self.tipo == tipo;
        let do_arquivo = matches!(self.fonte, Fonte::Arquivo(_)) && self.arquivo.as_ref().map(|(t, _, _)| *t == tipo).unwrap_or(false);
        let linha = if do_arquivo {
            let (_, versao, data) = self.arquivo.as_ref().unwrap();
            linha_do_cartao(versao, data)
        } else {
            match self.indice.as_ref().and_then(|i| i.de(tipo)) {
                Some(v) => linha_do_cartao(&v.versao, &v.data),
                None => "—".into(),
            }
        };
        let (titulo, desc) = match tipo {
            Tipo::System => ("OpenPod System", self.t("system_desc")),
            Tipo::Rockbox => ("OpenPod RockBox", self.t("rockbox_desc")),
        };
        let print = match tipo {
            Tipo::System => self.tela_system.as_ref().map(|t| t.id()),
            Tipo::Rockbox => self.tela_rockbox.as_ref().map(|t| t.id()),
        };
        // a capa do álbum ainda está em beta: só o RockBox avisa
        let nota = match tipo {
            Tipo::System => None,
            Tipo::Rockbox => Some(self.t("capa_beta")),
        };
        let r = estilo::cartao(sel).show(ui, |ui| {
            cartao_conteudo(ui, print, titulo, desc, &linha, nota, sel);
        });
        if r.response.interact(egui::Sense::click()).clicked() {
            self.tipo = tipo;
            self.erro = None;
        }
    }

    /// O cartão do idioma do player e do "começar do zero". Só aparece com
    /// o OpenPod RockBox escolhido (e não na restauração, que não mexe no
    /// cartão). Devolve se desenhou.
    fn cartao_idioma(&mut self, ui: &mut egui::Ui) -> bool {
        if self.tipo != Tipo::Rockbox || self.restaurar.is_some() {
            return false;
        }
        let lista = self.idiomas();
        let tem_lista = !lista.is_empty();
        let atual = self.arquivo_de_idioma(&lista);
        let nome = lista.iter().find(|i| i.arquivo == atual).map(|i| i.nome.clone()).unwrap_or_default();
        let mut escolha = atual.clone();
        let titulo_idioma = self.t("idioma_player").to_string();
        let rotulo_apagar = self.t("apagar_rockbox").to_string();
        estilo::cartao(false).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_min_width(718.0);
            if tem_lista {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(titulo_idioma).size(14.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("idioma-do-player")
                            .selected_text(egui::RichText::new(nome).size(13.5))
                            .width(230.0)
                            .show_ui(ui, |ui| {
                                for item in &lista {
                                    ui.selectable_value(&mut escolha, item.arquivo.clone(), item.nome.clone());
                                }
                            });
                    });
                });
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                ui.add(egui::Checkbox::new(&mut self.limpar_rockbox, egui::RichText::new(rotulo_apagar).size(13.0)));
            });
        });
        if escolha != atual {
            // escolher o idioma que já é o padrão não é escolher: o padrão
            // continua seguindo o instalador
            self.lang_rockbox = if escolha != cartao::arquivo_do_idioma(self.idioma) { Some(escolha) } else { None };
        }
        true
    }

    /// O aviso de "começar do zero": aparece quando a caixa está marcada e
    /// o usuário aperta Instalar. Só com "Apagar e instalar" a instalação
    /// continua.
    fn aviso_apagar(&mut self, ui: &mut egui::Ui) {
        if !self.confirmado_limpeza {
            return;
        }
        let titulo = self.t("apagar_titulo").to_string();
        let texto = self.t("apagar_texto").to_string();
        let apagar = self.t("apagar_e_instalar").to_string();
        let cancelar = self.t("cancelar").to_string();
        let janela =
            egui::Window::new(titulo).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0));
        janela.show(ui.ctx(), |ui| {
            ui.set_max_width(440.0);
            ui.add(egui::Label::new(egui::RichText::new(texto).size(13.5)).wrap());
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(estilo::botao_azul(&apagar)).clicked() {
                        self.confirmado_limpeza = false;
                        self.comeca_instalacao();
                    }
                    if ui.add(egui::Button::new(egui::RichText::new(cancelar).size(14.0))).clicked() {
                        self.confirmado_limpeza = false;
                    }
                });
            });
        });
    }

    fn tela_escolha(&mut self, ui: &mut egui::Ui) {
        ui.label(estilo::titulo(self.t("escolha")));
        ui.add_space(4.0);
        ui.label(estilo::subtitulo(self.t("escolha_sub")));
        ui.add_space(20.0);
        ui.columns(2, |cols| {
            self.cartao_escolha(&mut cols[0], Tipo::System);
            self.cartao_escolha(&mut cols[1], Tipo::Rockbox);
        });
        ui.add_space(14.0);
        if self.cartao_idioma(ui) {
            ui.add_space(14.0);
        }
        ui.add_space(14.0);

        let site = matches!(self.fonte, Fonte::Site);
        let r = estilo::cartao(false).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_min_width(718.0);
            ui.horizontal(|ui| {
                radio(ui, site);
                // so o texto "versao do site" escolhe o site: o cartao inteiro clicavel
                // ficava por cima do link "Escolher arquivo" e engolia o clique
                if ui.add(egui::Label::new(egui::RichText::new(self.t("site")).size(14.0).strong()).sense(egui::Sense::click())).clicked() {
                    self.fonte = Fonte::Site;
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(self.t("ou")).size(13.0).color(estilo::CINZA));
                ui.add_space(4.0);
                if ui.add(egui::Label::new(estilo::link(self.t("arquivo"))).sense(egui::Sense::click())).clicked() {
                    self.escolher_arquivo();
                }
                if let Fonte::Arquivo(p) = &self.fonte {
                    let nome = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(nome).size(12.0).color(estilo::CINZA));
                    });
                }
            });
        });
        let _ = r;
        if self.indice.is_none() {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(self.t("sem_site")).size(12.5).color(estilo::CINZA));
        }
        if self.baixando.is_some() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(self.t("baixando")).size(13.0).color(estilo::CINZA));
            });
        }
        if let Some(e) = &self.erro {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(e).size(13.0).color(erro_cor()));
        }

        ui.add_space(34.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Label::new(estilo::link(self.t("restaurar"))).sense(egui::Sense::click())).clicked() {
                self.restaurar_original();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pode = self.baixando.is_none();
                if ui.add_enabled(pode, estilo::botao_azul(self.t("continuar"))).clicked() {
                    self.continuar();
                }
            });
        });
    }

    fn tela_conectar(&mut self, ui: &mut egui::Ui) {
        ui.label(estilo::titulo(self.t("conecte")));
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            estilo::cartao(false).inner_margin(egui::Margin::symmetric(26, 18)).show(ui, |ui| {
                ui.set_min_size(egui::vec2(240.0, 250.0));
                desenhar_player(ui);
            });
            ui.add_space(26.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                passo_texto(ui, 1, self.t("passo1"));
                ui.add_space(18.0);
                passo_texto(ui, 2, self.t("passo2"));
                ui.add_space(18.0);
                passo_texto(ui, 3, self.t("passo3"));
                ui.add_space(26.0);
                self.pilula(ui);
                if self.admin_pendente() {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new(self.t("admin_aviso")).size(12.5).color(estilo::AVISO));
                    });
                }
                if self.admin_ativo() {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(self.t("admin_trabalhador")).size(12.0).color(estilo::CINZA));
                }
                if let Some(e) = &self.erro {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(e).size(12.5).color(erro_cor()));
                    if self.tem_recuperacao() {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(self.t("recuperar_texto")).size(12.0).color(estilo::CINZA));
                        if let Some(p) = &self.copia {
                            ui.add_space(2.0);
                            let texto = self.t("copia_em").replace("{}", &p.display().to_string());
                            ui.add(egui::Label::new(egui::RichText::new(texto).size(10.5).color(estilo::CINZA)).wrap());
                        }
                        ui.add_space(8.0);
                        if ui.add(estilo::botao_azul(self.t("restaurar"))).clicked() {
                            if let Some(p) = self.copia.clone() {
                                self.restaurar_com(p);
                            }
                        }
                    } else {
                        ui.add_space(2.0);
                        if ui.add(egui::Label::new(estilo::link(self.t("tentar"))).sense(egui::Sense::click())).clicked() {
                            self.erro = None;
                            self.conferir_falhou = false;
                            self.pronto = false;
                            if self.erro_admin {
                                // o macOS pode pedir a senha de novo
                                self.admin_pediu = false;
                                self.pedir_admin();
                            }
                        }
                    }
                }
                if self.restaurar.is_some() {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(self.t("restaurar_sub")).size(12.5).color(estilo::CINZA));
                }
            });
        });
        ui.add_space(18.0);
        if self.cartao_idioma(ui) {
            ui.add_space(18.0);
        }
        ui.horizontal(|ui| {
            if ui.add(egui::Label::new(estilo::link(self.t("voltar"))).sense(egui::Sense::click())).clicked() {
                self.voltar();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = self.pronto && self.bateria_mv.map(|m| m >= 3300).unwrap_or(false);
                let texto = if self.restaurar.is_some() { self.t("restaurando") } else { self.t("instalar") };
                if ui.add_enabled(ok, estilo::botao_azul(texto)).clicked() {
                    self.instalar();
                }
            });
        });
    }

    fn pilula(&self, ui: &mut egui::Ui) {
        let achou = self.encontrado.is_some();
        let (fundo, cor, texto) = if achou {
            let p = self.bateria_mv.map(porcento).unwrap_or(0);
            (estilo::VERDE_FUNDO, estilo::VERDE, format!("● {}", self.t("encontrado").replace("{}", &p.to_string())))
        } else {
            (egui::Color32::from_rgb(0xF1, 0xF3, 0xF6), estilo::CINZA, format!("● {}", self.t("procurando")))
        };
        let r = egui::Frame::new()
            .fill(fundo)
            .corner_radius(egui::CornerRadius::same(18))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(texto).size(13.5).color(cor).strong());
            });
        let _ = r;
    }

    fn tela_instalando(&mut self, ui: &mut egui::Ui) {
        let titulo = if self.tipo == Tipo::Rockbox { self.t("instalando_rockbox") } else { self.t("instalando_system") };
        ui.label(estilo::titulo(titulo));
        ui.add_space(4.0);
        ui.label(estilo::subtitulo(self.t("nao_desconecte")));
        ui.add_space(20.0);
        let passos: &[(&str, &str)] = if self.tipo == Tipo::Rockbox {
            &[
                ("backup", self.t("etapa_backup")),
                ("verify", self.t("etapa_verify")),
                ("write", self.t("etapa_write")),
                ("card", self.t("etapa_card")),
            ]
        } else {
            &[("backup", self.t("etapa_backup")), ("verify", self.t("etapa_verify")), ("write", self.t("etapa_write"))]
        };
        let n = passos.len();
        let atual = match (self.tipo, self.fase) {
            (_, Fase::Connect) | (_, Fase::Backup) => 0,
            (_, Fase::Verify) => 1,
            (_, Fase::Write) | (_, Fase::Check) => 2,
            (Tipo::System, Fase::Reboot) => 2,
            (Tipo::Rockbox, Fase::Reboot) => 3,
            (Tipo::System, _) => 3.min(n),
            (Tipo::Rockbox, _) => 3,
        };
        let geral = progresso_geral(self.tipo, self.fase, self.progresso);
        estilo::cartao(false).inner_margin(egui::Margin::symmetric(22, 18)).show(ui, |ui| {
            for (i, (_, rotulo)) in passos.iter().enumerate() {
                let estado = if i < atual {
                    EstadoPasso::Feito
                } else if i == atual {
                    EstadoPasso::Atual
                } else {
                    EstadoPasso::Pendente
                };
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    estilo::passo(ui, estado, None);
                    ui.add_space(8.0);
                    let cor = if estado == EstadoPasso::Pendente { estilo::CINZA_CLARO } else { estilo::TEXTO };
                    let peso = estado == EstadoPasso::Atual;
                    let txt = egui::RichText::new(*rotulo).size(14.5).color(cor);
                    ui.label(if peso { txt.strong() } else { txt });
                    if i == atual {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let p = (geral * 100.0).round() as u32;
                            ui.label(egui::RichText::new(format!("{p}%")).size(12.5).color(estilo::CINZA));
                            ui.add_space(4.0);
                            estilo::barra(ui, geral, 200.0);
                        });
                    }
                });
                if i + 1 < n {
                    ui.add_space(14.0);
                }
            }
        });
        if self.aviso_cartao {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(self.t("cartao_falta")).size(13.0).color(estilo::AVISO));
        }
        if self.aviso_disco {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(self.t("disco_falta")).size(13.0).color(estilo::AVISO));
        }
    }

    fn tela_pronto(&mut self, ui: &mut egui::Ui) {
        ui.label(estilo::titulo(self.t("pronto")));
        ui.add_space(10.0);
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 500.0), egui::Layout::top_down(egui::Align::Center), |ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(116.0, 116.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 58.0, estilo::VERDE);
            ui.painter().add(egui::Shape::line(
                vec![r.center() + egui::vec2(-26.0, 0.0), r.center() + egui::vec2(-8.0, 19.0), r.center() + egui::vec2(29.0, -22.0)],
                egui::Stroke::new(9.0, egui::Color32::WHITE),
            ));
            ui.add_space(22.0);
            let msg = if self.resultado.restaurado {
                self.t("pronto_restaurado")
            } else if self.tipo == Tipo::Rockbox {
                self.t("pronto_rockbox")
            } else {
                self.t("pronto_system")
            };
            ui.label(egui::RichText::new(msg).size(19.0).strong());
            ui.add_space(6.0);
            let sub = if self.resultado.restaurado {
                self.t("pronto_sub_restaurado")
            } else if self.tipo == Tipo::Rockbox {
                self.t("pronto_sub_rockbox")
            } else {
                self.t("pronto_sub_system")
            };
            ui.label(egui::RichText::new(sub).size(13.5).color(estilo::CINZA));
            if self.resultado.config_mantida {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(self.t("config_mantida")).size(13.5).color(estilo::CINZA));
            }
            if self.resultado.comecou_do_zero {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(self.t("pronto_comecou")).size(13.5).color(estilo::CINZA));
            }
            let pasta = crate::aparelho::backup::dir();
            ui.add_space(4.0);
            let texto = self.t("backup_em").replace("{}", &pasta.display().to_string());
            ui.label(egui::RichText::new(texto).size(13.5).color(estilo::CINZA));
            ui.add_space(24.0);
            if ui.add(estilo::botao_azul(self.t("concluir"))).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

/// A linha da versão no cartão: a versão como vem no MANIFEST (ou no
/// índice do site) e a data. A versão do RockBox é a do build:
/// `1.0.822b9f5e99`.
fn linha_do_cartao(versao: &str, data: &str) -> String {
    if data.is_empty() {
        versao.to_string()
    } else {
        format!("{versao} · {data}")
    }
}

/// A telinha do player: o print da tela do sistema, com os cantos
/// arredondados e uma borda fina, como o aparelho na mão.
fn telinha(ui: &mut egui::Ui, tex: egui::TextureId) {
    let img = egui::Image::new((tex, egui::vec2(120.0, 150.0))).corner_radius(egui::CornerRadius::same(10));
    let r = ui.add(img);
    ui.painter().rect_stroke(r.rect, egui::CornerRadius::same(10), egui::Stroke::new(1.0, estilo::BORDA), egui::StrokeKind::Inside);
}

/// O conteúdo do cartão da tela de escolha: a telinha com o print do
/// sistema e, ao lado, o texto.
fn cartao_conteudo(
    ui: &mut egui::Ui,
    print: Option<egui::TextureId>,
    titulo: &str,
    desc: &str,
    linha: &str,
    nota: Option<&str>,
    sel: bool,
) {
    ui.horizontal(|ui| {
        if let Some(tex) = print {
            ui.vertical(|ui| {
                telinha(ui, tex);
            });
            ui.add_space(16.0);
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(titulo).size(16.0).strong());
            ui.add_space(3.0);
            ui.label(egui::RichText::new(desc).size(13.5).color(estilo::CINZA));
            if let Some(n) = nota {
                ui.add_space(3.0);
                ui.label(egui::RichText::new(n).size(12.0).color(estilo::CINZA));
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new(linha).size(12.5).color(estilo::CINZA_CLARO));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            radio(ui, sel);
        });
    });
}

fn radio(ui: &mut egui::Ui, sel: bool) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let c = r.center();
    if sel {
        ui.painter().circle_filled(c, 9.0, estilo::AZUL);
        ui.painter().circle_filled(c, 3.4, egui::Color32::WHITE);
    } else {
        ui.painter().circle_stroke(c, 8.0, egui::Stroke::new(1.5, estilo::CINZA_CLARO));
    }
}

fn passo_texto(ui: &mut egui::Ui, n: u32, texto: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 14.0, estilo::AZUL);
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            n.to_string(),
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
            egui::Color32::WHITE,
        );
        ui.add_space(10.0);
        ui.label(egui::RichText::new(texto).size(15.0));
    });
}

fn desenhar_player(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(240.0, 250.0), egui::Sense::hover());
    let p = ui.painter();
    let centro = rect.center() + egui::vec2(0.0, -6.0);
    let corpo = egui::Rect::from_center_size(centro, egui::vec2(112.0, 196.0));
    p.rect_filled(corpo, egui::CornerRadius::same(10), egui::Color32::from_rgb(0x1E, 0x1F, 0x23));
    let tela = egui::Rect::from_min_size(corpo.min + egui::vec2(20.0, 18.0), egui::vec2(72.0, 54.0));
    p.rect_filled(tela, egui::CornerRadius::same(3), egui::Color32::from_rgb(0x0A, 0x0A, 0x0C));
    let dpad = centro + egui::vec2(0.0, 32.0);
    p.circle_filled(dpad, 30.0, egui::Color32::from_rgb(0x3A, 0x3C, 0x42));
    p.circle_filled(dpad, 9.0, egui::Color32::from_rgb(0x21, 0x22, 0x26));
    let tri = vec![dpad + egui::vec2(-6.0, 12.0), dpad + egui::vec2(6.0, 12.0), dpad + egui::vec2(0.0, 21.0)];
    p.add(egui::Shape::convex_polygon(tri, estilo::AZUL, egui::Stroke::NONE));
    p.line_segment(
        [corpo.center_bottom(), corpo.center_bottom() + egui::vec2(0.0, 30.0)],
        egui::Stroke::new(3.0, egui::Color32::from_rgb(0x9A, 0xA1, 0xAA)),
    );
}

fn erro_cor() -> egui::Color32 {
    egui::Color32::from_rgb(0xC0, 0x39, 0x2B)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A altura do cartão da tela de escolha, medida sem janela: o
    /// cartão cresceu com a telinha do player e não pode empurrar o
    /// resto da tela para fora.
    fn altura_do_cartao(desc: &str, nota: Option<&str>) -> f32 {
        let ctx = egui::Context::default();
        let tex = egui::TextureId::default();
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(980.0, 780.0)));
        let mut altura = 0.0;
        let mut saida = ctx.run_ui(raw, |ctx| {
            let ret = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(980.0, 780.0));
            let construtor = egui::UiBuilder::new().max_rect(ret).layout(egui::Layout::top_down(egui::Align::Min));
            let mut ui = egui::Ui::new(ctx.clone(), egui::Id::unique("cartao"), construtor);
            ui.set_width(760.0);
            estilo::cartao(false).show(&mut ui, |ui| {
                cartao_conteudo(ui, Some(tex), "OpenPod System", desc, "1.0.822b9f5e99 · 22/11/2026", nota, false);
            });
            altura = ui.min_rect().height();
        });
        // sem janela nobody applies the textures: the delta is dropped
        // on purpose
        saida.textures_delta.clear();
        altura
    }

    #[test]
    fn o_cartao_da_escolha_cabe_na_janela() {
        for idioma in [Idioma::Pt, Idioma::En] {
            // o cartão do System e o do RockBox (com a nota da capa)
            for (desc, nota) in [(t(idioma, "system_desc"), None), (t(idioma, "rockbox_desc"), Some(t(idioma, "capa_beta")))] {
                let h = altura_do_cartao(desc, nota);
                assert!(h > 150.0, "o cartão não tem a telinha do player: {h}");
                // a tela de escolha inteira fica com ~630 px; a janela tem 780
                assert!(h < 240.0, "o cartão ficou alto demais ({h}): a tela não cabe na janela");
            }
        }
    }

    /// A altura da tela da escolha inteira, medida sem janela: com a
    /// lista de idiomas e a caixa de "começar do zero", nada fica
    /// cortado.
    fn altura_da_escolha(app: &mut App) -> f32 {
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(980.0, 780.0)));
        let mut altura = 0.0;
        let mut saida = ctx.run_ui(raw, |ctx| {
            // as telinhas do cartão, como na janela de verdade
            let img = egui::ColorImage::new([120, 150], vec![egui::Color32::BLACK; 120 * 150]);
            app.tela_system = Some(ctx.load_texture("tela-system", img.clone(), egui::TextureOptions::LINEAR));
            app.tela_rockbox = Some(ctx.load_texture("tela-rockbox", img, egui::TextureOptions::LINEAR));
            let ret = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(980.0, 780.0));
            let construtor = egui::UiBuilder::new().max_rect(ret).layout(egui::Layout::top_down(egui::Align::Min));
            let mut ui = egui::Ui::new(ctx.clone(), egui::Id::unique("escolha"), construtor);
            ui.set_width(760.0);
            app.tela_escolha(&mut ui);
            altura = ui.min_rect().height();
        });
        saida.textures_delta.clear();
        altura
    }

    fn app_de_teste(idioma: Idioma) -> App {
        App::com_backend(idioma, Box::new(crate::aparelho::sim::Sim::new(100.0)))
    }

    #[test]
    fn a_tela_da_escolha_cabe_na_janela() {
        for idioma in [Idioma::Pt, Idioma::En] {
            let mut app = app_de_teste(idioma);
            app.tipo = Tipo::Rockbox;
            app.linguas = vec!["english.lng".into(), "portugues-brasileiro.lng".into(), "deutsch.lng".into()];
            let h = altura_da_escolha(&mut app);
            // a janela tem 780 de altura (e o rodapé da tela conta)
            assert!(h < 700.0, "a tela da escolha ficou alta demais ({h}) em {idioma:?}");
        }
    }

    /// A caixa de "começar do zero" começa desmarcada, e o aviso vem antes
    /// de qualquer instalação.
    #[test]
    fn o_comecar_do_zero_pede_confirmacao() {
        let mut app = app_de_teste(Idioma::Pt);
        assert!(!app.limpar_rockbox, "a caixa começa desmarcada");
        app.tipo = Tipo::Rockbox;
        app.pacote = Some(PathBuf::from("pacote.tar.gz"));

        // sem marcar: instala direto, como sempre
        app.instalar();
        assert!(!app.confirmado_limpeza, "sem a caixa não há aviso");
        assert_eq!(app.tela, Tela::Instalando);

        // marcada: o aviso vem antes
        let mut app = app_de_teste(Idioma::Pt);
        app.tipo = Tipo::Rockbox;
        app.pacote = Some(PathBuf::from("pacote.tar.gz"));
        app.limpar_rockbox = true;
        app.instalar();
        assert!(app.confirmado_limpeza, "o aviso de apagar tem de vir");
        assert_eq!(app.tela, Tela::Escolha, "não se instala antes do aviso");
    }

    /// O comando de gravação leva o "começar do zero" (e só no RockBox).
    #[test]
    fn o_comando_traz_o_comecar_do_zero() {
        let mut app = app_de_teste(Idioma::Pt);
        app.tipo = Tipo::Rockbox;
        app.limpar_rockbox = true;
        let cmd = app.comando_gravar(PathBuf::from("pacote.tar.gz"));
        match cmd {
            Comando::Gravar { limpar_rockbox, .. } => assert!(limpar_rockbox, "o RockBox começa do zero"),
            outra => panic!("veio outra coisa: {outra:?}"),
        }
        // no OpenPod System o cartão não é tocado: a caixa não vale
        let mut app = app_de_teste(Idioma::Pt);
        app.tipo = Tipo::System;
        app.limpar_rockbox = true;
        let cmd = app.comando_gravar(PathBuf::from("pacote.tar.gz"));
        match cmd {
            Comando::Gravar { limpar_rockbox, .. } => assert!(!limpar_rockbox),
            outra => panic!("veio outra coisa: {outra:?}"),
        }
    }

    #[test]
    fn a_versao_do_cartao_vem_do_manifest() {
        // a versão do RockBox é a do build: 1.0.<compilação>
        assert_eq!(linha_do_cartao("1.0.822b9f5e99", "22/11/2026"), "1.0.822b9f5e99 · 22/11/2026");
        assert_eq!(linha_do_cartao("1.0.822b9f5e99", ""), "1.0.822b9f5e99");
        assert_eq!(linha_do_cartao("3.5", "10/12/2026"), "3.5 · 10/12/2026");
    }

    #[test]
    fn a_nota_da_capa_existe_nos_dois_idiomas() {
        assert_eq!(t(Idioma::Pt, "capa_beta"), "* Capa do álbum: em beta");
        assert_eq!(t(Idioma::En, "capa_beta"), "* Album art: in beta");
    }

    #[test]
    fn recuperacao_so_depois_do_backup_na_gravacao() {
        for fase in [Fase::Write, Fase::Check, Fase::Verify, Fase::Reboot] {
            assert!(ha_recuperacao(true, Some(fase)), "{fase:?}");
        }
        for fase in [Fase::Connect, Fase::Backup, Fase::Sync, Fase::Eject] {
            assert!(!ha_recuperacao(true, Some(fase)), "{fase:?}");
        }
        assert!(!ha_recuperacao(false, Some(Fase::Write)));
        assert!(!ha_recuperacao(true, None));
    }

    #[test]
    fn pacote_sem_assinatura_nao_entra() {
        let dir = std::env::current_dir().unwrap().join(".tmp").join(format!("app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("sem-assinatura.tar.gz");
        let f = std::fs::File::create(&p).unwrap();
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::fast()));
        let dados = b"formato=1\nlinha=rockbox\n";
        let mut h = tar::Header::new_gnu();
        h.set_size(dados.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, "MANIFEST", &dados[..]).unwrap();
        b.finish().unwrap();
        assert!(conferir_pacote(&p).is_err(), "sem ASSINATURA não pode entrar");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
