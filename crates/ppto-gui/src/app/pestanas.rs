// SPDX-License-Identifier: GPL-3.0-or-later
//! Pestañas Recursos, Subcontratar y Resumen.

use super::*;

impl Aplicacion {
    pub(super) fn pestana_recursos(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let Some(e) = &self.calc.explosion else { return };
        let e = e.clone();
        let dec = self.p.decimales;
        ui.label("Cantidades totales necesarias para ejecutar el presupuesto. Los precios se pueden editar aquí.");
        self.resumen_recursos(ui);
        self.aviso_ofertas(ui);
        egui::CollapsingHeader::new(RichText::new("Descuentos por fabricante").strong())
            .id_salt("descuentos")
            .default_open(!self.p.fabricantes().is_empty())
            .show(ui, |ui| self.form_descuentos(ui, acciones));
        egui::CollapsingHeader::new(RichText::new("Coeficiente de dificultad (horas de mano de obra propia)").strong())
            .id_salt("dificultad")
            .show(ui, |ui| self.form_dificultad(ui, acciones));
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for nat in [
                Naturaleza::ManoObra,
                Naturaleza::Material,
                Naturaleza::Maquinaria,
                Naturaleza::Subcontrata,
                Naturaleza::Otros,
                Naturaleza::Porcentaje,
                Naturaleza::Partida,
            ] {
                let grupo: Vec<_> = e.por_naturaleza(nat).cloned().collect();
                if grupo.is_empty() {
                    continue;
                }
                let subtotal: Decimal = grupo.iter().map(|r| r.importe).sum();
                let titulo = if nat == Naturaleza::Partida {
                    "Partidas de precio directo (sin descomponer)"
                } else {
                    nombre_naturaleza(nat)
                };
                ui.label(
                    RichText::new(format!("{titulo}  —  {} €", eur(subtotal)))
                        .strong()
                        .color(color_naturaleza(nat, ui.visuals().dark_mode)),
                );
                egui::Grid::new(("recursos", nombre_naturaleza(nat)))
                    .striped(true)
                    .num_columns(8)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        for t in [
                            "Código",
                            "Ud",
                            "Resumen",
                            "Cantidad",
                            "Tarifa (PVP − dto.)",
                            "Precio",
                            "Importe",
                            "Origen del precio",
                        ] {
                            ui.label(RichText::new(t).weak());
                        }
                        ui.end_row();
                        for r in &grupo {
                            ui.label(&r.codigo);
                            let mut u = r.unidad.clone();
                            if ui
                                .add(egui::TextEdit::singleline(&mut u).desired_width(34.0))
                                .on_hover_text("Unidad")
                                .changed()
                            {
                                acciones.push(Accion::Unidad(r.codigo.clone(), u));
                            }
                            let mut t = r.resumen.clone();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut t)
                                        .desired_width(300.0)
                                        .hint_text("escriba el resumen…"),
                                )
                                .on_hover_text(&r.resumen)
                                .changed()
                            {
                                acciones.push(Accion::Resumen(r.codigo.clone(), t));
                            }
                            if nat == Naturaleza::Porcentaje {
                                ui.label("");
                                ui.label("");
                                ui.label("");
                            } else if let Some(t) = self.p.conceptos.get(&r.codigo).and_then(|c| c.tarifa.clone()) {
                                // Ligado a tarifa: se edita el PVP; el neto sale del descuento
                                ui.label(num(r.cantidad, 3));
                                ui.horizontal(|ui| {
                                    if let Some(v) = self.edicion.decimal(
                                        ui,
                                        &format!("pvp|{}", r.codigo),
                                        t.pvp,
                                        dec.precio,
                                        ANCHO_NUM,
                                    ) {
                                        acciones.push(Accion::Pvp(r.codigo.clone(), v));
                                    }
                                    ui.label(RichText::new(format!("{} grupo", t.fabricante)).weak());
                                    if let Some(g) = self.edicion.texto(
                                        ui,
                                        &format!("grp|{}", r.codigo),
                                        t.grupo.as_deref().unwrap_or(""),
                                        45.0,
                                    ) {
                                        acciones.push(Accion::GrupoTarifa(r.codigo.clone(), g));
                                    }
                                    ui.label(
                                        RichText::new(format!(
                                            "− {} % ({})",
                                            num(self.p.descuento_tarifa(&t), 2),
                                            self.p.clave_aplicada(&t)
                                        ))
                                        .weak(),
                                    )
                                    .on_hover_text(
                                        "Si el grupo tiene descuento propio (p. ej. S&P/E08) se usa ese; si no, el general del fabricante.",
                                    );
                                });
                                ui.label(eur(r.precio)).on_hover_text(
                                    "Neto = PVP × (1 − descuento). Cambie el PVP o el descuento del fabricante.",
                                );
                            } else {
                                ui.label(num(r.cantidad, 3));
                                ui.label("");
                                if let Some(v) = self.edicion.decimal(
                                    ui,
                                    &format!("rp|{}", r.codigo),
                                    r.precio,
                                    dec.precio,
                                    ANCHO_NUM,
                                ) {
                                    acciones.push(Accion::Precio(r.codigo.clone(), v));
                                }
                            }
                            ui.label(eur(r.importe));
                            if nat == Naturaleza::Porcentaje {
                                ui.label("");
                            } else {
                                self.celda_origen(ui, &r.codigo, acciones);
                            }
                            ui.end_row();
                        }
                    });
                if nat == Naturaleza::ManoObra {
                    let horas: Decimal = grupo.iter().map(|r| r.cantidad).sum();
                    ui.label(RichText::new(format!("Horas de mano de obra: {}", num(horas, 2))).italics());
                }
                ui.add_space(8.0);
            }
            ui.separator();
            ui.label(format!(
                "Total explosión: {} €     PEM: {} €",
                eur(e.total),
                eur(e.coste_directo)
            ));
            let txt = format!("Descuadre de redondeo: {} €", eur(e.descuadre_redondeo));
            ui.label(RichText::new(txt).weak()).on_hover_text(
                "El PEM suma importes de línea redondeados; la explosión multiplica cantidades totales. \
                 La diferencia se muestra, no se oculta.",
            );
        });
    }

    pub(super) fn pestana_subcontratar(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        egui::CollapsingHeader::new(
            RichText::new("Subcontrata a ratio (se aplica al presupuesto)")
                .strong()
                .size(15.0),
        )
        .default_open(true)
        .show(ui, |ui| self.form_ratio(ui, acciones));
        ui.separator();
        ui.label(RichText::new("Comparar ofertas (simulación)").strong().size(15.0));
        let dec = self.p.decimales;
        ui.label("Simula contratar fuera parte de una partida (normalmente la mano de obra). El presupuesto original no cambia.");
        ui.add_space(6.0);

        // ---- formulario
        let partidas: Vec<String> = self.calc.cantidades.keys().cloned().collect();
        if self.form.partida.is_empty() || !partidas.contains(&self.form.partida) {
            let preferida = self
                .sel
                .as_ref()
                .map(|(_, h)| h.clone())
                .filter(|h| partidas.contains(h));
            if let Some(p) = preferida.as_ref().or(partidas.first()) {
                self.form.partida = p.clone();
                self.form.recursos.clear();
            }
        }
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Nueva oferta").strong());
            ui.horizontal(|ui| {
                ui.label("Código");
                ui.add(egui::TextEdit::singleline(&mut self.form.codigo).desired_width(70.0));
                ui.label("Descripción");
                ui.add(egui::TextEdit::singleline(&mut self.form.descripcion).desired_width(300.0));
            });
            ui.horizontal(|ui| {
                ui.label("Partida");
                let antes = self.form.partida.clone();
                let etiquetas: Vec<(String, String)> =
                    partidas.iter().map(|p| (p.clone(), self.etiqueta_partida(p))).collect();
                let actual = self.etiqueta_partida(&self.form.partida);
                egui::ComboBox::from_id_salt("partida_sub")
                    .width(360.0)
                    .selected_text(actual)
                    .show_ui(ui, |ui| {
                        for (p, t) in etiquetas {
                            ui.selectable_value(&mut self.form.partida, p, t);
                        }
                    });
                if antes != self.form.partida {
                    self.form.recursos.clear();
                }
            });
            if let Some(lineas) = self.calc.lineas.get(&self.form.partida).cloned() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sustituye:");
                    for l in lineas.iter().filter(|l| l.naturaleza != Naturaleza::Porcentaje) {
                        let marcado = self
                            .form
                            .recursos
                            .entry(l.hijo.clone())
                            .or_insert(l.naturaleza == Naturaleza::ManoObra);
                        ui.checkbox(marcado, format!("{} ({} €)", l.hijo, eur(l.importe)));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.form.alzado, false, "Por unidad");
                ui.radio_value(&mut self.form.alzado, true, "Alzado");
                ui.separator();
                if self.form.alzado {
                    ui.label("Importe €");
                    if let Some(v) = self.edicion.decimal(ui, "f|importe", self.form.importe, 2, 90.0) {
                        self.form.importe = v;
                    }
                } else {
                    ui.label("Unidad");
                    ui.add(egui::TextEdit::singleline(&mut self.form.unidad).desired_width(40.0));
                    ui.label("Precio €");
                    if let Some(v) = self
                        .edicion
                        .decimal(ui, "f|precio", self.form.precio, dec.precio, ANCHO_NUM)
                    {
                        self.form.precio = v;
                    }
                    let ud_p = self
                        .p
                        .conceptos
                        .get(&self.form.partida)
                        .map(|c| c.unidad.clone())
                        .unwrap_or_default();
                    ui.label(format!("{} por {ud_p} de partida", self.form.unidad));
                    if let Some(v) = self.edicion.decimal(ui, "f|factor", self.form.factor, 3, 60.0) {
                        self.form.factor = v;
                    }
                }
                ui.separator();
                let recursos: Vec<String> = self
                    .form
                    .recursos
                    .iter()
                    .filter(|(_, v)| **v)
                    .map(|(k, _)| k.clone())
                    .collect();
                if ui
                    .add_enabled(!recursos.is_empty(), egui::Button::new("+ Añadir oferta"))
                    .clicked()
                {
                    let contratacion = if self.form.alzado {
                        Contratacion::Alzado {
                            importe: self.form.importe,
                        }
                    } else {
                        Contratacion::PorUnidad {
                            unidad: self.form.unidad.clone(),
                            precio: self.form.precio,
                        }
                    };
                    self.ofertas.push(PaqueteTrabajo {
                        codigo: self.form.codigo.clone(),
                        descripcion: self.form.descripcion.clone(),
                        contratacion,
                        asignaciones: vec![Asignacion {
                            partida: self.form.partida.clone(),
                            recursos,
                            factor_conversion: if self.form.alzado {
                                Decimal::ZERO
                            } else {
                                self.form.factor
                            },
                        }],
                        incluye: vec![],
                        excluye: vec![],
                    });
                    self.form.codigo = format!("SUB-{}", self.ofertas.len() + 1);
                }
            });
        });

        // ---- comparación
        ui.add_space(8.0);
        ui.label(
            RichText::new("Comparación de ofertas (cada una por separado frente a ejecutarlo con medios propios)")
                .strong(),
        );
        let resultados: Vec<(usize, &PaqueteTrabajo, Result<ResultadoEscenario, String>)> = self
            .ofertas
            .iter()
            .enumerate()
            .map(|(i, o)| {
                (
                    i,
                    o,
                    simular(&self.p, std::slice::from_ref(o)).map_err(|e| e.to_string()),
                )
            })
            .collect();
        let mut mejor: Option<(usize, Decimal)> = None;
        for (i, _, r) in &resultados {
            if let Ok(r) = r
                && mejor.is_none_or(|(_, a)| r.ahorro > a)
            {
                mejor = Some((*i, r.ahorro));
            }
        }
        let mut notas: Vec<String> = Vec::new();
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("ofertas").striped(true).num_columns(9).show(ui, |ui| {
                for t in [
                    "Oferta",
                    "Contratación",
                    "Partida",
                    "Coste propio",
                    "Contratado",
                    "Equivale a",
                    "PEM escenario",
                    "Ahorro",
                    "Horas lib.",
                ] {
                    ui.label(RichText::new(t).weak());
                }
                ui.end_row();
                for (i, o, r) in &resultados {
                    let partida = o.asignaciones.first().map(|a| a.partida.clone()).unwrap_or_default();
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").on_hover_text("Quitar oferta").clicked() {
                            acciones.push(Accion::QuitarOferta(*i));
                        }
                        let t = format!("{} {}", o.codigo, corto(&o.descripcion, 18));
                        if mejor.map(|m| m.0) == Some(*i) {
                            ui.label(RichText::new(format!("★ {t}")).strong());
                        } else {
                            ui.label(t);
                        }
                    });
                    ui.label(match &o.contratacion {
                        Contratacion::PorUnidad { unidad, precio } => format!("{} €/{unidad}", eur(*precio)),
                        Contratacion::Alzado { importe } => format!("alzado {} €", eur(*importe)),
                    });
                    ui.label(&partida);
                    match r {
                        Ok(r) => {
                            let ud = self
                                .p
                                .conceptos
                                .get(&partida)
                                .map(|c| c.unidad.clone())
                                .unwrap_or_default();
                            let eq = r.precio_equivalente(&o.codigo, &partida, 2).unwrap_or_default();
                            let horas: Decimal = r.horas_liberadas.values().copied().sum();
                            ui.label(eur(r.coste_retirado));
                            ui.label(eur(r.coste_contratado));
                            ui.label(format!("{} €/{ud}", eur(eq)));
                            ui.label(eur(r.coste_escenario));
                            let color = if r.ahorro >= Decimal::ZERO {
                                Color32::from_rgb(70, 170, 90)
                            } else {
                                Color32::from_rgb(220, 80, 60)
                            };
                            ui.label(RichText::new(eur(r.ahorro)).color(color));
                            ui.label(num(horas, 1));
                            for n in &r.notas {
                                // «SUB-A: texto» → «texto», para no repetir la misma nota por oferta
                                let t = n.split_once(": ").map_or(n.as_str(), |(_, t)| t).to_owned();
                                if !notas.contains(&t) {
                                    notas.push(t);
                                }
                            }
                        }
                        Err(e) => {
                            ui.label(RichText::new(e).color(Color32::from_rgb(220, 80, 60)));
                        }
                    }
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            for n in &notas {
                ui.label(RichText::new(format!("Nota: {n}")).weak());
            }
        });
    }

    pub(super) fn pestana_resumen(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let pem = self.calc.pem;
        let cd = self.calc.coste_directo;
        ui.label(RichText::new("Costes indirectos").strong());
        ui.horizontal(|ui| {
            ui.label("Costes indirectos %");
            if let Some(n) = self.edicion.decimal(ui, "pct|ci", self.p.costes_indirectos, 2, 60.0) {
                acciones.push(Accion::Ci(n));
            }
            let mut o = self.p.opciones_ci;
            ui.checkbox(&mut o.redondear_coste_antes, "Redondear coste antes de aplicarlos");
            ui.checkbox(&mut o.aplicar_a_sin_descomponer, "Aplicar a partidas sin descomponer");
            ui.checkbox(
                &mut o.redondear_auxiliares,
                "Redondear partidas que actúan como auxiliares",
            );
            if o != self.p.opciones_ci {
                acciones.push(Accion::OpcionesCi(o));
            }
        });
        egui::Grid::new("ci")
            .striped(true)
            .num_columns(2)
            .min_col_width(220.0)
            .show(ui, |ui| {
                ui.label("Coste directo");
                ui.label(format!("{} €", eur(cd)));
                ui.end_row();
                ui.label(format!(
                    "Costes indirectos ({} % por partida)",
                    num(self.p.costes_indirectos, 2)
                ));
                ui.label(format!("{} €", eur(pem - cd)));
                ui.end_row();
                ui.label(RichText::new("PEM").strong());
                ui.label(RichText::new(format!("{} €", eur(pem))).strong());
                ui.end_row();
            });
        if !self.p.costes_indirectos.is_zero() && !cd.is_zero() {
            let margen = (pem - cd) / pem * Decimal::ONE_HUNDRED;
            ui.label(
                RichText::new(format!(
                    "Sobre el PEM, los indirectos son un {} % (margen sobre venta). Aplicados al total darían {} €; \
                     la diferencia es el redondeo por partida.",
                    num(margen, 2),
                    eur(venta::con_costes_indirectos(cd, self.p.costes_indirectos, 2))
                ))
                .weak(),
            );
        }
        ui.add_space(12.0);
        ui.label(RichText::new("Resumen del presupuesto").strong());
        egui::Grid::new("resumen_pct").num_columns(3).show(ui, |ui| {
            for (nombre, clave) in [
                ("Gastos generales %", "gg"),
                ("Beneficio industrial %", "bi"),
                ("IVA %", "iva"),
                ("Retención de garantía % (certificaciones)", "ret"),
            ] {
                ui.label(nombre);
                let v = match clave {
                    "gg" => &mut self.gg,
                    "bi" => &mut self.bi,
                    "ret" => &mut self.retencion,
                    _ => &mut self.iva,
                };
                if let Some(n) = self.edicion.decimal(ui, &format!("pct|{clave}"), *v, 2, 60.0) {
                    *v = n;
                }
                ui.end_row();
            }
        });
        ui.label(
            RichText::new(format!(
                "Sobrecoste de la obra: {} % (GG + BI). Es el que llevan todas las partidas salvo las que tengan el suyo.",
                num(self.gg + self.bi, 2)
            ))
            .strong(),
        );
        let desc = self.p.seguimiento.descuento.clone();
        let r = venta::resumen_con_descuento(pem, self.gg, self.bi, self.iva, desc.as_ref(), 2);
        self.editor_descuento(ui, acciones);
        ui.add_space(6.0);
        egui::Grid::new("resumen")
            .striped(true)
            .num_columns(2)
            .min_col_width(220.0)
            .show(ui, |ui| {
                let filas = [
                    ("Presupuesto de ejecución material".to_string(), r.pem, false),
                    (
                        format!("{} % Gastos generales", num(self.gg, 2)),
                        r.gastos_generales,
                        false,
                    ),
                    (
                        format!("{} % Beneficio industrial", num(self.bi, 2)),
                        r.beneficio_industrial,
                        false,
                    ),
                    ("Suma".into(), r.bruto, false),
                    (
                        desc.as_ref()
                            .map_or_else(|| "Descuento comercial".to_owned(), |d| d.etiqueta()),
                        -r.descuento,
                        false,
                    ),
                    ("Presupuesto base".into(), r.base, true),
                    (format!("{} % IVA", num(self.iva, 2)), r.iva, false),
                    ("TOTAL".into(), r.total, true),
                ];
                for (i, (t, v, fuerte)) in filas.into_iter().enumerate() {
                    // Suma y descuento, solo si hay descuento
                    if (i == 3 || i == 4) && r.descuento.is_zero() {
                        continue;
                    }
                    let (a, b) = (RichText::new(t), RichText::new(format!("{} €", eur(v))));
                    if fuerte {
                        ui.label(a.strong());
                        ui.label(b.strong());
                    } else {
                        ui.label(a);
                        ui.label(b);
                    }
                    ui.end_row();
                }
            });

        ui.add_space(14.0);
        egui::CollapsingHeader::new(RichText::new("Coste y venta partida a partida").strong())
            .default_open(true)
            .show(ui, |ui| self.tabla_coste_venta(ui));
    }
}

impl Aplicacion {
    /// Descuento comercial de la oferta: % o cerrar a una cifra.
    fn editor_descuento(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        use super::obra::OpObra;
        use ppto_core::venta::DescuentoComercial;
        let actual = self.p.seguimiento.descuento.clone();
        let (venta, _) = self.p.venta_y_descuento(self.gg, self.bi).unwrap_or_default();
        ui.add_space(6.0);
        ui.label(RichText::new("Descuento comercial (cierre de la oferta)").strong());
        ui.horizontal_wrapped(|ui| {
            ui.label("%");
            let pct = actual.as_ref().map(|d| d.porcentaje);
            if let Some(n) = self.edicion.opcional(ui, "descuento|pct", pct, 2, 60.0) {
                acciones.push(Accion::Obra(OpObra::Descuento(n.map(|p| DescuentoComercial {
                    concepto: actual.as_ref().map(|d| d.concepto.clone()).unwrap_or_default(),
                    ..DescuentoComercial::porcentaje(p)
                }))));
            }
            ui.separator();
            ui.label("o cerrar en");
            ui.add(
                egui::TextEdit::singleline(&mut self.cerrar_en)
                    .hint_text("€ sin IVA")
                    .desired_width(90.0),
            );
            let objetivo = ppto_core::formato::leer(&self.cerrar_en);
            if ui
                .add_enabled(objetivo.is_some(), egui::Button::new("Cerrar"))
                .on_hover_text(format!(
                    "Calcula el descuento para que la oferta (venta {} € sin IVA) quede en esa cifra",
                    eur(venta)
                ))
                .clicked()
            {
                match objetivo.and_then(|o| DescuentoComercial::cerrar_en(venta, o)) {
                    Some(mut d) => {
                        d.concepto = actual.as_ref().map(|d| d.concepto.clone()).unwrap_or_default();
                        acciones.push(Accion::Obra(OpObra::Descuento(Some(d))));
                        self.cerrar_en.clear();
                    }
                    None => self.error(format!("La cifra debe estar entre 0 y la venta ({} €).", eur(venta))),
                }
            }
            if actual.is_some() {
                ui.separator();
                ui.label("Texto");
                let c = actual.as_ref().map(|d| d.concepto.clone()).unwrap_or_default();
                if let Some(t) = self.edicion.texto(ui, "descuento|concepto", &c, 160.0)
                    && let Some(mut d) = actual.clone()
                {
                    d.concepto = t;
                    acciones.push(Accion::Obra(OpObra::Descuento(Some(d))));
                }
                if ui.button("Quitar").clicked() {
                    acciones.push(Accion::Obra(OpObra::Descuento(None)));
                }
            }
        });
        ui.label(
            RichText::new(
                "Los precios de las partidas no cambian: sale como una línea en la oferta (PDF y Excel) y se aplica en el mismo % a cada certificación.",
            )
            .small()
            .weak(),
        );
    }
}
