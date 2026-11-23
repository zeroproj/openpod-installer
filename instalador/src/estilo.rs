//! O visual do instalador: claro, premium, cartões arredondados, botão azul.
//! O modelo é `docs/openpod/ideia_instalador.png`.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, TextStyle};

pub const AZUL: Color32 = Color32::from_rgb(0x16, 0x73, 0xE6);
pub const FUNDO: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF8);
pub const CARTAO: Color32 = Color32::WHITE;
pub const TEXTO: Color32 = Color32::from_rgb(0x1B, 0x1F, 0x24);
pub const CINZA: Color32 = Color32::from_rgb(0x6B, 0x72, 0x7B);
pub const CINZA_CLARO: Color32 = Color32::from_rgb(0x9A, 0xA1, 0xAA);
pub const BORDA: Color32 = Color32::from_rgb(0xE2, 0xE6, 0xEB);
pub const VERDE: Color32 = Color32::from_rgb(0x22, 0xB0, 0x5E);
pub const VERDE_FUNDO: Color32 = Color32::from_rgb(0xE9, 0xF8, 0xEF);
pub const AVISO: Color32 = Color32::from_rgb(0xB2, 0x6A, 0x00);

pub const INTER: &[u8] = include_bytes!("../assets/Inter.ttf");
pub const LOGO: &[u8] = include_bytes!("../assets/OpenLogo.png");
/// O print da tela inicial do OpenPod System (384×480, a tela do player).
pub const TELA_SYSTEM: &[u8] = include_bytes!("../assets/tela-system.png");
/// O print do menu do OpenPod RockBox com o tema OpenPod128.
pub const TELA_ROCKBOX: &[u8] = include_bytes!("../assets/tela-rockbox.png");

pub fn instalar_fontes(ctx: &egui::Context) {
    let mut fontes = FontDefinitions::default();
    fontes.font_data.insert("inter".into(), std::sync::Arc::new(FontData::from_static(INTER)));
    for familia in [FontFamily::Proportional, FontFamily::Monospace] {
        fontes.families.entry(familia).or_default().insert(0, "inter".into());
    }
    ctx.set_fonts(fontes);
}

pub fn aplicar_tema(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Light);
    let mut estilo = (*ctx.style_of(egui::Theme::Light)).clone();
    estilo.visuals = egui::Visuals::light();
    let v = &mut estilo.visuals;
    v.panel_fill = FUNDO;
    v.window_fill = FUNDO;
    v.extreme_bg_color = Color32::WHITE;
    v.override_text_color = Some(TEXTO);
    v.hyperlink_color = AZUL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXTO);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXTO);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXTO);
    v.selection.bg_fill = AZUL.gamma_multiply(0.25);
    estilo.spacing.item_spacing = egui::vec2(10.0, 10.0);
    estilo.spacing.button_padding = egui::vec2(16.0, 9.0);
    estilo.text_styles = [
        (TextStyle::Heading, egui::FontId::new(30.0, FontFamily::Proportional)),
        (TextStyle::Body, egui::FontId::new(14.5, FontFamily::Proportional)),
        (TextStyle::Button, egui::FontId::new(14.5, FontFamily::Proportional)),
        (TextStyle::Small, egui::FontId::new(12.5, FontFamily::Proportional)),
        (TextStyle::Monospace, egui::FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_style_of(egui::Theme::Light, estilo);
}

pub fn cartao(selecionado: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(CARTAO)
        .corner_radius(CornerRadius::same(16))
        .stroke(Stroke::new(if selecionado { 2.0 } else { 1.0 }, if selecionado { AZUL } else { BORDA }))
        .inner_margin(egui::Margin::symmetric(18, 16))
}

pub fn botao_azul(texto: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(texto.to_string()).color(Color32::WHITE).size(15.0).strong())
        .fill(AZUL)
        .corner_radius(CornerRadius::same(10))
        .min_size(egui::vec2(150.0, 42.0))
}

pub fn link(texto: &str) -> egui::RichText {
    egui::RichText::new(texto.to_string()).color(AZUL).size(13.5)
}

pub fn titulo(texto: &str) -> egui::RichText {
    egui::RichText::new(texto.to_string()).size(31.0).strong().color(TEXTO)
}

pub fn subtitulo(texto: &str) -> egui::RichText {
    egui::RichText::new(texto.to_string()).size(14.0).color(CINZA)
}

/// * feito: verde com check; atual: anel azul; pendente: anel cinza.
pub fn passo(ui: &mut egui::Ui, estado: EstadoPasso, numero: Option<u32>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    match estado {
        EstadoPasso::Feito => {
            p.circle_filled(c, 11.0, VERDE);
            p.add(egui::Shape::line(
                vec![c + egui::vec2(-4.5, 0.0), c + egui::vec2(-1.0, 3.6), c + egui::vec2(5.0, -3.8)],
                Stroke::new(2.0, Color32::WHITE),
            ));
        }
        EstadoPasso::Atual => {
            p.circle_stroke(c, 9.5, Stroke::new(2.4, AZUL));
            p.circle_stroke(c, 4.0, Stroke::new(4.0, AZUL));
        }
        EstadoPasso::Pendente => {
            p.circle_stroke(c, 9.5, Stroke::new(1.6, Color32::from_rgb(0xD5, 0xDA, 0xE0)));
        }
    }
    if let Some(n) = numero {
        let p = ui.painter();
        p.circle_filled(c, 12.0, AZUL);
        p.text(c, egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::new(13.0, FontFamily::Proportional), Color32::WHITE);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EstadoPasso {
    Feito,
    Atual,
    Pendente,
}

/// Uma barra de progresso fina, azul.
pub fn barra(ui: &mut egui::Ui, fracao: f32, largura: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(largura, 8.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(0xE6, 0xE9, 0xED));
    let w = (rect.width() * fracao.clamp(0.0, 1.0)).max(if fracao > 0.0 { 8.0 } else { 0.0 });
    if w > 0.0 {
        let mut r = rect;
        r.set_width(w);
        p.rect_filled(r, CornerRadius::same(4), AZUL);
    }
}
