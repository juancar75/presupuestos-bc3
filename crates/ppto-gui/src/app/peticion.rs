// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana **Petición de precios a proveedores** (Informes).
//!
//! Lo que está sin precio en la obra, de dos maneras:
//! - **Por proveedor**: cada fabricante con sus líneas (por su tarifa o el
//!   origen del precio). Lo que no tiene proveedor se marca y se asigna.
//! - **Por tipo de producto**: recuperadores, rejillas… para pedir lo mismo
//!   a varias marcas y comparar; los fabricantes que suministran cada tipo
//!   se recuerdan en la biblioteca.
//!
//! A cada proveedor solo le llega su alcance, más las **variantes** que el
//! técnico añada del árbol (partidas que pueden tener ya precio, para que
//! las valore como alternativa). Cada petición sale en su Excel y se abre
//! como correo en Outlook, sin enviar.

use super::correo::{self, Correo};
use super::pdf::ruta_perfiles;
use super::{Accion, Aplicacion, corto};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_biblioteca::Biblioteca;
use ppto_core::formato::cant;
use ppto_core::peticion::{self, LineaPeticion, Peticion};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc;

const NARANJA: Color32 = Color32::from_rgb(230, 150, 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Vista {
    Proveedor,
    Tipo,
}

pub(super) struct VentanaPeticion {
    pub(super) peticiones: Vec<Peticion>,
    pub(super) vista: Vista,
    /// Proveedor (nombre de la petición) o tipo elegido.
    elegido: String,
    /// Líneas marcadas para asignarles proveedor.
    marcadas: BTreeSet<String>,
    asignar: String,
    /// Variantes añadidas, por «P:proveedor» o «T:tipo».
    variantes: BTreeMap<String, Vec<LineaPeticion>>,
    /// Destinatarios elegidos de cada tipo.
    destinatarios: BTreeMap<String, BTreeSet<String>>,
    otro: String,
    recordar: bool,
    fecha: String,
    obra: String,
    referencia: String,
    plazo: String,
    observaciones: String,
    bib: Biblioteca,
    mensaje: Option<(String, bool)>,
    ultimo: Option<PathBuf>,
    pub(super) recalcular: bool,
    pub(super) envio: Option<mpsc::Receiver<Result<String, String>>>,
}

fn limpio(s: &str) -> String {
    s.chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '-' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

impl VentanaPeticion {
    fn clave(&self) -> String {
        match self.vista {
            Vista::Proveedor => format!("P:{}", self.elegido),
            Vista::Tipo => format!("T:{}", self.elegido),
        }
    }

    fn peticion(&self, nombre: &str) -> Option<&Peticion> {
        self.peticiones.iter().find(|x| x.nombre() == nombre)
    }

    /// Fabricantes candidatos para un tipo: los que lo suministran según la
    /// biblioteca, los que ya tienen líneas de ese tipo y los añadidos.
    fn candidatos(&self, tipo: &str) -> Vec<String> {
        let mut v: BTreeSet<String> = self
            .bib
            .proveedores_de(tipo)
            .into_iter()
            .map(|n| ppto_core::descuentos::normalizar(&n))
            .collect();
        for x in &self.peticiones {
            if !x.proveedor.is_empty() && x.lineas.iter().any(|l| l.tipo == tipo) {
                v.insert(x.proveedor.clone());
            }
        }
        if let Some(d) = self.destinatarios.get(tipo) {
            v.extend(d.iter().cloned());
        }
        v.into_iter().collect()
    }

    fn email(&self, proveedor: &str) -> String {
        self.bib
            .fabricante(proveedor)
            .map(|f| f.datos.email.trim().to_owned())
            .unwrap_or_default()
    }

    fn datos_proveedor(&self, nombre: &str) -> Vec<(String, String)> {
        let Some(f) = self.bib.fabricante(nombre) else {
            return Vec::new();
        };
        let d = &f.datos;
        [
            ("Razón social", &d.razon_social),
            ("Contacto", &d.contacto),
            ("Email", &d.email),
        ]
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k.to_owned(), v.clone()))
        .collect()
    }

    fn cuerpo(&self, alcance: &str, hay_variantes: bool) -> String {
        let mut s =
            String::from("Buenos días:\n\nLes rogamos nos envíen su mejor oferta para lo indicado en el Excel adjunto");
        if !alcance.is_empty() {
            s.push_str(&format!(" ({})", alcance.to_lowercase()));
        }
        if !self.obra.trim().is_empty() {
            s.push_str(&format!(" para la obra «{}»", self.obra.trim()));
        }
        if !self.referencia.trim().is_empty() {
            s.push_str(&format!(", nuestra referencia {}", self.referencia.trim()));
        }
        s.push_str(".\n\nPueden rellenar directamente las celdas amarillas: neto por unidad (o PVP y descuento), plazo de entrega y, si proponen otra marca o modelo, cuál.");
        if hay_variantes {
            s.push_str(" Las líneas del apartado «Variantes» son alternativas: valórenlas aparte.");
        }
        if !self.plazo.trim().is_empty() {
            s.push_str(&format!(
                "\n\nLes agradeceríamos la respuesta antes del {}.",
                self.plazo.trim()
            ));
        }
        if !self.observaciones.trim().is_empty() {
            s.push_str(&format!("\n\n{}", self.observaciones.trim()));
        }
        s.push_str("\n\nUn saludo.\n");
        s
    }
}

impl Aplicacion {
    pub(super) fn abrir_peticion(&mut self) {
        let bib = ppto_biblioteca::ruta()
            .as_deref()
            .map(Biblioteca::cargar)
            .unwrap_or_default();
        let obra = self
            .obra_textos
            .get("titulo")
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.p.nombre.clone());
        let mut v = self.ventana_peticion.take().unwrap_or_else(|| VentanaPeticion {
            peticiones: Vec::new(),
            vista: Vista::Proveedor,
            elegido: String::new(),
            marcadas: BTreeSet::new(),
            asignar: String::new(),
            variantes: BTreeMap::new(),
            destinatarios: BTreeMap::new(),
            otro: String::new(),
            recordar: true,
            fecha: ppto_pdf::fecha_hoy(),
            obra,
            referencia: String::new(),
            plazo: String::new(),
            observaciones: String::new(),
            bib: Biblioteca::default(),
            mensaje: None,
            ultimo: None,
            recalcular: false,
            envio: None,
        });
        v.bib = bib;
        self.calcular_peticion(&mut v);
        self.ventana_peticion = Some(v);
    }

    fn calcular_peticion(&self, v: &mut VentanaPeticion) {
        match peticion::peticiones(&self.p) {
            Ok(x) => v.peticiones = x,
            Err(e) => v.mensaje = Some((format!("No se puede calcular: {e}"), true)),
        }
        v.recalcular = false;
    }

    /// Carpeta donde se guardan las peticiones: «04 Ofertas\Peticiones de
    /// precio» de la carpeta de documentos de la obra, o una temporal.
    fn carpeta_peticiones(&self) -> PathBuf {
        let docs = self
            .obra_textos
            .get("documentos")
            .map(|d| PathBuf::from(d.trim()))
            .filter(|d| d.is_dir());
        match docs {
            Some(d) => d.join("04 Ofertas").join("Peticiones de precio"),
            None => std::env::temp_dir().join("presupuestos-bc3").join("peticiones"),
        }
    }

    /// Prepara un Excel por destinatario con `lineas` y abre los correos.
    pub(super) fn enviar_peticiones(&self, v: &mut VentanaPeticion, envios: Vec<(String, Vec<LineaPeticion>, String)>) {
        let carpeta = self.carpeta_peticiones();
        if let Err(e) = std::fs::create_dir_all(&carpeta) {
            v.mensaje = Some((format!("No se puede crear {}: {e}", carpeta.display()), true));
            return;
        }
        let perfil = ruta_perfiles()
            .map(|r| ppto_pdf::Perfiles::cargar(&r))
            .unwrap_or_default()
            .activo()
            .clone();
        let mut correos = Vec::new();
        let mut sin_email = Vec::new();
        for (dest, lineas, alcance) in envios {
            if lineas.is_empty() {
                continue;
            }
            let cab = ppto_informes::CabeceraPeticion {
                empresa: perfil.empresa.clone(),
                empresa_datos: perfil.empresa_datos.clone(),
                destinatario: dest.clone(),
                alcance: alcance.clone(),
                obra: v.obra.trim().to_owned(),
                referencia: v.referencia.trim().to_owned(),
                fecha: v.fecha.trim().to_owned(),
                plazo: v.plazo.trim().to_owned(),
                proveedor: v.datos_proveedor(&dest),
                observaciones: v.observaciones.trim().to_owned(),
            };
            let bytes = match ppto_informes::excel_peticion(&lineas, &cab, self.p.decimales.medicion) {
                Ok(b) => b,
                Err(e) => {
                    v.mensaje = Some((format!("No se pudo crear el Excel de {dest}: {e}"), true));
                    return;
                }
            };
            let nombre = if alcance.is_empty() {
                format!("Petición de precios {dest} - {}.xlsx", v.obra.trim())
            } else {
                format!("Petición de precios {alcance} - {dest} - {}.xlsx", v.obra.trim())
            };
            let ruta = carpeta.join(limpio(&nombre));
            if let Err(e) = std::fs::write(&ruta, bytes) {
                v.mensaje = Some((format!("No se pudo guardar {}: {e}", ruta.display()), true));
                return;
            }
            let para = v.email(&dest);
            if para.is_empty() {
                sin_email.push(dest.clone());
            }
            let mut asunto = format!("Petición de precios — {}", v.obra.trim());
            if !alcance.is_empty() {
                asunto.push_str(&format!(" — {alcance}"));
            }
            if !v.referencia.trim().is_empty() {
                asunto.push_str(&format!(" (ref. {})", v.referencia.trim()));
            }
            correos.push(Correo {
                para,
                asunto,
                cuerpo: v.cuerpo(&alcance, lineas.iter().any(|l| l.variante)),
                adjunto: ruta,
            });
        }
        if correos.is_empty() {
            v.mensaje = Some(("No hay nada que pedir.".into(), true));
            return;
        }
        v.ultimo = Some(carpeta.clone());
        let aviso = if sin_email.is_empty() {
            String::new()
        } else {
            format!(
                " Sin email en la biblioteca: {} (escríbalo en el correo o en Precios → Biblioteca).",
                sin_email.join(", ")
            )
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let r = correo::preparar(&correos, &carpeta).map(|m| format!("{m}{aviso}"));
            let _ = tx.send(r);
        });
        v.envio = Some(rx);
        v.mensaje = Some(("Abriendo los correos en Outlook…".into(), false));
    }

    pub(super) fn ventana_peticion(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(mut v) = self.ventana_peticion.take() else {
            return;
        };
        if v.recalcular {
            self.calcular_peticion(&mut v);
        }
        if let Some(rx) = &v.envio {
            match rx.try_recv() {
                Ok(r) => {
                    v.mensaje = Some(match r {
                        Ok(m) => (m, false),
                        Err(e) => (e, true),
                    });
                    v.envio = None;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(mpsc::TryRecvError::Disconnected) => v.envio = None,
            }
        }
        let mut abierta = true;
        egui::Window::new("Petición de precios a proveedores")
            .open(&mut abierta)
            .default_width(1000.0)
            .default_height(620.0)
            .resizable(true)
            .show(ctx, |ui| self.contenido_peticion(ui, &mut v, acciones));
        if abierta {
            self.ventana_peticion = Some(v);
        }
    }

    fn contenido_peticion(&self, ui: &mut Ui, v: &mut VentanaPeticion, acciones: &mut Vec<Accion>) {
        ui.horizontal(|ui| {
            let antes = v.vista;
            ui.selectable_value(&mut v.vista, Vista::Proveedor, "Por proveedor")
                .on_hover_text("Cada fabricante con lo suyo");
            ui.selectable_value(&mut v.vista, Vista::Tipo, "Por tipo de producto")
                .on_hover_text("Lo mismo a varias marcas para comparar (p. ej. recuperadores)");
            if v.vista != antes {
                v.elegido.clear();
                v.marcadas.clear();
            }
            ui.separator();
            if ui
                .button("⟳ Recalcular")
                .on_hover_text("Con los cambios hechos en la obra")
                .clicked()
            {
                self.calcular_peticion(v);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Obra");
            ui.add(egui::TextEdit::singleline(&mut v.obra).desired_width(220.0));
            ui.label("Nuestra ref.");
            ui.add(egui::TextEdit::singleline(&mut v.referencia).desired_width(100.0));
            ui.label("Fecha");
            ui.add(egui::TextEdit::singleline(&mut v.fecha).desired_width(80.0));
            ui.label("Respuesta antes del");
            ui.add(
                egui::TextEdit::singleline(&mut v.plazo)
                    .hint_text("dd/mm/aaaa")
                    .desired_width(80.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Observaciones");
            ui.add(
                egui::TextEdit::singleline(&mut v.observaciones)
                    .hint_text("plazo de obra, entregas parciales, condiciones…")
                    .desired_width(f32::INFINITY),
            );
        });
        if let Some((m, error)) = v.mensaje.clone() {
            ui.horizontal(|ui| {
                let c = if error {
                    Color32::from_rgb(220, 80, 60)
                } else {
                    Color32::from_rgb(80, 175, 110)
                };
                ui.label(RichText::new(m).color(c));
                if let Some(r) = v.ultimo.clone()
                    && ui.small_button("Abrir carpeta").clicked()
                {
                    super::asistente::abrir_en_sistema(&r);
                }
            });
        }
        ui.separator();
        if v.peticiones.iter().all(|x| x.lineas.is_empty()) {
            ui.label(
                "No hay nada sin precio en la obra. Puede añadir partidas como variante eligiéndolas en el árbol.",
            );
        }
        match v.vista {
            Vista::Proveedor => self.peticion_por_proveedor(ui, v, acciones),
            Vista::Tipo => self.peticion_por_tipo(ui, v),
        }
    }

    /// Partidas elegidas en el árbol (o la seleccionada).
    fn elegidas_arbol(&self) -> Vec<String> {
        let mut v: Vec<String> = self.arbol_sel.iter().map(|(_, p)| p.clone()).collect();
        if v.is_empty()
            && let Some((_, p)) = &self.sel
        {
            v.push(p.clone());
        }
        v
    }

    /// Botón para añadir variantes del árbol y lista de las añadidas.
    fn bloque_variantes(&self, ui: &mut Ui, v: &mut VentanaPeticion, base: &[LineaPeticion]) {
        let clave = v.clave();
        let elegidas = self.elegidas_arbol();
        ui.horizontal(|ui| {
            let b = ui
                .add_enabled(
                    !elegidas.is_empty(),
                    egui::Button::new(format!("＋ Variante desde el árbol ({})", elegidas.len())),
                )
                .on_hover_text(
                    "Añade las partidas elegidas en el árbol (Ctrl/Mayús + clic) para que el proveedor las valore \
                     como alternativa, aunque ya tengan precio",
                );
            if b.clicked() {
                let lista = v.variantes.entry(clave.clone()).or_default();
                let mut errores = Vec::new();
                for c in &elegidas {
                    if lista.iter().any(|l| &l.codigo == c) || base.iter().any(|l| &l.codigo == c) {
                        continue;
                    }
                    match peticion::variante(&self.p, c) {
                        Ok(l) => lista.push(l),
                        Err(e) => errores.push(e.to_string()),
                    }
                }
                if !errores.is_empty() {
                    v.mensaje = Some((errores.join("; "), true));
                }
            }
        });
        let mut quitar = None;
        if let Some(lista) = v.variantes.get(&clave).filter(|l| !l.is_empty()) {
            ui.label(RichText::new("Variantes (se valoran aparte)").strong());
            egui::Grid::new("peticion_variantes")
                .striped(true)
                .num_columns(5)
                .show(ui, |ui| {
                    for (i, l) in lista.iter().enumerate() {
                        if ui.small_button("✕").on_hover_text("Quitar").clicked() {
                            quitar = Some(i);
                        }
                        ui.label(&l.codigo);
                        ui.label(corto(&l.resumen, 60)).on_hover_text(&l.texto);
                        ui.label(&l.unidad);
                        ui.label(cant(l.cantidad, self.p.decimales.medicion));
                        ui.end_row();
                    }
                });
        }
        if let Some(i) = quitar
            && let Some(l) = v.variantes.get_mut(&clave)
        {
            l.remove(i);
        }
    }

    fn peticion_por_proveedor(&self, ui: &mut Ui, v: &mut VentanaPeticion, acciones: &mut Vec<Accion>) {
        let lista: Vec<(String, usize, bool)> = v
            .peticiones
            .iter()
            .map(|x| (x.nombre().to_owned(), x.lineas.len(), x.proveedor.is_empty()))
            .collect();
        if !lista.iter().any(|l| l.0 == v.elegido)
            && let Some(l) = lista.first()
        {
            v.elegido = l.0.clone();
        }
        egui::Panel::left("peticion_proveedores")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Proveedores").strong());
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (n, l, sin) in &lista {
                        let mut t = RichText::new(format!("{n}\n{l} líneas"));
                        if *sin {
                            t = t.italics().color(NARANJA);
                        }
                        if ui.selectable_label(v.elegido == *n, t).clicked() {
                            v.elegido = n.clone();
                            v.marcadas.clear();
                        }
                    }
                });
                ui.add_space(6.0);
                let con: Vec<&Peticion> = v.peticiones.iter().filter(|x| !x.proveedor.is_empty()).collect();
                if ui
                    .add_enabled(
                        !con.is_empty(),
                        egui::Button::new(format!("📧 A todos ({})", con.len())),
                    )
                    .on_hover_text("Un correo por proveedor, cada uno solo con lo suyo y sus variantes")
                    .clicked()
                {
                    let envios = con
                        .iter()
                        .map(|x| {
                            let mut ls = x.lineas.clone();
                            ls.extend(
                                v.variantes
                                    .get(&format!("P:{}", x.nombre()))
                                    .cloned()
                                    .unwrap_or_default(),
                            );
                            (x.proveedor.clone(), ls, String::new())
                        })
                        .collect();
                    self.enviar_peticiones(v, envios);
                }
            });
        let Some(x) = v.peticion(&v.elegido).cloned() else {
            return;
        };
        let sin = x.proveedor.is_empty();
        ui.horizontal(|ui| {
            ui.heading(x.nombre());
            if !sin {
                let e = v.email(&x.proveedor);
                ui.label(if e.is_empty() {
                    RichText::new("sin email en la biblioteca").weak().color(NARANJA)
                } else {
                    RichText::new(e).weak()
                });
            }
        });
        // Asignar proveedor a las marcadas
        ui.horizontal(|ui| {
            ui.label(format!("Marcadas: {}", v.marcadas.len()));
            ui.label("Asignar a");
            ui.add(
                egui::TextEdit::singleline(&mut v.asignar)
                    .hint_text("fabricante o proveedor")
                    .desired_width(160.0),
            );
            ui.menu_button("▾", |ui| {
                let mut nombres: BTreeSet<String> = v.bib.fabricantes.values().map(|f| f.nombre.clone()).collect();
                nombres.extend(
                    v.peticiones
                        .iter()
                        .filter(|x| !x.proveedor.is_empty())
                        .map(|x| x.proveedor.clone()),
                );
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for n in nombres {
                        if ui.button(&n).clicked() {
                            v.asignar = n;
                            ui.close();
                        }
                    }
                });
            });
            let puede = !v.marcadas.is_empty() && !v.asignar.trim().is_empty();
            if ui
                .add_enabled(puede, egui::Button::new("Asignar"))
                .on_hover_text("Queda como proveedor de esas líneas (origen del precio) para las próximas veces")
                .clicked()
            {
                let fab = v.asignar.trim().to_owned();
                let lote: Vec<Accion> = v
                    .marcadas
                    .iter()
                    .filter_map(|c| {
                        let k = self.p.conceptos.get(c)?;
                        let mut o = k.origen.clone().unwrap_or_default();
                        o.fabricante = fab.clone();
                        Some(Accion::Origen(c.clone(), Some(Box::new(o)), false))
                    })
                    .collect();
                if !lote.is_empty() {
                    acciones.push(Accion::Lote(format!("asignar {fab} en la petición de precios"), lote));
                    v.recalcular = true;
                    v.elegido = ppto_core::descuentos::normalizar(&fab);
                }
                v.marcadas.clear();
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("peticion_lineas")
            .max_height(ui.available_height() - 150.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("peticion_prov")
                    .striped(true)
                    .num_columns(7)
                    .show(ui, |ui| {
                        for c in ["", "Código", "Ref.", "Tipo", "Descripción", "Ud", "Cantidad"] {
                            ui.label(RichText::new(c).strong());
                        }
                        ui.end_row();
                        for l in &x.lineas {
                            let mut m = v.marcadas.contains(&l.codigo);
                            let r = ui
                                .add_enabled(!l.de_tarifa, egui::Checkbox::without_text(&mut m))
                                .on_disabled_hover_text("El proveedor sale de su tarifa");
                            if r.changed() {
                                if m {
                                    v.marcadas.insert(l.codigo.clone());
                                } else {
                                    v.marcadas.remove(&l.codigo);
                                }
                            }
                            ui.label(&l.codigo);
                            ui.label(l.referencia.as_deref().unwrap_or(""));
                            ui.label(RichText::new(l.tipo).weak());
                            let mut ayuda = l.resumen.clone();
                            if !l.texto.is_empty() {
                                ayuda.push_str(&format!("\n\n{}", l.texto));
                            }
                            ayuda.push_str(&format!("\n\nPartidas: {}", l.partidas.join(", ")));
                            ui.label(corto(&l.resumen, 60)).on_hover_text(ayuda);
                            ui.label(&l.unidad);
                            ui.label(cant(l.cantidad, self.p.decimales.medicion));
                            ui.end_row();
                        }
                    });
            });
        ui.separator();
        self.bloque_variantes(ui, v, &x.lineas);
        ui.horizontal(|ui| {
            let r = ui
                .add_enabled(
                    !sin,
                    egui::Button::new(format!("📧 Correo a {} en Outlook", x.nombre())),
                )
                .on_hover_text("Guarda su Excel (solo su alcance y las variantes) y abre el correo sin enviar")
                .on_disabled_hover_text("Asigne antes un proveedor a estas líneas");
            if r.clicked() {
                let mut ls = x.lineas.clone();
                ls.extend(v.variantes.get(&v.clave()).cloned().unwrap_or_default());
                self.enviar_peticiones(v, vec![(x.proveedor.clone(), ls, String::new())]);
            }
        });
    }

    fn peticion_por_tipo(&self, ui: &mut Ui, v: &mut VentanaPeticion) {
        let tipos = peticion::por_tipo(&v.peticiones);
        let lista: Vec<(String, usize)> = tipos.iter().map(|(t, ls)| ((*t).to_owned(), ls.len())).collect();
        if !lista.iter().any(|l| l.0 == v.elegido)
            && let Some(l) = lista.first()
        {
            v.elegido = l.0.clone();
        }
        egui::Panel::left("peticion_tipos")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Tipos de producto").strong());
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (t, n) in &lista {
                        if ui
                            .selectable_label(v.elegido == *t, format!("{t}\n{n} líneas"))
                            .clicked()
                        {
                            v.elegido = t.clone();
                        }
                    }
                });
            });
        let tipo = v.elegido.clone();
        let lineas: Vec<(String, LineaPeticion)> = tipos.get(tipo.as_str()).cloned().unwrap_or_default();
        ui.heading(&tipo);
        // Destinatarios
        let candidatos = v.candidatos(&tipo);
        if !v.destinatarios.contains_key(&tipo) {
            v.destinatarios
                .insert(tipo.clone(), candidatos.iter().cloned().collect());
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Pedir a:");
            for c in &candidatos {
                let d = v.destinatarios.entry(tipo.clone()).or_default();
                let mut m = d.contains(c);
                let e = v.bib.fabricante(c).map(|f| f.datos.email.clone()).unwrap_or_default();
                let r = ui.checkbox(&mut m, c).on_hover_text(if e.is_empty() {
                    "sin email en la biblioteca".to_owned()
                } else {
                    e
                });
                if r.changed() {
                    if m {
                        d.insert(c.clone());
                    } else {
                        d.remove(c);
                    }
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut v.otro)
                    .hint_text("otra marca…")
                    .desired_width(120.0),
            );
            if ui.small_button("＋").clicked() && !v.otro.trim().is_empty() {
                let n = ppto_core::descuentos::normalizar(&v.otro);
                v.destinatarios.entry(tipo.clone()).or_default().insert(n);
                v.otro.clear();
            }
        });
        if candidatos.is_empty() {
            ui.label(
                RichText::new(format!(
                    "No hay fabricantes de «{tipo}» en la biblioteca: escriba las marcas y se recordarán."
                ))
                .small()
                .color(NARANJA),
            );
        }
        ui.checkbox(
            &mut v.recordar,
            "Recordar en la biblioteca que estas marcas suministran este tipo",
        );
        egui::ScrollArea::vertical()
            .id_salt("peticion_tipo_lineas")
            .max_height(ui.available_height() - 150.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("peticion_tipo")
                    .striped(true)
                    .num_columns(6)
                    .show(ui, |ui| {
                        for c in ["Código", "Ref.", "Descripción", "Ud", "Cantidad", "Proveedor actual"] {
                            ui.label(RichText::new(c).strong());
                        }
                        ui.end_row();
                        for (prov, l) in &lineas {
                            ui.label(&l.codigo);
                            ui.label(l.referencia.as_deref().unwrap_or(""));
                            ui.label(corto(&l.resumen, 60)).on_hover_text(if l.texto.is_empty() {
                                l.resumen.clone()
                            } else {
                                format!("{}\n\n{}", l.resumen, l.texto)
                            });
                            ui.label(&l.unidad);
                            ui.label(cant(l.cantidad, self.p.decimales.medicion));
                            ui.label(RichText::new(if prov.is_empty() { "—" } else { prov }).weak());
                            ui.end_row();
                        }
                    });
            });
        ui.separator();
        let base: Vec<LineaPeticion> = lineas.iter().map(|(_, l)| l.clone()).collect();
        self.bloque_variantes(ui, v, &base);
        let dest: Vec<String> = v
            .destinatarios
            .get(&tipo)
            .map(|d| d.iter().cloned().collect())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !dest.is_empty() && !base.is_empty(),
                    egui::Button::new(format!("📧 Correos en Outlook ({})", dest.len())),
                )
                .on_hover_text("Un correo por marca, cada uno con un Excel solo de este tipo (y sus variantes)")
                .clicked()
            {
                if v.recordar
                    && let Some(r) = ppto_biblioteca::ruta()
                {
                    let nuevos: Vec<&String> = dest
                        .iter()
                        .filter(|d| !v.bib.fabricante(d).is_some_and(|f| f.datos.suministra(&tipo)))
                        .collect();
                    if !nuevos.is_empty() {
                        match Biblioteca::modificar(&r, |b| {
                            for d in &nuevos {
                                b.anadir_tipo(d, &tipo);
                            }
                        }) {
                            Ok(()) => v.bib = Biblioteca::cargar(&r),
                            Err(e) => v.mensaje = Some((format!("No se pudo guardar la biblioteca: {e}"), true)),
                        }
                    }
                }
                let mut ls = base.clone();
                ls.extend(v.variantes.get(&v.clave()).cloned().unwrap_or_default());
                let envios = dest.iter().map(|d| (d.clone(), ls.clone(), tipo.clone())).collect();
                self.enviar_peticiones(v, envios);
            }
        });
    }
}
