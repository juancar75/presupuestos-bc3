// SPDX-License-Identifier: GPL-3.0-or-later
//! Panel del asistente: conversación por obra con la IA, con adjuntos
//! (ofertas de fabricantes, tarifas, mediciones) e informes.
//!
//! Dos modos:
//! - **Encargo por MCP** (gratis): el mensaje queda pendiente y la IA del
//!   usuario (Claude Desktop con el servidor `presupuestos`) lo atiende al
//!   decirle «atiende el asistente».
//! - **Directo**: el programa llama a la API de Anthropic con la clave del
//!   usuario, en segundo plano, con límite de gasto mensual.
//!
//! En los dos casos la IA solo hace propuestas (que se aceptan o rechazan en
//! «Propuestas IA») e informes: no cambia el presupuesto.

use super::Aplicacion;
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_asistente::{Ajustes, Evento, Gasto, Modo, Proveedor, Respuesta, Rutas};
use ppto_core::Decimal;
use ppto_propuestas::conversacion::{Autor, Conversaciones, carpeta_informes};
use ppto_propuestas::{ObraActiva, clave};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime};

enum Aviso {
    Evento(Evento),
    Fin(Result<Respuesta, String>),
}

/// Consulta en curso en el modo directo.
struct Trabajo {
    rx: Receiver<Aviso>,
    inicio: Instant,
    estado: String,
}

pub(super) struct PanelAsistente {
    pub abierto: bool,
    /// `None` en las pruebas: no se lee ni se escribe nada del disco.
    rutas: Option<Rutas>,
    ruta_ajustes: Option<PathBuf>,
    conv: Conversaciones,
    mtime: Option<SystemTime>,
    ultima_lectura: Option<Instant>,
    pub entrada: String,
    pub adjuntos: Vec<String>,
    ajustes: Ajustes,
    ver_ajustes: bool,
    trabajo: Option<Trabajo>,
    error: Option<String>,
}

impl PanelAsistente {
    pub fn sin_disco() -> Self {
        Self {
            abierto: false,
            rutas: None,
            ruta_ajustes: None,
            conv: Conversaciones::default(),
            mtime: None,
            ultima_lectura: None,
            entrada: String::new(),
            adjuntos: Vec::new(),
            ajustes: Ajustes::default(),
            ver_ajustes: false,
            trabajo: None,
            error: None,
        }
    }

    pub fn en_disco() -> Self {
        let ruta_ajustes = Ajustes::ruta();
        let ajustes = ruta_ajustes.as_deref().map(Ajustes::cargar).unwrap_or_default();
        Self {
            rutas: Rutas::de_config(),
            ruta_ajustes,
            ajustes,
            ..Self::sin_disco()
        }
    }

    /// Relee la conversación si el fichero cambió (cada 2 s como mucho).
    fn releer(&mut self, forzar: bool) {
        let Some(r) = &self.rutas else { return };
        let ahora = Instant::now();
        if !forzar && self.ultima_lectura.is_some_and(|t| ahora - t < Duration::from_secs(2)) {
            return;
        }
        self.ultima_lectura = Some(ahora);
        let m = std::fs::metadata(&r.conversaciones).and_then(|m| m.modified()).ok();
        if forzar || m != self.mtime {
            self.mtime = m;
            self.conv = Conversaciones::cargar(&r.conversaciones);
        }
    }

    fn gasto_mes(&self) -> Decimal {
        self.rutas
            .as_ref()
            .map(|r| Gasto::cargar(&r.gasto).del_mes(&ppto_asistente::mes_actual()))
            .unwrap_or_default()
    }

    /// Recoge los avisos de la consulta en curso.
    fn atender_avisos(&mut self) {
        let Some(t) = &mut self.trabajo else { return };
        loop {
            match t.rx.try_recv() {
                Ok(Aviso::Evento(Evento::Pensando)) => t.estado = "pensando".into(),
                Ok(Aviso::Evento(Evento::Herramienta(h))) => t.estado = format!("usando «{h}»"),
                Ok(Aviso::Fin(r)) => {
                    self.error = r.err();
                    self.trabajo = None;
                    self.releer(true);
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.trabajo = None;
                    self.releer(true);
                    return;
                }
            }
        }
    }

    /// Mensaje del usuario: se guarda en la conversación y, en el modo
    /// directo, se lanza la consulta en segundo plano.
    pub fn enviar(&mut self, p: &ppto_core::Presupuesto) -> Result<(), String> {
        let texto = self.entrada.trim().to_owned();
        if texto.is_empty() && self.adjuntos.is_empty() {
            return Ok(());
        }
        let obra = clave(p);
        let adjuntos = std::mem::take(&mut self.adjuntos);
        let texto = if texto.is_empty() {
            "Revisa los adjuntos.".to_owned()
        } else {
            texto
        };
        match &self.rutas {
            Some(r) => {
                Conversaciones::modificar(&r.conversaciones, |c| c.anadir(&obra, Autor::Usuario, &texto, adjuntos))
                    .map_err(|e| e.to_string())?;
            }
            None => {
                self.conv.anadir(&obra, Autor::Usuario, &texto, adjuntos);
            }
        }
        self.entrada.clear();
        self.error = None;
        self.releer(true);
        if self.ajustes.modo == Modo::Directo {
            self.lanzar(p)?;
        }
        Ok(())
    }

    /// Modo directo: atiende en segundo plano los mensajes pendientes de la obra.
    pub fn lanzar(&mut self, p: &ppto_core::Presupuesto) -> Result<(), String> {
        let obra = clave(p);
        if self.trabajo.is_some() {
            return Ok(());
        }
        if let Some(r) = self.rutas.clone() {
            let clave_api = self
                .ajustes
                .clave()
                .ok_or("falta la clave de la API: póngala en Ajustes del asistente")?;
            // La IA lee la obra tal como está ahora.
            ObraActiva::guardar(p, &r.obra_activa).map_err(|e| e.to_string())?;
            let ajustes = self.ajustes.clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let http = ppto_asistente::Http::para(&ajustes, clave_api);
                let tx2 = tx.clone();
                let r = ppto_asistente::atender_pendiente(&http, &ajustes, &r, &obra, &mut |e| {
                    let _ = tx2.send(Aviso::Evento(e));
                });
                let _ = tx.send(Aviso::Fin(r));
            });
            self.trabajo = Some(Trabajo {
                rx,
                inicio: Instant::now(),
                estado: "enviando".into(),
            });
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn mensajes(&self, obra: &str) -> Vec<ppto_propuestas::conversacion::Mensaje> {
        self.conv.de_obra(obra).into_iter().cloned().collect()
    }

    fn guardar_ajustes(&mut self) {
        if let Some(r) = &self.ruta_ajustes
            && let Err(e) = self.ajustes.guardar(r)
        {
            self.error = Some(format!("no se pudieron guardar los ajustes: {e}"));
        }
    }
}

/// Abre un fichero o carpeta con el programa del sistema.
pub(super) fn abrir_en_sistema(ruta: &Path) {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(ruta).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(ruta).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(ruta).spawn();
    let _ = r;
}

fn hora(segundos: u64) -> String {
    // Hora UTC del mensaje (hh:mm); suficiente para seguir la conversación.
    let s = segundos % 86_400;
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

fn nombre(ruta: &str) -> String {
    Path::new(ruta)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| ruta.to_owned())
}

impl Aplicacion {
    /// Panel lateral derecho. Se llama antes del panel central.
    pub(super) fn panel_asistente(&mut self, ui: &mut Ui) {
        if !self.asistente.abierto {
            return;
        }
        self.asistente.atender_avisos();
        self.asistente.releer(false);
        if self.asistente.trabajo.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        egui::Panel::right("asistente")
            .resizable(true)
            .default_size(400.0)
            .min_size(300.0)
            .show(ui, |ui| self.contenido_asistente(ui));
    }

    fn contenido_asistente(&mut self, ui: &mut Ui) {
        let obra = clave(&self.p);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading("✨ Asistente");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cerrar").clicked() {
                    self.asistente.abierto = false;
                }
                ui.toggle_value(&mut self.asistente.ver_ajustes, "Ajustes");
            });
        });
        ui.label(RichText::new(&obra).weak());
        let a = &mut self.asistente;
        let antes = a.ajustes.modo;
        ui.horizontal(|ui| {
            ui.radio_value(&mut a.ajustes.modo, Modo::Encargo, "Encargo por MCP")
                .on_hover_text("Gratis con su suscripción: el mensaje queda pendiente y se lo pide a Claude Desktop");
            ui.radio_value(&mut a.ajustes.modo, Modo::Directo, "Directo (API)")
                .on_hover_text("El programa llama a la API con su clave; se paga por uso");
        });
        if a.ajustes.modo != antes {
            a.guardar_ajustes();
        }
        if a.ver_ajustes {
            ajustes(ui, a);
        }
        // Informes de la obra
        let carpeta = a.rutas.as_ref().map(|r| carpeta_informes(&r.conversaciones, &obra));
        if let Some(c) = carpeta {
            let informes: Vec<PathBuf> = std::fs::read_dir(&c)
                .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
                .unwrap_or_default();
            if !informes.is_empty() {
                ui.collapsing(format!("Informes ({})", informes.len()), |ui| {
                    for f in &informes {
                        if ui.link(nombre(&f.display().to_string())).clicked() {
                            abrir_en_sistema(f);
                        }
                    }
                    if ui.small_button("Abrir carpeta").clicked() {
                        abrir_en_sistema(&c);
                    }
                });
            }
        }
        ui.separator();

        // Entrada (abajo) y conversación (arriba, con desplazamiento).
        let ocupado = a.trabajo.is_some();
        let mut enviar = false;
        egui::Panel::bottom("asistente_entrada")
            .show_separator_line(false)
            .show(ui, |ui| {
                if let Some(t) = &a.trabajo {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{} · {} s", t.estado, t.inicio.elapsed().as_secs()));
                    });
                }
                if let Some(e) = &a.error {
                    ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(220, 80, 60)));
                }
                let mut quitar = None;
                for (i, f) in a.adjuntos.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Adjunto: {}", nombre(f))).small());
                        if ui.small_button("x").on_hover_text("Quitar").clicked() {
                            quitar = Some(i);
                        }
                    });
                }
                if let Some(i) = quitar {
                    a.adjuntos.remove(i);
                }
                let r = ui.add(
                    egui::TextEdit::multiline(&mut a.entrada)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("Describa la obra o pida algo: «valora las partidas de climatización con la oferta adjunta», «resume el presupuesto para el cliente»…"),
                );
                let ctrl_intro = r.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui
                        .button("Adjuntar…")
                        .on_hover_text("Ofertas de fabricantes, tarifas, mediciones (PDF, Excel, CSV, imágenes)")
                        .clicked()
                        && let Some(fs) = rfd::FileDialog::new()
                            .add_filter("Documentos", &["pdf", "xlsx", "xls", "xlsm", "ods", "csv", "txt", "png", "jpg", "jpeg"])
                            .pick_files()
                    {
                        a.adjuntos.extend(fs.into_iter().map(|f| f.display().to_string()));
                    }
                    let puede = !ocupado && (!a.entrada.trim().is_empty() || !a.adjuntos.is_empty());
                    if ui
                        .add_enabled(puede, egui::Button::new(RichText::new("Enviar").strong()))
                        .on_hover_text("Ctrl+Intro")
                        .clicked()
                        || (puede && ctrl_intro)
                    {
                        enviar = true;
                    }
                });
                ui.add_space(4.0);
            });
        if enviar && let Err(e) = self.asistente.enviar(&self.p) {
            self.asistente.error = Some(e);
        }

        let mut lanzar = false;
        let a = &mut self.asistente;
        let mensajes: Vec<_> = a.conv.de_obra(&obra).into_iter().cloned().collect();
        let pendientes = mensajes
            .iter()
            .filter(|m| m.autor == Autor::Usuario && !m.atendido)
            .count();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if mensajes.is_empty() {
                    ui.label(RichText::new(ayuda(a.ajustes.modo)).weak());
                }
                for m in &mensajes {
                    let (quien, color) = match m.autor {
                        Autor::Usuario => ("Usted", ui.visuals().hyperlink_color),
                        Autor::Asistente => ("Asistente", Color32::from_rgb(80, 175, 110)),
                        Autor::Sistema => ("Programa", Color32::from_rgb(220, 80, 60)),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(quien).strong().color(color));
                        ui.label(RichText::new(hora(m.creado)).small().weak());
                        if m.autor == Autor::Usuario && !m.atendido {
                            ui.label(
                                RichText::new("pendiente")
                                    .small()
                                    .color(Color32::from_rgb(220, 150, 40)),
                            );
                        }
                    });
                    ui.add(egui::Label::new(&m.texto).wrap().selectable(true));
                    for f in &m.adjuntos {
                        if ui
                            .link(RichText::new(format!("Adjunto: {}", nombre(f))).small())
                            .clicked()
                        {
                            abrir_en_sistema(Path::new(f));
                        }
                    }
                    if let Some(c) = &m.coste {
                        ui.label(RichText::new(c).small().weak());
                    }
                    ui.add_space(6.0);
                }
                if pendientes > 0
                    && a.ajustes.modo == Modo::Directo
                    && a.trabajo.is_none()
                    && ui
                        .small_button("Reintentar")
                        .on_hover_text("Volver a enviarlo a la API")
                        .clicked()
                {
                    lanzar = true;
                }
                if pendientes > 0 && a.ajustes.modo == Modo::Encargo && a.trabajo.is_none() {
                    ui.label(
                        RichText::new(
                            "Pendiente de la IA. En Claude Desktop (con el servidor «presupuestos») escriba: \
                             «atiende el asistente». La respuesta y las propuestas aparecerán aquí.",
                        )
                        .color(Color32::from_rgb(220, 150, 40)),
                    );
                    if a.rutas.is_some()
                        && ui
                            .small_button("Atender ahora por la API")
                            .on_hover_text("Cambia al modo directo y lo envía con su clave de API (de pago)")
                            .clicked()
                    {
                        a.ajustes.modo = Modo::Directo;
                        a.guardar_ajustes();
                        lanzar = true;
                    }
                }
            });
        if lanzar && let Err(e) = self.asistente.lanzar(&self.p) {
            self.asistente.error = Some(e);
        }
    }
}

fn ayuda(modo: Modo) -> &'static str {
    match modo {
        Modo::Encargo => {
            "Escriba lo que necesita (y adjunte ofertas o tarifas). El mensaje queda pendiente: \
             en Claude Desktop diga «atiende el asistente» y la IA leerá la obra, hará propuestas \
             (que usted acepta en Propuestas IA) y contestará aquí. Sin coste adicional."
        }
        Modo::Directo => {
            "Escriba lo que necesita (y adjunte ofertas o tarifas). El programa consulta a la IA \
             con su clave de API: hará propuestas (que usted acepta en Propuestas IA), guardará \
             informes y contestará aquí con el coste de cada respuesta."
        }
    }
}

fn ajustes(ui: &mut Ui, a: &mut PanelAsistente) {
    let gasto = a.gasto_mes();
    let mut cambio = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::Grid::new("ajustes_asistente").num_columns(2).show(ui, |ui| {
            ui.label("Servicio de IA");
            let mut prov = a.ajustes.proveedor;
            egui::ComboBox::from_id_salt("proveedor_asistente")
                .selected_text(prov.nombre())
                .show_ui(ui, |ui| {
                    for p in [Proveedor::Anthropic, Proveedor::OpenAi, Proveedor::Compatible] {
                        ui.selectable_value(&mut prov, p, p.nombre());
                    }
                });
            if prov != a.ajustes.proveedor {
                a.ajustes.cambiar_proveedor(prov);
                cambio = true;
            }
            ui.end_row();
            if a.ajustes.proveedor == Proveedor::Compatible {
                ui.label("Dirección de la API");
                cambio |= ui
                    .add(
                        egui::TextEdit::singleline(&mut a.ajustes.url_base)
                            .desired_width(240.0)
                            .hint_text("http://localhost:11434/v1"),
                    )
                    .on_hover_text(
                        "Ollama: http://localhost:11434/v1 · LM Studio: http://localhost:1234/v1 · \
                         Gemini: https://generativelanguage.googleapis.com/v1beta/openai",
                    )
                    .lost_focus();
                ui.end_row();
            }
            ui.label("Clave de la API");
            let r = ui
                .add(
                    egui::TextEdit::singleline(&mut a.ajustes.clave_api)
                        .password(true)
                        .desired_width(200.0),
                )
                .on_hover_text(
                    "Se guarda solo en este equipo. Vacía: se usa la variable ANTHROPIC_API_KEY u OPENAI_API_KEY",
                );
            if r.changed() {
                // Una clave sk-ant-… es de Claude y una sk-… de ChatGPT: se
                // elige el servicio solo (salvo en «compatible»).
                if a.ajustes.proveedor != Proveedor::Compatible
                    && let Some(p) = Proveedor::de_clave(&a.ajustes.clave_api)
                {
                    a.ajustes.cambiar_proveedor(p);
                }
            }
            cambio |= r.lost_focus();
            ui.end_row();
            ui.label("Modelo");
            ui.horizontal(|ui| {
                cambio |= ui
                    .add(egui::TextEdit::singleline(&mut a.ajustes.modelo).desired_width(160.0))
                    .lost_focus();
                let modelos = a.ajustes.proveedor.modelos();
                if !modelos.is_empty() {
                    egui::ComboBox::from_id_salt("modelo_asistente")
                        .selected_text("elegir")
                        .show_ui(ui, |ui| {
                            for m in modelos {
                                cambio |= ui
                                    .selectable_value(&mut a.ajustes.modelo, (*m).to_owned(), *m)
                                    .changed();
                            }
                        });
                }
            });
            ui.end_row();
            for (texto, valor) in [
                ("Límite mensual ($)", &mut a.ajustes.limite_mensual),
                ("$ / millón tokens entrada", &mut a.ajustes.precio_entrada),
                ("$ / millón tokens salida", &mut a.ajustes.precio_salida),
            ] {
                ui.label(texto);
                let mut t = ppto_core::formato::num(*valor, 2);
                let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(80.0));
                if r.changed()
                    && let Some(v) = ppto_core::formato::leer(&t)
                    && v >= Decimal::ZERO
                {
                    *valor = v;
                }
                cambio |= r.lost_focus();
                ui.end_row();
            }
            ui.label("Gastado este mes");
            ui.label(format!(
                "{} de {}",
                ppto_asistente::dolares(gasto),
                ppto_asistente::dolares(a.ajustes.limite_mensual)
            ));
            ui.end_row();
        });
        if let Some(av) = a.ajustes.aviso_clave() {
            ui.colored_label(Color32::from_rgb(230, 150, 60), av);
        }
        let precios = match a.ajustes.proveedor {
            Proveedor::Anthropic => "Compruebe los precios del modelo en anthropic.com/pricing; cambian con el tiempo.",
            Proveedor::OpenAi => "Compruebe los precios del modelo en openai.com/api/pricing; cambian con el tiempo.",
            Proveedor::Compatible => "Ponga los precios de su servicio (0 si es un modelo local).",
        };
        ui.label(RichText::new(precios).small().weak());
    });
    if cambio {
        a.guardar_ajustes();
    }
}
