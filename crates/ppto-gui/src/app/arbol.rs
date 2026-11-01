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
        match self.calc.estados.get(codigo) {
            Some(Estado::SinPrecio) => t.color(COLOR_SIN_VALORAR),
            Some(e) if e.incompleta() => t.color(COLOR_INCOMPLETA),
            _ => t,
        }
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
                ui.label(RichText::new(format!("■ {rojas} sin valorar")).color(COLOR_SIN_VALORAR).small());
            }
            if naranjas > 0 {
                ui.label(RichText::new(format!("■ {naranjas} incompletas")).color(COLOR_INCOMPLETA).small())
                    .on_hover_text("Con mano de obra pero sin material, o con material sin mano de obra (las subcontratadas no se marcan)");
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
                        ui.add(
                            egui::Button::new(
                                self.color_estado(
                                    RichText::new(format!("{}  {}", c.codigo, c.resumen)).strong(),
                                    &l.hijo,
                                ),
                            )
                            .frame(false)
                            .wrap_mode(egui::TextWrapMode::Truncate)
                            .sense(egui::Sense::click_and_drag()),
                        )
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
                let elegido = self.sel.as_ref() == Some(&(capitulo.to_owned(), l.hijo.clone()));
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
                let r = match self.calc.estados.get(&l.hijo) {
                    Some(e) if *e != ppto_core::valoracion::Estado::Valorada => r.on_hover_text(e.texto()),
                    _ => r,
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
                    acciones.push(Accion::Seleccionar(capitulo.to_owned(), l.hijo.clone()));
                }
                r.context_menu(|ui| menu_linea(ui, capitulo, &l.hijo, false, acciones));
            }
        }
    }
}
