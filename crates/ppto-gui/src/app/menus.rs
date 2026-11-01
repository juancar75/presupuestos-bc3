// SPDX-License-Identifier: GPL-3.0-or-later
//! Barra superior y menús (Archivo, Edición, Precios, IA) y ventanas de información.

use super::*;

impl Aplicacion {
    /// Ventana «Acerca de»: versión instalada y dónde está cada cosa.
    pub(super) fn ventana_acerca(&mut self, ctx: &egui::Context) {
        let mut abierta = self.ver_acerca;
        egui::Window::new("Acerca de presupuestos-bc3")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading(format!("Versión {VERSION}"));
                ui.label("Gestor de presupuestos BC3 (FIEBDC-3) · licencia GPL-3.0-or-later");
                ui.add_space(6.0);
                egui::Grid::new("acerca").num_columns(2).show(ui, |ui| {
                    ui.label("Programa:");
                    ui.label(
                        std::env::current_exe()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "—".into()),
                    );
                    ui.end_row();
                    ui.label("Datos y ajustes:");
                    ui.label(
                        pdf::carpeta_config()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "—".into()),
                    );
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.hyperlink_to(
                    "Versiones publicadas y novedades",
                    "https://github.com/juancar75/presupuestos-bc3/releases",
                );
                if ui.button("Copiar versión").clicked() {
                    ui.ctx().copy_text(format!("presupuestos-bc3 {VERSION}"));
                }
            });
        self.ver_acerca = abierta;
    }

    pub(super) fn ventana_informe(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        let Some(imp) = &self.informe else { return };
        let mut abierta = self.ver_informe;
        egui::Window::new("Informe de importación BC3")
            .open(&mut abierta)
            .default_size([760.0, 520.0])
            .show(ctx, |ui| {
                let c = &imp.cabecera;
                ui.label(format!(
                    "{} · {} · {}",
                    c.programa, c.version_formato, c.juego_caracteres
                ));
                let regs: Vec<String> = imp.registros.iter().map(|(k, v)| format!("~{k} {v}")).collect();
                ui.label(RichText::new(regs.join("   ")).weak());
                match imp.pem_declarado {
                    Some(d) if d == self.calc.pem => {
                        ui.label(
                            RichText::new(format!("PEM {} € — coincide con el BC3", eur(d)))
                                .color(Color32::from_rgb(70, 170, 90)),
                        );
                    }
                    Some(d) => {
                        ui.label(
                            RichText::new(format!(
                                "PEM calculado {} € · declarado {} € · diferencia {} €",
                                eur(self.calc.pem),
                                eur(d),
                                eur(self.calc.pem - d)
                            ))
                            .color(Color32::from_rgb(220, 150, 40)),
                        );
                    }
                    None => {}
                }
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Precios que no cuadran con el BC3: {}",
                        imp.discrepancias.len()
                    ))
                    .strong(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("disc")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        egui::Grid::new("discrepancias")
                            .striped(true)
                            .num_columns(5)
                            .show(ui, |ui| {
                                for t in ["Código", "Resumen", "Declarado", "Calculado", "Diferencia"] {
                                    ui.label(RichText::new(t).weak());
                                }
                                ui.end_row();
                                for d in imp.discrepancias.iter().take(500) {
                                    let r = ui.add(egui::Button::new(&d.codigo).frame(false));
                                    if r.clicked() {
                                        acciones.push(Accion::Ir(d.codigo.clone()));
                                    }
                                    ui.label(corto(&d.resumen, 40));
                                    ui.label(eur(d.declarado));
                                    ui.label(eur(d.calculado));
                                    ui.label(eur(d.diferencia()));
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Incidencias: {} errores, {} avisos, {} en total",
                        imp.errores(),
                        imp.avisos(),
                        imp.incidencias.len()
                    ))
                    .strong(),
                );
                egui::ScrollArea::vertical().id_salt("inc").show(ui, |ui| {
                    for i in &imp.incidencias {
                        let color = match i.gravedad {
                            ppto_bc3::Gravedad::Error => Color32::from_rgb(220, 80, 60),
                            ppto_bc3::Gravedad::Aviso => Color32::from_rgb(220, 150, 40),
                            ppto_bc3::Gravedad::Info => Color32::GRAY,
                        };
                        ui.label(RichText::new(i.to_string().replace('→', "->")).color(color));
                    }
                });
            });
        self.ver_informe = abierta;
    }

    /// Barra superior: menús (Archivo, Edición, Precios, IA, Ver, Ayuda) y, a
    /// la vista, lo de uso diario (deshacer, guardar, asistente, PEM).
    pub(super) fn barra_superior(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Archivo", |ui| self.menu_archivo(ui));
            ui.menu_button("Edición", |ui| self.menu_edicion(ui, acciones));
            ui.menu_button("Precios", |ui| self.menu_precios(ui));
            ui.menu_button("Informes", |ui| self.menu_informes(ui));
            ui.menu_button("Obra", |ui| self.menu_obra(ui));
            ui.menu_button("IA", |ui| self.menu_ia(ui));
            ui.menu_button("Ver", |ui| {
                for (n, (p, t, _)) in vistas::VISTAS.iter().enumerate() {
                    if ui
                        .add(egui::Button::selectable(self.pestana == *p, *t).shortcut_text(format!("Ctrl+{}", n + 1)))
                        .clicked()
                    {
                        ui.close();
                        self.pestana = *p;
                    }
                }
                ui.separator();
                let oscuro = ui.visuals().dark_mode;
                if ui
                    .button(if oscuro { "☀ Modo claro" } else { "🌙 Modo oscuro" })
                    .clicked()
                {
                    ui.close();
                    self.cambiar_tema(ui.ctx(), !oscuro);
                }
                if ui
                    .add_enabled(self.informe.is_some(), egui::Button::new("Informe de importación"))
                    .clicked()
                {
                    ui.close();
                    self.ver_informe = true;
                }
            });
            ui.menu_button("Ayuda", |ui| {
                if ui.button(format!("Info (versión {VERSION})")).clicked() {
                    ui.close();
                    self.ver_acerca = true;
                }
                if ui.button("Buscar actualizaciones…").clicked() {
                    ui.close();
                    self.actualizacion.buscar(true);
                    self.actualizacion.ventana = true;
                }
                ui.hyperlink_to("Novedades", "https://github.com/juancar75/presupuestos-bc3/releases");
            });
            self.boton_actualizar(ui);
            ui.separator();
            // Uso diario
            let des = self.historial.deshacer.last().map(|(_, q)| q.clone());
            let reh = self.historial.rehacer.last().map(|(_, q)| q.clone());
            if ui
                .add_enabled(des.is_some(), egui::Button::new("⟲"))
                .on_hover_text(des.map_or("Nada que deshacer".into(), |q| format!("Deshacer {q} (Ctrl+Z)")))
                .clicked()
            {
                self.deshacer();
            }
            if ui
                .add_enabled(reh.is_some(), egui::Button::new("⟳"))
                .on_hover_text(reh.map_or("Nada que rehacer".into(), |q| format!("Rehacer {q} (Ctrl+Y)")))
                .clicked()
            {
                self.rehacer();
            }
            if ui
                .add_enabled(self.cambios || self.fichero.is_none(), egui::Button::new("💾 Guardar"))
                .on_hover_text("Guarda una revisión nueva (R1, R2…); las anteriores no se modifican (Ctrl+S)")
                .clicked()
            {
                self.guardar(false);
            }
            ui.toggle_value(&mut self.asistente.abierto, "✨ Asistente")
                .on_hover_text("Conversación con la IA sobre esta obra: encargos, ofertas adjuntas e informes");
            self.boton_propuestas(ui);
            ui.separator();
            if !self.revisiones.is_empty() {
                let mut elegida = self.revision_actual;
                egui::ComboBox::from_id_salt("revision")
                    .selected_text(format!("Revisión {}", self.etiqueta_actual()))
                    .show_ui(ui, |ui| {
                        for r in &self.revisiones {
                            let candado = if r.bloqueada { " 🔒" } else { "" };
                            ui.selectable_value(
                                &mut elegida,
                                Some(r.id),
                                format!(
                                    "{}{} · {} · {}",
                                    r.etiqueta,
                                    candado,
                                    r.autor,
                                    &r.creado_en[..16.min(r.creado_en.len())]
                                ),
                            );
                        }
                    });
                if elegida != self.revision_actual
                    && let Some(id) = elegida
                {
                    self.cambiar_revision(id);
                }
            }
            if self.cambios {
                ui.label(RichText::new("* cambios sin guardar").color(Color32::from_rgb(220, 150, 40)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{} €", eur(self.calc.pem))).size(20.0).strong());
                ui.label(RichText::new("PEM").weak());
            });
        });
    }

    pub(super) fn menu_archivo(&mut self, ui: &mut Ui) {
        let boton = |ui: &mut Ui, texto: &str, atajo: &str| ui.add(egui::Button::new(texto).shortcut_text(atajo));
        if boton(ui, "Nuevo", "Ctrl+N")
            .on_hover_text("Presupuesto vacío")
            .clicked()
        {
            ui.close();
            self.nuevo_presupuesto();
        }
        ui.menu_button("Nuevo desde plantilla", |ui| {
            for t in ppto_core::plantillas::Plantilla::TODAS {
                if ui.button(t.nombre()).clicked() {
                    ui.close();
                    self.nuevo_desde_plantilla(t);
                }
            }
        });
        if boton(ui, "📂 Abrir…", "Ctrl+O").clicked() {
            ui.close();
            self.dialogo_abrir();
        }
        if ui
            .button("Abrir en otra ventana…")
            .on_hover_text(
                "Otra obra en otra ventana (p. ej. en el otro monitor) para copiar partidas con Ctrl+C / Ctrl+V",
            )
            .clicked()
        {
            ui.close();
            if let Some(r) = rfd::FileDialog::new()
                .add_filter("Presupuesto (SQLite o BC3)", &["sqlite", "db", "bc3", "BC3"])
                .pick_file()
            {
                self.abrir_otra_ventana(Some(r));
            }
        }
        if ui.button("Nueva ventana").clicked() {
            ui.close();
            self.abrir_otra_ventana(None);
        }
        let txt_guardar = if self.fichero.is_some() {
            "💾 Guardar revisión"
        } else {
            "💾 Guardar…"
        };
        if ui
            .add_enabled(
                self.cambios || self.fichero.is_none(),
                egui::Button::new(txt_guardar).shortcut_text("Ctrl+S"),
            )
            .clicked()
        {
            ui.close();
            self.guardar(false);
        }
        if boton(ui, "Guardar como…", "Ctrl+Mayús+S").clicked() {
            ui.close();
            self.guardar(true);
        }
        ui.separator();
        if boton(ui, "📄 PDF / Imprimir…", "Ctrl+P")
            .on_hover_text(
                "Presupuesto para el cliente: portada con logo, partidas, mediciones, resumen y observaciones",
            )
            .clicked()
        {
            ui.close();
            self.abrir_pdf();
        }
        ui.menu_button("Comparar con…", |ui| {
            let otras: Vec<(i64, String)> = self
                .revisiones
                .iter()
                .filter(|r| Some(r.id) != self.revision_actual)
                .map(|r| (r.id, r.etiqueta.clone()))
                .collect();
            if otras.is_empty() {
                ui.label(RichText::new("(no hay otras revisiones guardadas)").weak());
            }
            for (id, et) in otras {
                if ui.button(format!("Revisión {et}")).clicked() {
                    ui.close();
                    self.comparar_con_revision(id);
                }
            }
            ui.separator();
            if ui
                .button("Un fichero BC3…")
                .on_hover_text("Por ejemplo, la versión que te devuelve el cliente o la de Presto")
                .clicked()
            {
                ui.close();
                self.comparar_con_bc3();
            }
        });
        ui.menu_button("📥 Importar", |ui| {
            if ui
                .button("BC3…")
                .on_hover_text("Leer un presupuesto FIEBDC-3 (p. ej. exportado de Presto)")
                .clicked()
            {
                ui.close();
                self.dialogo_importar();
            }
            ui.separator();
            if ui
                .button("Excel ciego…")
                .on_hover_text("Listado «Presupuesto y mediciones» de Presto en Excel, con precios a cero")
                .clicked()
            {
                ui.close();
                self.dialogo_ciego();
            }
            if ui
                .button("Modelo BIM (IFC o Revit)…")
                .on_hover_text(
                    "Mediciones de las instalaciones de un modelo .ifc o .rvt: tuberías, conductos, equipos…",
                )
                .clicked()
            {
                ui.close();
                self.dialogo_modelo();
            }
            if ui
                .button("Traer precios de un BC3…")
                .on_hover_text("Base de precios, obra anterior o el mismo ciego valorado en Presto")
                .clicked()
            {
                ui.close();
                self.traer_precios();
            }
        });
        ui.menu_button("📤 Exportar", |ui| {
            if ui
                .button("BC3 · Presto 8.8 o posterior…")
                .on_hover_text("FIEBDC-3/2002 en ANSI, como lo exporta Presto 8.8")
                .clicked()
            {
                ui.close();
                self.exportar_bc3(None);
            }
            if ui
                .button("BC3 · Presto antiguo (códigos de 12 caracteres)…")
                .on_hover_text(
                    "Para versiones que cortan los códigos al importar: los códigos largos se renombran para que no se mezclen conceptos",
                )
                .clicked()
            {
                ui.close();
                self.exportar_bc3(Some(12));
            }
            ui.separator();
            if ui
                .button("📊 Excel…")
                .on_hover_text("Resumen, presupuesto, descompuestos, mediciones, recursos y horas por oficio")
                .clicked()
            {
                ui.close();
                self.exportar_excel();
            }
            if ui
                .button("📄 PDF para el cliente…")
                .on_hover_text("Portada con logo, partidas, mediciones, resumen y observaciones")
                .clicked()
            {
                ui.close();
                self.abrir_pdf();
            }
            if ui
                .button("Excel ciego valorado…")
                .on_hover_text("El mismo Excel del cliente con los precios en la columna PrPres")
                .clicked()
            {
                ui.close();
                self.devolver_valorado();
            }
        });
        ui.separator();
        if ui
            .button("📄 Abrir ejemplo")
            .on_hover_text("Ejemplo sintético de suelo radiante")
            .clicked()
        {
            ui.close();
            self.cargar_ejemplo();
        }
        ui.separator();
        if ui.button("Salir").clicked() {
            ui.close();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn menu_edicion(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let des = self.historial.deshacer.last().map(|(_, q)| q.clone());
        let reh = self.historial.rehacer.last().map(|(_, q)| q.clone());
        if ui
            .add_enabled(
                des.is_some(),
                egui::Button::new(des.map_or("Deshacer".into(), |q| format!("Deshacer {q}"))).shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            ui.close();
            self.deshacer();
        }
        if ui
            .add_enabled(
                reh.is_some(),
                egui::Button::new(reh.map_or("Rehacer".into(), |q| format!("Rehacer {q}"))).shortcut_text("Ctrl+Y"),
            )
            .clicked()
        {
            ui.close();
            self.rehacer();
        }
        ui.separator();
        let sel = self.sel.clone();
        let es_partida = sel.as_ref().is_some_and(|(_, h)| {
            self.p
                .conceptos
                .get(h)
                .is_some_and(|c| c.naturaleza == Naturaleza::Partida)
        });
        let pista = |ui: egui::Response| {
            ui.on_disabled_hover_text("Elija antes una partida en el árbol (también con clic derecho en el árbol)")
        };
        if pista(ui.add_enabled(es_partida, egui::Button::new("Cortar partida"))).clicked()
            && let Some((p, h)) = &sel
        {
            ui.close();
            acciones.push(Accion::Portapapeles(Some((p.clone(), h.clone(), true))));
        }
        if pista(ui.add_enabled(es_partida, egui::Button::new("Copiar partida"))).clicked()
            && let Some((p, h)) = &sel
        {
            ui.close();
            acciones.push(Accion::Portapapeles(Some((p.clone(), h.clone(), false))));
        }
        let destino = sel.as_ref().map(|(p, _)| p.clone());
        let texto_pegar = match (&self.portapapeles, &destino) {
            (Some((_, h, _)), Some(d)) => format!("Pegar {h} en {d}"),
            _ => "Pegar".into(),
        };
        if ui
            .add_enabled(
                self.portapapeles.is_some() && destino.is_some(),
                egui::Button::new(texto_pegar),
            )
            .on_hover_text("En el capítulo de la partida elegida; para otro sitio, clic derecho en el capítulo")
            .clicked()
            && let (Some((o, h, cortar)), Some(d)) = (self.portapapeles.clone(), destino)
        {
            ui.close();
            acciones.push(if cortar {
                Accion::Trasladar(o, h, d, None)
            } else {
                Accion::Copiar(o, h, d)
            });
        }
        ui.separator();
        if ui
            .add(egui::Button::new("🔍 Buscar partida").shortcut_text("Ctrl+F"))
            .clicked()
        {
            ui.close();
            self.enfocar_buscar = true;
        }
    }

    pub(super) fn menu_precios(&mut self, ui: &mut Ui) {
        if ui
            .button("Tarifas de subcontratas…")
            .on_hover_text("Importar de Excel, revisar unidades y usarlas en los descompuestos")
            .clicked()
        {
            ui.close();
            self.abrir_tarifas();
        }
        if ui
            .button("Biblioteca…")
            .on_hover_text("Partidas de obras anteriores, tarifas de fabricante por año, descuentos y ofertas")
            .clicked()
        {
            ui.close();
            self.ventana_biblioteca = Some(biblioteca::VentanaBiblioteca::abrir());
        }
        if ui
            .button("Bases de precios…")
            .on_hover_text("Tus BC3 de bases de precios: buscar y copiar mano de obra, materiales o partidas")
            .clicked()
        {
            ui.close();
            self.ventana_bases = Some(bases::VentanaBases::abrir());
        }
        if ui
            .button("Legalización…")
            .on_hover_text(
                "Tramitación, visitas de la OCA, justificación RITE/RSIF, desplazamientos, tasas, EICI y visado",
            )
            .clicked()
        {
            ui.close();
            self.ventana_legalizacion = Some(legalizacion::VentanaLegalizacion::abrir(&self.p));
        }
    }

    pub(super) fn menu_ia(&mut self, ui: &mut Ui) {
        if ui
            .button(if self.asistente.abierto {
                "Cerrar asistente"
            } else {
                "✨ Asistente"
            })
            .clicked()
        {
            ui.close();
            self.asistente.abierto = !self.asistente.abierto;
        }
        let n = self.propuestas_pendientes().len();
        if ui.button(format!("Propuestas IA ({n} pendientes)")).clicked() {
            ui.close();
            self.enlace.ventana = true;
        }
    }
}
