//! A aba da StorePod: o catálogo de temas, apps e jogos, o cartão do player
//! e o que está instalado nele.
//!
//! O catálogo vem do site (`storepod::URL_CATALOGO`); o cartão é o disco do
//! player ligado no OpenPod RockBox (`aparelho::sdcard`). Nada aqui toca na
//! flash nem no modo de recuperação: é só o cartão.

use crate::aparelho::sdcard;
use crate::estilo;
use crate::storepod::{self, Acao, Estado, Item, Registro};
use crate::textos::{self, Idioma};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

/// O que o trabalho da loja fez.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fim {
    Instalado,
    Removido,
    Atualizado,
}

/// O estado da aba da StorePod.
pub struct Loja {
    /// o idioma da tela (o da janela)
    idioma: Idioma,
    /// o catálogo: procurando, sem conexão ou com os itens
    pub estado: Estado,
    /// o ponto de montagem do cartão do player (quando ele aparece)
    pub cartao: Option<PathBuf>,
    /// o que está instalado no cartão
    pub registro: Registro,
    /// o trabalho em andamento
    pub ocupado: bool,
    /// o que o trabalho em andamento está fazendo
    acao: Option<Acao>,
    /// a última mensagem (instalado, removido, atualizado)
    pub aviso: Option<String>,
    /// o erro da última tentativa
    pub erro: Option<String>,
    /// os prints dos itens (id -> textura)
    prints: HashMap<String, egui::TextureHandle>,
    /// já começou a procurar o catálogo e o cartão?
    iniciada: bool,
    catalogo_rx: Option<Receiver<Option<String>>>,
    cartao_rx: Option<Receiver<Option<PathBuf>>>,
    print_rx: Option<Receiver<(String, Vec<u8>)>>,
    trabalho_rx: Option<Receiver<Result<(Fim, Registro), String>>>,
}

impl Loja {
    pub fn nova() -> Loja {
        Loja {
            idioma: Idioma::Pt,
            estado: Estado::Procurando,
            cartao: None,
            registro: Registro::new(),
            ocupado: false,
            acao: None,
            aviso: None,
            erro: None,
            prints: HashMap::new(),
            iniciada: false,
            catalogo_rx: None,
            cartao_rx: None,
            print_rx: None,
            trabalho_rx: None,
        }
    }

    /// Começa a procurar o catálogo e o cartão do player (só uma vez).
    pub fn abrir(&mut self) {
        if self.iniciada {
            return;
        }
        self.iniciada = true;
        self.estado = Estado::Procurando;

        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(storepod::buscar_catalogo());
        });
        self.catalogo_rx = Some(rx);

        // o disco do player: ele aparece e some enquanto o cabo está
        // ligado, então a vigia fica sempre de olho
        let (tx, rx) = channel();
        std::thread::spawn(move || loop {
            let achou = match sdcard::wait_card(Duration::from_secs(2), |_, _| true) {
                Ok(c) => Some(c.mount),
                Err(_) => None,
            };
            if tx.send(achou).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(400));
        });
        self.cartao_rx = Some(rx);
    }

    /// Confere o que chegou das outras threads.
    pub fn atualizar(&mut self, ctx: &egui::Context) {
        // o catálogo
        if let Some(rx) = self.catalogo_rx.take() {
            match rx.try_recv() {
                Ok(texto) => {
                    self.estado = storepod::estado(texto.as_deref());
                    if let Estado::Itens(itens) = &self.estado {
                        self.baixar_prints(itens.clone());
                    }
                }
                Err(TryRecvError::Empty) => self.catalogo_rx = Some(rx),
                Err(TryRecvError::Disconnected) => self.estado = Estado::SemConexao,
            }
        }
        // o cartão do player
        if let Some(rx) = self.cartao_rx.take() {
            match rx.try_recv() {
                Ok(montagem) => {
                    if self.cartao.as_deref() != montagem.as_deref() {
                        self.cartao = montagem;
                        self.ler_registro();
                    }
                }
                Err(TryRecvError::Empty) => self.cartao_rx = Some(rx),
                Err(TryRecvError::Disconnected) => {}
            }
        }
        // os prints
        if let Some(rx) = self.print_rx.take() {
            loop {
                match rx.try_recv() {
                    Ok((id, dados)) => {
                        if let Some(tex) = textura(ctx, &id, &dados) {
                            self.prints.insert(id, tex);
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        self.print_rx = Some(rx);
                        break;
                    }
                    Err(TryRecvError::Disconnected) => break,
                }
            }
        }
        // o trabalho (instalar, remover, atualizar)
        if let Some(rx) = self.trabalho_rx.take() {
            match rx.try_recv() {
                Ok(Ok((fim, registro))) => {
                    self.ocupado = false;
                    self.acao = None;
                    self.registro = registro;
                    self.aviso = Some(
                        match fim {
                            Fim::Instalado => "store_ok_instalado",
                            Fim::Removido => "store_ok_removido",
                            Fim::Atualizado => "store_ok_atualizado",
                        }
                        .to_string(),
                    );
                }
                Ok(Err(e)) => {
                    self.ocupado = false;
                    let acao = self.acao.take();
                    crate::log::linha("STORE", &e);
                    // o detalhe técnico (com o número E) vai para o diário
                    self.erro = Some(self.mensagem_do_erro(&e, acao));
                }
                Err(TryRecvError::Empty) => self.trabalho_rx = Some(rx),
                Err(TryRecvError::Disconnected) => self.ocupado = false,
            }
        }
    }

    /// A mensagem da tela para um erro do trabalho: a do OpenPod quando o
    /// item não é nosso, e a do trabalho (instalar ou remover) no resto.
    fn mensagem_do_erro(&self, e: &str, acao: Option<Acao>) -> String {
        let idioma = self.idioma;
        if e == crate::pacote::ERRO_ASSINATURA {
            return textos::t(idioma, "assinatura_invalida").to_string();
        }
        let chave = match acao {
            Some(Acao::Remover) => "erro_store_remover",
            _ => "erro_store_instalar",
        };
        textos::t(idioma, chave).to_string()
    }

    /// Lê o registro do cartão (o que está instalado).
    fn ler_registro(&mut self) {
        let Some(cartao) = self.cartao.clone() else { return };
        match storepod::ler_registro(&cartao) {
            Ok(r) => self.registro = r,
            Err(e) => {
                crate::log::linha("STORE", &e);
                self.registro = Registro::new();
                self.erro = Some(self.mensagem_do_erro(&e, None));
            }
        }
    }

    /// Baixa os prints dos itens (em segundo plano: a tela não espera).
    fn baixar_prints(&mut self, itens: Vec<Item>) {
        if self.print_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for item in itens {
                let Some(url) = item.print.clone() else { continue };
                if let Some(dados) = storepod::baixar_print(&url) {
                    if tx.send((item.id.clone(), dados)).is_err() {
                        return;
                    }
                }
            }
        });
        self.print_rx = Some(rx);
    }

    /// Manda o trabalho para uma thread: baixar, conferir, instalar (ou
    /// remover) no cartão.
    fn acionar(&mut self, item: &Item, acao: Acao) {
        let Some(cartao) = self.cartao.clone() else { return };
        self.ocupado = true;
        self.acao = Some(acao);
        self.erro = None;
        self.aviso = None;
        let item = item.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = (|| -> Result<(Fim, Registro), String> {
                let mut registro = storepod::ler_registro(&cartao)?;
                storepod::aplicar(&cartao, &item, &mut registro, acao)?;
                let fim = match acao {
                    Acao::Instalar => Fim::Instalado,
                    Acao::Remover => Fim::Removido,
                    Acao::Atualizar => Fim::Atualizado,
                };
                Ok((fim, registro))
            })();
            let _ = tx.send(r);
        });
        self.trabalho_rx = Some(rx);
    }

    /// A tela da StorePod.
    pub fn desenhar(&mut self, ui: &mut egui::Ui, idioma: Idioma) {
        self.idioma = idioma;
        ui.label(estilo::titulo(textos::t(idioma, "store_titulo")));
        ui.add_space(4.0);
        ui.label(estilo::subtitulo(textos::t(idioma, "store_sub")));
        ui.add_space(18.0);

        // o cartão do player
        match &self.cartao {
            Some(montagem) => {
                let texto = textos::t(idioma, "store_cartao_em").replace("{}", &montagem.display().to_string());
                ui.label(egui::RichText::new(texto).size(13.0).color(estilo::VERDE));
            }
            None => {
                ui.label(egui::RichText::new(textos::t(idioma, "store_conecte")).size(14.5).strong());
                ui.add_space(2.0);
                ui.add(egui::Label::new(egui::RichText::new(textos::t(idioma, "store_conecte_sub")).size(12.5).color(estilo::CINZA)).wrap());
            }
        }
        ui.add_space(16.0);

        // o catálogo
        match self.estado.clone() {
            Estado::Procurando => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new(textos::t(idioma, "store_procurando")).size(13.0).color(estilo::CINZA));
                });
            }
            Estado::SemConexao => {
                ui.label(egui::RichText::new(textos::t(idioma, "store_sem_conexao")).size(13.5).color(estilo::CINZA));
            }
            Estado::Itens(itens) if itens.is_empty() => {
                ui.label(egui::RichText::new(textos::t(idioma, "store_vazia")).size(13.5).color(estilo::CINZA));
            }
            Estado::Itens(itens) => {
                let mut colunas = vec![Vec::new(), Vec::new()];
                for (i, item) in itens.iter().enumerate() {
                    colunas[i % 2].push(item.clone());
                }
                ui.columns(2, |cols| {
                    for (i, coluna) in colunas.iter().enumerate() {
                        for item in coluna {
                            self.cartao_item(&mut cols[i], item, idioma);
                            cols[i].add_space(14.0);
                        }
                    }
                });
            }
        }

        if self.ocupado {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(textos::t(idioma, "store_trabalhando")).size(13.0).color(estilo::CINZA));
            });
        }
        if let Some(a) = &self.aviso {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(textos::t(idioma, a)).size(13.0).color(estilo::VERDE));
        }
        if let Some(e) = &self.erro {
            ui.add_space(6.0);
            ui.add(egui::Label::new(egui::RichText::new(e).size(12.5).color(egui::Color32::from_rgb(0xC0, 0x39, 0x2B))).wrap());
        }
    }

    /// O cartão de um item: o print, o nome, a descrição, o tipo, o tamanho
    /// e o botão (instalar, remover ou atualizar).
    fn cartao_item(&mut self, ui: &mut egui::Ui, item: &Item, idioma: Idioma) {
        let botao = storepod::botao(item, &self.registro);
        let versao = self.registro.get(&item.id).map(|i| i.versao.clone());
        let nome = item.nome.em(idioma).to_string();
        let descricao = item.descricao.em(idioma).to_string();
        let tipo = match item.tipo.as_str() {
            "tema" => textos::t(idioma, "store_tema").to_string(),
            "app" => textos::t(idioma, "store_app").to_string(),
            "jogo" => textos::t(idioma, "store_jogo").to_string(),
            outro => outro.to_string(),
        };
        let tamanho = storepod::tamanho_humano(item.tamanho, idioma);
        let versao_texto = textos::t(idioma, "store_versao").replace("{}", &item.versao);
        let rotulo = match botao {
            storepod::Botao::Instalar => textos::t(idioma, "instalar").to_string(),
            storepod::Botao::Remover => textos::t(idioma, "store_remover").to_string(),
            storepod::Botao::Atualizar => textos::t(idioma, "store_atualizar").to_string(),
            storepod::Botao::Instalado => textos::t(idioma, "store_instalado").to_string(),
        };
        let pode = self.cartao.is_some() && !self.ocupado && item.pode() && botao != storepod::Botao::Instalado;
        let print = self.prints.get(&item.id).map(|t| t.id());

        let clicou = estilo::cartao(false).inner_margin(egui::Margin::symmetric(16, 14)).show(ui, |ui| {
            ui.set_min_width(330.0);
            if let Some(tex) = print {
                ui.add(egui::Image::new((tex, egui::vec2(320.0, 140.0))));
                ui.add_space(10.0);
            }
            ui.label(egui::RichText::new(nome).size(15.5).strong());
            ui.add_space(3.0);
            if !descricao.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(descricao).size(12.5).color(estilo::CINZA)).wrap());
                ui.add_space(6.0);
            }
            let linha = format!("{tipo} · {tamanho} · {versao_texto}");
            ui.label(egui::RichText::new(linha).size(11.5).color(estilo::CINZA_CLARO));
            if let Some(v) = &versao {
                ui.add_space(2.0);
                let texto = format!("{v} · {}", textos::t(idioma, "store_ok_instalado"));
                ui.label(egui::RichText::new(texto).size(11.5).color(estilo::VERDE));
            }
            ui.add_space(12.0);
            let resposta = ui.add_enabled(pode, estilo::botao_azul(&rotulo));
            resposta.clicked()
        });
        let clicou = clicou.inner;
        if clicou {
            self.acionar(item, Acao::de(botao));
        }
        if !item.pode() {
            ui.label(egui::RichText::new(textos::t(idioma, "store_precisa_rockbox")).size(11.5).color(estilo::AVISO));
        }
    }
}

/// Uma imagem vira textura da janela.
fn textura(ctx: &egui::Context, nome: &str, dados: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(dados).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let cor = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(format!("storepod-{nome}"), cor, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::storepod::Texto;

    /// Desenha a aba da StorePod sem janela (como na tela de verdade).
    fn desenhar(loja: &mut Loja, idioma: Idioma) {
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(980.0, 780.0)));
        let mut saida = ctx.run_ui(raw, |ctx| {
            let ret = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(980.0, 780.0));
            let construtor = egui::UiBuilder::new().max_rect(ret).layout(egui::Layout::top_down(egui::Align::Min));
            let mut ui = egui::Ui::new(ctx.clone(), egui::Id::unique("loja"), construtor);
            ui.set_width(760.0);
            loja.desenhar(&mut ui, idioma);
        });
        saida.textures_delta.clear();
    }

    fn item(id: &str, versao: &str) -> Item {
        Item {
            id: id.into(),
            tipo: "tema".into(),
            nome: Texto { pt: "Um tema".into(), en: "A theme".into() },
            descricao: Texto { pt: "A descrição".into(), en: "The description".into() },
            versao: versao.into(),
            tamanho: 12345,
            ..Default::default()
        }
    }

    /// A tela da StorePod: sem conexão, catálogo vazio e com itens.
    #[test]
    fn a_tela_da_loja_desenha_nos_dois_idiomas() {
        for idioma in [Idioma::Pt, Idioma::En] {
            let mut loja = Loja::nova();
            // o player ainda não apareceu: a tela pede a conexão
            assert!(loja.cartao.is_none());
            loja.estado = Estado::Procurando;
            desenhar(&mut loja, idioma);
            // sem conexão com a StorePod
            loja.estado = Estado::SemConexao;
            desenhar(&mut loja, idioma);
            // o catálogo veio, mas está vazio
            loja.estado = Estado::Itens(vec![]);
            desenhar(&mut loja, idioma);
            // com itens: os cartões saem
            loja.estado = Estado::Itens(vec![item("tema-a", "1.0"), item("tema-b", "2.0")]);
            desenhar(&mut loja, idioma);
        }
    }

    /// O botão do item segue o que está instalado no cartão.
    #[test]
    fn o_botao_do_item_segue_o_registro() {
        let mut loja = Loja::nova();
        let item = item("tema-a", "1.0");
        loja.estado = Estado::Itens(vec![item.clone()]);
        // sem cartão: nada instalado, o botão é "Instalar"
        assert_eq!(storepod::botao(&item, &loja.registro), storepod::Botao::Instalar);
        // instalado na mesma versão: "Remover"
        loja.registro.insert(
            "tema-a".into(),
            storepod::Instalado { versao: "1.0".into(), arquivos: vec![".rockbox/themes/a.cfg".into()] },
        );
        assert_eq!(storepod::botao(&item, &loja.registro), storepod::Botao::Remover);
        // instalado numa versão antiga: "Atualizar"
        loja.registro.insert(
            "tema-a".into(),
            storepod::Instalado { versao: "0.9".into(), arquivos: vec![".rockbox/themes/a.cfg".into()] },
        );
        assert_eq!(storepod::botao(&item, &loja.registro), storepod::Botao::Atualizar);
    }
}
