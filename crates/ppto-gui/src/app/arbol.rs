// SPDX-License-Identifier: GPL-3.0-or-later
//! Árbol de la obra: capítulos y partidas, búsqueda, colores de lo que falta y F5.

use super::*;

impl Aplicacion {
    /// F5 (como en Presto): abrir renglón encima de lo que señala el ratón;
    /// si no señala nada, encima de la partida elegida.
    pub(super) fn tecla_f5(&self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        if !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            return;
        }
        let a = self.f5.take().or_else(|| {
            self.sel
                .as_ref()
                .filter(|(cap, _)| {
                    self.p
                        .conceptos
                        .get(cap)
                        .is_some_and(|c| c.naturaleza == Naturaleza::Capitulo)
                })
                .map(|(cap, h)| Accion::Insertar(TipoAlta::Partida { capitulo: cap.clone() }, h.clone()))
        });
        if let Some(a) = a {
            acciones.push(a);
        }
    }

    /// Rojo: sin valorar. Naranja: mano de obra sin material o al revés.
    pub(super) fn color_estado(&self, t: RichText, codigo: &str) -> RichText {
        use ppto_core::valoracion::Estado;
        if self.p.conceptos.get(codigo).is_some_and(|c| c.fuera_de_importe()) {
            return t.weak().strikethrough();
        }
        match self.calc.estados.get(codigo) {
            Some(Estado::SinPrecio) => t.color(COLOR_SIN_VALORAR),
            Some(e) if e.incompleta() => t.color(COLOR_INCOMPLETA),
            _ => t,
        }
    }

    /// Qué significa el color (o el tachado) de un capítulo o partida del
    /// árbol, para el texto emergente al pasar el ratón.
    pub(super) fn explicar_estado(&self, codigo: &str) -> Option<String> {
        use ppto_core::valoracion::Estado;
        let c = self.p.conceptos.get(codigo)?;
        if c.incluida {
            return Some("Tachada: partida INCLUIDA en otra (importe 0 y sale como «INCLUIDO»).".into());
        }
        if c.excluida {
            return Some("Tachada: partida EXCLUIDA (fuera de la oferta: importe 0 y sale como «EXCLUIDO»).".into());
        }
        let cap = c.naturaleza == Naturaleza::Capitulo;
        Some(match self.calc.estados.get(codigo)? {
            Estado::Valorada => return None,
            Estado::SinPrecio if cap => "Rojo: dentro hay partidas sin valorar (precio 0 €).".into(),
            Estado::SinPrecio => "Rojo: sin valorar (precio 0 €). Póngale descompuesto o acepte una propuesta.".into(),
            _ if cap => "Naranja: dentro hay partidas incompletas (material sin mano de obra o al revés).".into(),
            Estado::SoloMaterial => "Naranja: incompleta. Tiene material pero no mano de obra ni subcontrata.".into(),
            Estado::SoloManoObra => "Naranja: incompleta. Tiene mano de obra pero no material.".into(),
        })
    }

    /// Recuento de lo que falta, encima del árbol.
    pub(super) fn leyenda_estados(&self, ui: &mut Ui) {
        use ppto_core::valoracion::Estado;
        let partidas = self
            .p
            .conceptos
            .values()
            .filter(|c| c.naturaleza == Naturaleza::Partida);
        let (mut rojas, mut naranjas) = (0, 0);
        for c in partidas {
            match self.calc.estados.get(&c.codigo) {
                Some(Estado::SinPrecio) => rojas += 1,
                Some(e) if e.incompleta() => naranjas += 1,
                _ => {}
            }
        }
        if rojas + naranjas == 0 {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if rojas > 0 {
                ui.label(
                    RichText::new(format!("■ {rojas} sin valorar"))
                        .color(COLOR_SIN_VALORAR)
                        .small(),
                )
                .on_hover_text("Rojo: partidas con precio 0 € (y los capítulos que las contienen)");
            }
            if naranjas > 0 {
                ui.label(
                    RichText::new(format!("■ {naranjas} incompletas"))
                        .color(COLOR_INCOMPLETA)
                        .small(),
                )
                .on_hover_text(
                    "Naranja: con mano de obra pero sin material, o con material sin mano de obra \
                         (una subcontrata cuenta como mano de obra; las legalizaciones y servicios solo \
                         de horas también salen en naranja)",
                );
            }
        });
    }

    pub(super) fn arbol(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        self.f5.replace(None);
        let raiz = self.p.raiz.clone();
        // Clic derecho en el nombre de la obra o en el espacio libre del
        // árbol: nuevo capítulo (y pegar en el nivel principal).
        let menu_raiz = |ui: &mut Ui, acciones: &mut Vec<Accion>| {
            if ui.button("Nuevo capítulo").clicked() {
                acciones.push(Accion::Alta(TipoAlta::Capitulo { padre: raiz.clone() }));
            }
            if ui
                .button("Renumerar capítulos (C01, C02, C01.01…)")
                .on_hover_text(
                    "Pone los códigos de los capítulos en orden; las partidas no cambian. Se puede deshacer.",
                )
                .clicked()
            {
                acciones.push(Accion::RenumerarCapitulos("C".into()));
                ui.close();
            }
            if let Some((o, h, cortar)) = &self.portapapeles
                && ui.button(format!("Pegar aquí {h}")).clicked()
            {
                acciones.push(if *cortar {
                    Accion::Trasladar(o.clone(), h.clone(), raiz.clone(), None)
                } else {
                    Accion::Copiar(o.clone(), h.clone(), raiz.clone())
                });
            }
        };
        let titulo = ui.add(
            egui::Label::new(RichText::new(&self.p.conceptos[&raiz].resumen).heading()).sense(egui::Sense::click()),
        );
        titulo.context_menu(|ui| menu_raiz(ui, acciones));
        ui.separator();
        ui.label(
            RichText::new("Clic derecho: nuevo capítulo, partida, cortar, copiar, pegar… · arrastre para mover")
                .weak()
                .small(),
        );
        let destino = self
            .capitulo_activo
            .clone()
            .filter(|c| self.p.conceptos.contains_key(c))
            .unwrap_or_else(|| raiz.clone());
        ui.label(
            RichText::new(format!(
                "Ctrl+C copia la partida elegida · Ctrl+V pega en {destino} (también entre ventanas)"
            ))
            .weak()
            .small(),
        );
        let campo = ui.add(
            egui::TextEdit::singleline(&mut self.buscar)
                .hint_text("Buscar partida por código o texto… (Ctrl+F)")
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.enfocar_buscar) {
            campo.request_focus();
        }
        let buscar = self.buscar.trim().to_lowercase();
        self.leyenda_estados(ui);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if buscar.is_empty() {
                self.rama(ui, &raiz, acciones);
                if self.calc.lineas.get(&raiz).is_none_or(Vec::is_empty) {
                    ui.label(
                        RichText::new("(presupuesto vacío: clic derecho aquí → Nuevo capítulo)")
                            .weak()
                            .small(),
                    );
                }
                // Espacio libre debajo del árbol: menú y soltar en el nivel principal
                let alto = ui.available_height().max(60.0);
                let libre = ui.allocate_response(egui::vec2(ui.available_width(), alto), egui::Sense::click());
                libre.context_menu(|ui| menu_raiz(ui, acciones));
                if let Some(a) = libre.dnd_release_payload::<Arrastre>() {
                    acciones.push(Accion::Trasladar(a.origen.clone(), a.hijo.clone(), raiz.clone(), None));
                } else if libre.dnd_hover_payload::<Arrastre>().is_some() {
                    ui.painter().hline(
                        libre.rect.x_range(),
                        libre.rect.top() + 2.0,
                        egui::Stroke::new(2.0, Color32::from_rgb(90, 150, 230)),
                    );
                }
            } else {
                self.resultados(ui, &buscar, acciones);
            }
        });
        self.desplazar_arbol = false;
    }

    /// Lista plana de partidas que contienen el texto buscado.
    pub(super) fn resultados(&self, ui: &mut Ui, buscar: &str, acciones: &mut Vec<Accion>) {
        let mut n = 0;
        for (cap, hijo) in self.orden_partidas() {
            let c = &self.p.conceptos[&hijo];
            let texto = format!("{} {} {}", c.codigo, c.resumen, c.texto.as_deref().unwrap_or("")).to_lowercase();
            if !texto.contains(buscar) {
                continue;
            }
            n += 1;
            let elegido = self.sel.as_ref() == Some(&(cap.clone(), hijo.clone()));
            let precio = self.calc.precios.get(&hijo).copied().unwrap_or_default();
            let r = ui.add(
                egui::Button::selectable(
                    elegido,
                    self.color_estado(RichText::new(format!("{}  {}", c.codigo, corto(&c.resumen, 34))), &hijo),
                )
                .wrap_mode(egui::TextWrapMode::Truncate),
            );
            let r = match self.explicar_estado(&hijo) {
                Some(t) => r.on_hover_text(t),
                None => r,
            };
            ui.label(
                RichText::new(format!("    en {cap} · {} €/{}", eur(precio), c.unidad))
                    .weak()
                    .small(),
            );
            if r.clicked() {
                acciones.push(Accion::Seleccionar(cap, hijo));
            }
            if n >= 300 {
                ui.label(RichText::new("… afine la búsqueda").weak());
                break;
            }
        }
        if n == 0 {
            ui.label(RichText::new("Sin resultados").weak());
        }
    }

    pub(super) fn rama(&self, ui: &mut Ui, capitulo: &str, acciones: &mut Vec<Accion>) {
        let Some(lineas) = self.calc.lineas.get(capitulo) else {
            return;
        };
        for l in lineas {
            let c = &self.p.conceptos[&l.hijo];
            if c.naturaleza == Naturaleza::Capitulo {
                // Cabecera propia (en vez de CollapsingHeader) para que el título
                // sirva a la vez para abrir/cerrar con clic y para arrastrar.
                let id = ui.make_persistent_id(("cap", capitulo, &l.hijo));
                let mut estado = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    self.calc.lineas.get(&l.hijo).is_none_or(|v| v.len() <= 40),
                );
                if self.desplazar_arbol && self.sel.as_ref().is_some_and(|(p, _)| self.p.ruta(p).contains(&l.hijo)) {
                    estado.set_open(true);
                }
                let (_, cabecera, cuerpo) = estado
                    .show_header(ui, |ui| {
                        let r =
                            ui.add(
                                egui::Button::new(self.color_estado(
                                    RichText::new(format!("{}  {}", c.codigo, c.resumen)).strong(),
                                    &l.hijo,
                                ))
                                .frame(false)
                                .wrap_mode(egui::TextWrapMode::Truncate)
                                .sense(egui::Sense::click_and_drag()),
                            );
                        match self.explicar_estado(&l.hijo) {
                            Some(t) => r.on_hover_text(t),
                            None => r,
                        }
                    })
                    .body(|ui| {
                        ui.label(RichText::new(format!("{} €", eur(l.importe))).weak());
                        self.rama(ui, &l.hijo, acciones);
                        if self.calc.lineas.get(&l.hijo).is_none_or(Vec::is_empty) {
                            ui.label(
                                RichText::new("(vacío: arrastre aquí una partida o clic derecho encima para añadir)")
                                    .weak()
                                    .small(),
                            );
                        }
                    });
                let cab = cabecera.inner;
                if cab.hovered() {
                    // F5 sobre un capítulo: partida nueva al final de él
                    self.f5.replace(Some(Accion::Alta(TipoAlta::Partida {
                        capitulo: l.hijo.clone(),
                    })));
                }
                if cab.clicked() || cab.secondary_clicked() {
                    acciones.push(Accion::CapituloActivo(l.hijo.clone()));
                }
                if cab.clicked() {
                    let mut e = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
                    e.toggle(ui);
                    e.store(ui.ctx());
                }
                cab.dnd_set_drag_payload(Arrastre {
                    origen: capitulo.to_owned(),
                    hijo: l.hijo.clone(),
                });
                if cab.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                // Soltar una partida o un capítulo sobre el capítulo (o dentro de
                // él, si está vacío): al final de él
                let vacio = self.calc.lineas.get(&l.hijo).is_none_or(Vec::is_empty);
                let soltado = cab.dnd_release_payload::<Arrastre>().or_else(|| {
                    cuerpo
                        .as_ref()
                        .filter(|_| vacio)
                        .and_then(|b| b.response.dnd_release_payload::<Arrastre>())
                });
                if let Some(a) = soltado.filter(|a| a.hijo != l.hijo) {
                    acciones.push(Accion::Trasladar(
                        a.origen.clone(),
                        a.hijo.clone(),
                        l.hijo.clone(),
                        None,
                    ));
                } else if cab.dnd_hover_payload::<Arrastre>().is_some_and(|a| a.hijo != l.hijo) {
                    ui.painter().rect_stroke(
                        cab.rect,
                        2.0,
                        egui::Stroke::new(2.0, Color32::from_rgb(90, 150, 230)),
                        egui::StrokeKind::Outside,
                    );
                }
                // El menú no se cierra al pinchar en el campo del título (sí al
                // pulsar cualquier opción o fuera de él).
                if cab.secondary_clicked() {
                    // Al abrir el menú, el título parte del actual
                    ui.data_mut(|d| d.remove::<String>(cab.id.with("titulo")));
                }
                egui::Popup::context_menu(&cab)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        // Título (resumen) del capítulo, editable aquí mismo
                        let id_t = cab.id.with("titulo");
                        let borrador = ui.data_mut(|d| d.get_temp::<String>(id_t));
                        let recien_abierto = borrador.is_none();
                        let mut titulo = borrador.unwrap_or_else(|| c.resumen.clone());
                        ui.horizontal(|ui| {
                            ui.label("Título:");
                            let t = ui.add(
                                egui::TextEdit::singleline(&mut titulo)
                                    .desired_width(220.0)
                                    .hint_text("p. ej. Planta baja"),
                            );
                            ui.data_mut(|d| d.insert_temp(egui::Id::new(CAMPO_TITULO), t.rect));
                            ui.data_mut(|d| d.insert_temp(egui::Id::new(CAMPO_TITULO).with("abierto"), l.hijo.clone()));
                            if recien_abierto && c.resumen.is_empty() {
                                t.request_focus();
                            }
                            let enter = t.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button("Guardar").clicked() || enter) && titulo.trim() != c.resumen {
                                acciones.push(Accion::Resumen(l.hijo.clone(), titulo.trim().to_owned()));
                                ui.close();
                            }
                        });
                        if ui.should_close() {
                            ui.data_mut(|d| d.remove::<String>(id_t));
                        } else {
                            ui.data_mut(|d| d.insert_temp(id_t, titulo));
                        }
                        editar_codigo(ui, cab.id.with("codigo"), &l.hijo, acciones);
                        ui.separator();
                        if ui.button("Nueva partida aquí").clicked() {
                            acciones.push(Accion::Alta(TipoAlta::Partida {
                                capitulo: l.hijo.clone(),
                            }));
                        }
                        if ui.button("Nuevo subcapítulo").clicked() {
                            acciones.push(Accion::Alta(TipoAlta::Capitulo { padre: l.hijo.clone() }));
                        }
                        if ui.button("Añadir partida existente…").clicked() {
                            acciones.push(Accion::Alta(TipoAlta::Linea { padre: l.hijo.clone() }));
                        }
                        if let Some((o, h, cortar)) = &self.portapapeles
                            && ui.button(format!("Pegar aquí {h}")).clicked()
                        {
                            acciones.push(if *cortar {
                                Accion::Trasladar(o.clone(), h.clone(), l.hijo.clone(), None)
                            } else {
                                Accion::Copiar(o.clone(), h.clone(), l.hijo.clone())
                            });
                        }
                        ui.separator();
                        menu_linea(ui, capitulo, &l.hijo, true, acciones);
                        // Cualquier clic dentro del menú que no sea en el título lo cierra
                        let campo = ui.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(CAMPO_TITULO)));
                        let clic = ui.input(|i| {
                            i.pointer.any_click()
                                && i.pointer
                                    .interact_pos()
                                    .is_some_and(|p| campo.is_none_or(|r| !r.contains(p)))
                        });
                        if clic && !recien_abierto {
                            ui.data_mut(|d| d.remove::<String>(id_t));
                            ui.close();
                        }
                    });
            } else {
                let par = (capitulo.to_owned(), l.hijo.clone());
                let en_varias = self.arbol_sel.contains(&par);
                let elegido = self.sel.as_ref() == Some(&par) || en_varias;
                let carga = Arrastre {
                    origen: capitulo.to_owned(),
                    hijo: l.hijo.clone(),
                };
                // Un solo botón para clic (abrir la partida) y arrastre (moverla):
                // envolverlo en dnd_drag_source anula el clic.
                let r = ui.add(
                    egui::Button::selectable(
                        elegido,
                        self.color_estado(
                            RichText::new(format!("{}  {}", c.codigo, corto(&c.resumen, 34))),
                            &l.hijo,
                        ),
                    )
                    .wrap_mode(egui::TextWrapMode::Truncate)
                    .sense(egui::Sense::click_and_drag()),
                );
                let r = match self.explicar_estado(&l.hijo) {
                    Some(t) => r.on_hover_text(t),
                    None => r,
                };
                if r.hovered() {
                    self.f5.replace(Some(Accion::Insertar(
                        TipoAlta::Partida {
                            capitulo: capitulo.to_owned(),
                        },
                        l.hijo.clone(),
                    )));
                }
                r.dnd_set_drag_payload(carga);
                if r.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                if elegido && self.desplazar_arbol {
                    r.scroll_to_me(Some(egui::Align::Center));
                }
                // Soltar otra partida encima: se coloca delante de esta
                if let Some(a) = r.dnd_release_payload::<Arrastre>() {
                    if a.hijo != l.hijo {
                        acciones.push(Accion::Trasladar(
                            a.origen.clone(),
                            a.hijo.clone(),
                            capitulo.to_owned(),
                            Some(l.hijo.clone()),
                        ));
                    }
                } else if r.dnd_hover_payload::<Arrastre>().is_some_and(|a| a.hijo != l.hijo) {
                    let y = r.rect.top() - 2.0;
                    ui.painter().hline(
                        r.rect.x_range(),
                        y,
                        egui::Stroke::new(2.0, Color32::from_rgb(90, 150, 230)),
                    );
                }
                ui.label(
                    RichText::new(format!(
                        "    {} {} × {} = {} €",
                        num(l.cantidad, 2),
                        c.unidad,
                        eur(l.precio),
                        eur(l.importe)
                    ))
                    .weak()
                    .small(),
                );
                if r.clicked() {
                    let m = ui.input(|i| i.modifiers);
                    acciones.push(if m.command || m.ctrl {
                        Accion::ElegirArbol(capitulo.to_owned(), l.hijo.clone(), false)
                    } else if m.shift {
                        Accion::ElegirArbol(capitulo.to_owned(), l.hijo.clone(), true)
                    } else {
                        Accion::Seleccionar(capitulo.to_owned(), l.hijo.clone())
                    });
                }
                if en_varias && self.arbol_sel.len() > 1 {
                    r.context_menu(|ui| self.menu_varias(ui, acciones));
                } else {
                    r.context_menu(|ui| menu_linea(ui, capitulo, &l.hijo, false, acciones));
                }
            }
        }
    }
}

/// Campo «Código» con su botón: cambia el código del concepto en todo el
/// presupuesto (líneas, mediciones y seguimiento).
pub(super) fn editar_codigo(ui: &mut Ui, id: egui::Id, codigo: &str, acciones: &mut Vec<Accion>) {
    let mut c = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| codigo.to_owned());
    ui.horizontal(|ui| {
        ui.label("Código:");
        let t = ui.add(egui::TextEdit::singleline(&mut c).desired_width(120.0));
        let enter = t.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui
            .button("Cambiar")
            .on_hover_text("Cambia el código en todo el presupuesto. Se puede deshacer.")
            .clicked()
            || enter)
            && c.trim() != codigo
            && !c.trim().is_empty()
        {
            acciones.push(Accion::Renombrar(codigo.to_owned(), c.trim().to_owned()));
            ui.data_mut(|d| d.remove::<String>(id));
            ui.close();
        }
    });
    if ui.should_close() {
        ui.data_mut(|d| d.remove::<String>(id));
    } else {
        ui.data_mut(|d| d.insert_temp(id, c));
    }
}

impl Aplicacion {
    /// Capítulos en el orden del árbol, con sangría por nivel.
    fn capitulos_en_orden(&self) -> Vec<(String, String)> {
        fn recorrer(a: &Aplicacion, padre: &str, nivel: usize, out: &mut Vec<(String, String)>) {
            for l in a.calc.lineas.get(padre).into_iter().flatten() {
                if let Some(c) = a.p.conceptos.get(&l.hijo)
                    && c.naturaleza == Naturaleza::Capitulo
                    && nivel < 8
                {
                    out.push((
                        l.hijo.clone(),
                        format!("{}{}  {}", "   ".repeat(nivel), c.codigo, corto(&c.resumen, 40)),
                    ));
                    recorrer(a, &l.hijo, nivel + 1, out);
                }
            }
        }
        let mut v = Vec::new();
        recorrer(self, &self.p.raiz, 0, &mut v);
        v
    }

    /// Menú con varias partidas elegidas (Ctrl o Mayús + clic).
    fn menu_varias(&self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let sel = &self.arbol_sel;
        let suma: Decimal = sel
            .iter()
            .filter_map(|(c, h)| {
                self.calc
                    .lineas
                    .get(c)?
                    .iter()
                    .find(|l| &l.hijo == h)
                    .map(|l| l.importe)
            })
            .sum();
        ui.label(RichText::new(format!("{} partidas elegidas · {} €", sel.len(), eur(suma))).strong());
        ui.separator();
        let capitulos = self.capitulos_en_orden();
        let lote = |que: String, v: Vec<Accion>, acciones: &mut Vec<Accion>| {
            if !v.is_empty() {
                acciones.push(Accion::Lote(que, v));
            }
        };
        ui.menu_button("Mover a…", |ui| {
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for (cap, rotulo) in &capitulos {
                    if ui.button(rotulo).clicked() {
                        let v = sel
                            .iter()
                            .filter(|(c, _)| c != cap)
                            .map(|(c, h)| Accion::Trasladar(c.clone(), h.clone(), cap.clone(), None))
                            .collect();
                        lote(format!("mover {} partidas a {cap}", sel.len()), v, acciones);
                        ui.close();
                    }
                }
            });
        });
        ui.menu_button("Copiar a…", |ui| {
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for (cap, rotulo) in &capitulos {
                    if ui.button(rotulo).clicked() {
                        let v = sel
                            .iter()
                            .filter(|(c, _)| c != cap)
                            .map(|(c, h)| Accion::Copiar(c.clone(), h.clone(), cap.clone()))
                            .collect();
                        lote(format!("copiar {} partidas a {cap}", sel.len()), v, acciones);
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        let mut partidas: Vec<&String> = sel.iter().map(|(_, h)| h).collect();
        partidas.sort();
        partidas.dedup();
        for (rotulo, f) in [
            (
                "Excluir de la oferta",
                Accion::Excluir as fn(String, bool) -> Accion,
                true,
            ),
            ("Quitar «excluida»", Accion::Excluir, false),
            ("Marcar «incluida en otra»", Accion::Incluir, true),
            ("Quitar «incluida»", Accion::Incluir, false),
        ]
        .map(|(r, f, b)| (r, move |h: &String| f(h.clone(), b)))
        {
            if ui.button(rotulo).clicked() {
                let v = partidas.iter().map(|h| f(h)).collect();
                lote(
                    format!("{} ({} partidas)", rotulo.to_lowercase(), partidas.len()),
                    v,
                    acciones,
                );
                ui.close();
            }
        }
        ui.separator();
        if ui
            .button(format!("Quitar las {} partidas", sel.len()))
            .on_hover_text("Lo que quede sin usar en ningún otro sitio se borra. Se puede deshacer.")
            .clicked()
        {
            let v = sel
                .iter()
                .map(|(c, h)| Accion::QuitarLinea(c.clone(), h.clone(), true))
                .collect();
            lote(format!("quitar {} partidas", sel.len()), v, acciones);
            ui.close();
        }
        if ui.button("Dejar de elegir   Esc").clicked() {
            acciones.push(Accion::DejarDeElegir);
            ui.close();
        }
    }
}
