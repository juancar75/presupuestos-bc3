// SPDX-License-Identifier: GPL-3.0-or-later
//! Ficha de la partida: texto, descompuesto, medición, coste y venta.

use super::*;

impl Aplicacion {
    pub(super) fn pestana_partida(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let Some((padre, hijo)) = self.sel.clone() else {
            ui.label("Selecciona una partida en el árbol de la izquierda.");
            return;
        };
        let Some(c) = self.p.conceptos.get(&hijo).cloned() else {
            return;
        };
        let dec = self.p.decimales;
        let linea_cap = self
            .calc
            .lineas
            .get(&padre)
            .and_then(|v| v.iter().find(|l| l.hijo == hijo))
            .cloned();

        let padre_es_capitulo = self
            .p
            .conceptos
            .get(&padre)
            .is_some_and(|p| p.naturaleza == Naturaleza::Capitulo);
        ui.horizontal(|ui| {
            let orden = self.orden_partidas();
            let pos = orden.iter().position(|x| x.0 == padre && x.1 == hijo);
            if ui
                .add_enabled(pos.is_some_and(|i| i > 0), egui::Button::new("< Anterior"))
                .on_hover_text("Ctrl+↑")
                .clicked()
            {
                acciones.push(Accion::Navegar(-1));
            }
            if ui
                .add_enabled(
                    pos.is_none_or(|i| i + 1 < orden.len()),
                    egui::Button::new("Siguiente >"),
                )
                .on_hover_text("Ctrl+↓ (pulse Intro antes para confirmar el precio)")
                .clicked()
            {
                acciones.push(Accion::Navegar(1));
            }
            if ui
                .button("Siguiente sin precio >>")
                .on_hover_text("Ctrl+Mayús+↓")
                .clicked()
            {
                acciones.push(Accion::SiguienteSinPrecio);
            }
            if let Some(i) = pos {
                let sin = orden
                    .iter()
                    .filter(|(_, h)| self.calc.precios.get(h).is_none_or(|p| p.is_zero()))
                    .count();
                ui.label(RichText::new(format!("{} de {} · {sin} sin precio", i + 1, orden.len())).weak());
            }
        });
        ui.horizontal(|ui| {
            if !padre_es_capitulo
                && ui
                    .button(format!("<< Volver a {padre}"))
                    .on_hover_text("Subir al nivel anterior")
                    .clicked()
                && let Some(abuelo) = self.p.padres(&padre).into_iter().next()
            {
                acciones.push(Accion::Seleccionar(abuelo, padre.clone()));
            }
            let mut ruta = self.p.ruta(&padre);
            ruta.push(hijo.clone());
            ui.label(RichText::new(ruta.join("  >  ")).weak().small());
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(&c.codigo).strong().size(16.0));
            let mut unidad = c.unidad.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut unidad).desired_width(40.0))
                .on_hover_text("Unidad")
                .changed()
            {
                acciones.push(Accion::Unidad(c.codigo.clone(), unidad));
            }
            let mut resumen = c.resumen.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut resumen).desired_width(480.0))
                .changed()
            {
                acciones.push(Accion::Resumen(c.codigo.clone(), resumen));
            }
        });
        if let Some(l) = &linea_cap {
            ui.label(
                RichText::new(format!(
                    "Cantidad {} {}   ·   Precio {} €/{}   ·   Importe {} €",
                    cant(l.cantidad, dec.medicion),
                    c.unidad,
                    eur(l.precio),
                    c.unidad,
                    eur(l.importe)
                ))
                .size(15.0),
            );
            self.coste_y_venta(ui, &hijo, &c.unidad, l, acciones);
        }
        ui.add_space(6.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("Texto descriptivo").strong())
                .id_salt(("texto", &hijo))
                .default_open(true)
                .show(ui, |ui| {
                    let mut texto = c.texto.clone().unwrap_or_default();
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut texto)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3)
                            .hint_text("Descripción completa de la partida (texto largo)"),
                    );
                    if r.changed() {
                        acciones.push(Accion::Texto(c.codigo.clone(), texto));
                    }
                });
            ui.add_space(6.0);
            ui.label(RichText::new("Precio descompuesto").strong());
            if let Some(lineas) = self.calc.lineas.get(&hijo).cloned() {
                egui::Grid::new("descompuesto")
                    .striped(true)
                    .num_columns(8)
                    .show(ui, |ui| {
                        for t in ["Código", "Ud", "Resumen", "Tipo", "Cantidad", "Precio", "Importe", ""] {
                            ui.label(RichText::new(t).weak());
                        }
                        ui.end_row();
                        for l in &lineas {
                            let h = &self.p.conceptos[&l.hijo];
                            let y0 = ui.cursor().top();
                            if h.naturaleza == Naturaleza::Partida {
                                if ui
                                    .link(&h.codigo)
                                    .on_hover_text("Entrar en la subpartida para ver o editar su descompuesto")
                                    .clicked()
                                {
                                    acciones.push(Accion::Seleccionar(hijo.clone(), l.hijo.clone()));
                                }
                            } else {
                                ui.label(&h.codigo);
                            }
                            // Unidad y resumen editables en los recursos (una subpartida
                            // se edita entrando en ella).
                            let editable = h.naturaleza != Naturaleza::Partida;
                            let campo_resumen = |ui: &mut Ui, acciones: &mut Vec<Accion>| {
                                if editable {
                                    let mut r = h.resumen.clone();
                                    if ui
                                        .add(
                                            egui::TextEdit::singleline(&mut r)
                                                .desired_width(260.0)
                                                .hint_text("escriba el resumen…"),
                                        )
                                        .on_hover_text(&h.resumen)
                                        .changed()
                                    {
                                        acciones.push(Accion::Resumen(h.codigo.clone(), r));
                                    }
                                } else {
                                    ui.label(corto(&h.resumen, 46)).on_hover_text(&h.resumen);
                                }
                            };
                            if editable {
                                let mut u = h.unidad.clone();
                                if ui
                                    .add(egui::TextEdit::singleline(&mut u).desired_width(34.0))
                                    .on_hover_text("Unidad")
                                    .changed()
                                {
                                    acciones.push(Accion::Unidad(h.codigo.clone(), u));
                                }
                            } else {
                                ui.label(&h.unidad);
                            }
                            match &h.tarifa {
                                // Material de tarifa: PVP y descuento a la vista (en el BC3
                                // sale a PVP con su línea «Descuento …»).
                                Some(t) => {
                                    let d = self.p.descuento_tarifa(t).normalize();
                                    ui.vertical(|ui| {
                                        campo_resumen(ui, acciones);
                                        ui.label(
                                            RichText::new(format!(
                                                "PVP {} € − {} % {}",
                                                num(t.pvp, dec.precio),
                                                num(d, d.scale()),
                                                self.p.clave_aplicada(t)
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                    });
                                }
                                None => campo_resumen(ui, acciones),
                            }
                            ui.label(
                                RichText::new(nombre_naturaleza(h.naturaleza))
                                    .color(color_naturaleza(h.naturaleza, ui.visuals().dark_mode)),
                            );
                            if let Some(v) = self.edicion.decimal(
                                ui,
                                &format!("r|{hijo}|{}", l.hijo),
                                l.cantidad,
                                dec.rendimiento,
                                ANCHO_NUM,
                            ) {
                                acciones.push(Accion::Rendimiento(hijo.clone(), l.hijo.clone(), v));
                            }
                            if h.naturaleza.es_basico() {
                                if let Some(v) =
                                    self.edicion
                                        .decimal(ui, &format!("p|{}", l.hijo), l.precio, dec.precio, ANCHO_NUM)
                                {
                                    acciones.push(Accion::Precio(l.hijo.clone(), v));
                                }
                            } else {
                                let nota = if h.naturaleza == Naturaleza::Porcentaje {
                                    " (base)"
                                } else {
                                    ""
                                };
                                ui.label(format!("{}{nota}", eur(l.precio)));
                            }
                            ui.label(eur(l.importe));
                            ui.horizontal(|ui| {
                                if ui
                                    .small_button("+")
                                    .on_hover_text("Insertar una línea encima de esta (F5)")
                                    .clicked()
                                {
                                    acciones.push(Accion::Insertar(
                                        TipoAlta::Linea { padre: hijo.clone() },
                                        l.hijo.clone(),
                                    ));
                                }
                                if ui.small_button("^").on_hover_text("Subir").clicked() {
                                    acciones.push(Accion::Mover(hijo.clone(), l.hijo.clone(), true));
                                }
                                if ui.small_button("v").on_hover_text("Bajar").clicked() {
                                    acciones.push(Accion::Mover(hijo.clone(), l.hijo.clone(), false));
                                }
                                if ui
                                    .small_button("x")
                                    .on_hover_text("Quitar la línea (el recurso sigue en la obra)")
                                    .clicked()
                                {
                                    acciones.push(Accion::QuitarLinea(hijo.clone(), l.hijo.clone(), false));
                                }
                            });
                            // F5 con el ratón en esta fila: línea nueva encima
                            let y1 = ui.min_rect().bottom();
                            if ui.rect_contains_pointer(egui::Rect::from_x_y_ranges(ui.min_rect().x_range(), y0..=y1)) {
                                self.f5.replace(Some(Accion::Insertar(
                                    TipoAlta::Linea { padre: hijo.clone() },
                                    l.hijo.clone(),
                                )));
                            }
                            ui.end_row();
                        }
                        for _ in 0..5 {
                            ui.label("");
                        }
                        ui.label(RichText::new("Precio").strong());
                        ui.label(
                            RichText::new(eur(self.calc.precios.get(&hijo).copied().unwrap_or_default())).strong(),
                        );
                        ui.end_row();
                    });
                if ui.button("+ Añadir línea…").clicked() {
                    acciones.push(Accion::Alta(TipoAlta::Linea { padre: hijo.clone() }));
                }
            } else if c.naturaleza == Naturaleza::Partida {
                ui.horizontal(|ui| {
                    ui.label("Precio directo (sin descomponer):");
                    if let Some(v) = self
                        .edicion
                        .decimal(ui, &format!("pd|{hijo}"), c.precio, dec.precio, ANCHO_NUM)
                    {
                        acciones.push(Accion::Precio(hijo.clone(), v));
                    }
                    ui.label(RichText::new(format!("€/{}", c.unidad)).weak());
                });
                ui.label(RichText::new("O descompóngala:").weak());
                if ui.button("+ Añadir línea…").clicked() {
                    acciones.push(Accion::Alta(TipoAlta::Linea { padre: hijo.clone() }));
                }
            } else {
                ui.label(RichText::new("Recurso básico sin descomposición.").weak());
            }

            ui.add_space(12.0);
            if !padre_es_capitulo {
                // Subpartida: su cantidad es el rendimiento dentro de la partida superior
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Cantidad en {padre}:")).strong());
                    if let Some(l) = &linea_cap
                        && let Some(v) = self.edicion.decimal(
                            ui,
                            &format!("q|{padre}|{hijo}"),
                            l.cantidad,
                            dec.rendimiento,
                            ANCHO_NUM,
                        )
                    {
                        acciones.push(Accion::Rendimiento(padre.clone(), hijo.clone(), v));
                    }
                    ui.label(RichText::new(&c.unidad).weak());
                });
                for a in &self.avisos {
                    ui.label(RichText::new(a.to_string()).color(Color32::from_rgb(220, 150, 40)));
                }
                return;
            }
            ui.label(RichText::new(format!("Medición en {}", padre)).strong());
            let clave = (padre.clone(), hijo.clone());
            match self.p.mediciones.get(&clave).cloned() {
                Some(m) => self.hoja_medicion(ui, &padre, &hijo, &c.unidad, m, acciones),
                None => {
                    ui.horizontal(|ui| {
                        ui.label("Cantidad sin hoja de medición:");
                        if let Some(l) = &linea_cap
                            && let Some(v) = self.edicion.decimal(
                                ui,
                                &format!("q|{padre}|{hijo}"),
                                l.cantidad,
                                dec.medicion,
                                ANCHO_NUM,
                            )
                        {
                            acciones.push(Accion::Rendimiento(padre.clone(), hijo.clone(), v));
                        }
                        if ui.button("Crear hoja de medición").clicked() {
                            let q = linea_cap.as_ref().map(|l| l.cantidad).unwrap_or(Decimal::ONE);
                            let m = Medicion::new(vec![LineaMedicion::normal("", Some(q), None, None, None)]);
                            acciones.push(Accion::Medicion(padre.clone(), hijo.clone(), m));
                        }
                    });
                }
            }
            for a in &self.avisos {
                ui.label(RichText::new(a.to_string()).color(Color32::from_rgb(220, 150, 40)));
            }
        });
    }

    pub(super) fn hoja_medicion(
        &mut self,
        ui: &mut Ui,
        padre: &str,
        hijo: &str,
        unidad: &str,
        m: Medicion,
        acciones: &mut Vec<Accion>,
    ) {
        let dec = self.p.decimales;
        let res = m.calcular(&dec, unidad);
        let parciales = res.as_ref().map(|r| r.parciales.clone()).unwrap_or_default();
        let mut nueva = m.clone();
        let mut cambio = false;
        let mut quitar = None;
        egui::Grid::new("medicion").striped(true).num_columns(8).show(ui, |ui| {
            for t in [
                "Comentario",
                "Uds",
                "Longitud",
                "Anchura",
                "Altura",
                "Fórmula",
                "Parcial",
                "",
            ] {
                ui.label(RichText::new(t).weak());
            }
            ui.end_row();
            for (i, l) in nueva.lineas.iter_mut().enumerate() {
                let mut com = l.comentario.clone();
                if ui
                    .add_sized([200.0, 20.0], egui::TextEdit::singleline(&mut com))
                    .lost_focus()
                    && com != l.comentario
                {
                    l.comentario = com;
                    cambio = true;
                }
                let subtotal = matches!(l.tipo, TipoLinea::SubtotalParcial | TipoLinea::SubtotalAcumulado);
                if subtotal {
                    for _ in 0..5 {
                        ui.label("");
                    }
                } else {
                    let campos: [(&str, &mut Option<Decimal>, u32); 4] = [
                        ("u", &mut l.unidades, dec.dimensiones),
                        ("l", &mut l.longitud, dec.dimensiones),
                        ("a", &mut l.anchura, dec.dimensiones),
                        ("h", &mut l.altura, dec.dimensiones),
                    ];
                    for (k, v, d) in campos {
                        if let Some(n) = self
                            .edicion
                            .opcional(ui, &format!("m|{padre}|{hijo}|{i}|{k}"), *v, d, 70.0)
                        {
                            *v = n;
                            cambio = true;
                        }
                    }
                    let mut f = l.formula.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut f)
                                .desired_width(110.0)
                                .hint_text("p.ej. a*b*c"),
                        )
                        .on_hover_text("Variables: a=uds, b=longitud, c=anchura, d=altura, p=π")
                        .lost_focus()
                    {
                        let f = f.trim().to_owned();
                        let nueva_f = (!f.is_empty()).then_some(f);
                        if nueva_f != l.formula {
                            l.tipo = if nueva_f.is_some() {
                                TipoLinea::Formula
                            } else {
                                TipoLinea::Normal
                            };
                            l.formula = nueva_f;
                            cambio = true;
                        }
                    }
                }
                let parcial = parciales.get(i).copied().unwrap_or_default();
                let txt = RichText::new(cant(parcial, dec.medicion));
                ui.label(if subtotal { txt.italics() } else { txt });
                if ui.small_button("✖").on_hover_text("Quitar línea").clicked() {
                    quitar = Some(i);
                }
                ui.end_row();
            }
            for _ in 0..5 {
                ui.label("");
            }
            ui.label(RichText::new("Total").strong());
            let total = res.as_ref().map(|r| r.total).unwrap_or_default();
            ui.label(RichText::new(format!("{} {unidad}", cant(total, dec.medicion))).strong());
            ui.end_row();
        });
        if let Err(e) = &res {
            ui.label(RichText::new(e.to_string()).color(Color32::from_rgb(220, 80, 60)));
        }
        ui.horizontal(|ui| {
            if ui.button("+ Añadir línea").clicked() {
                nueva
                    .lineas
                    .push(LineaMedicion::normal("", Some(Decimal::ONE), None, None, None));
                cambio = true;
            }
            if ui.button("+ Subtotal").clicked() {
                nueva.lineas.push(LineaMedicion::subtotal("Subtotal"));
                cambio = true;
            }
        });
        if let Some(i) = quitar {
            nueva.lineas.remove(i);
            cambio = true;
        }
        if cambio {
            acciones.push(Accion::Medicion(padre.to_owned(), hijo.to_owned(), nueva));
        }
    }

    /// Coste y venta de la partida seleccionada, paso a paso: coste directo
    /// (por naturaleza) → costes indirectos → PEM → GG → BI → venta.
    pub(super) fn coste_y_venta(
        &mut self,
        ui: &mut Ui,
        hijo: &str,
        unidad: &str,
        l: &LineaValorada,
        acciones: &mut Vec<Accion>,
    ) {
        let d = self.p.decimales.importe;
        let q = l.cantidad;
        let coste = self.calc.precios.get(hijo).copied().unwrap_or_default();
        let Some(concepto) = self.p.conceptos.get(hijo).cloned() else {
            return;
        };
        let margen_propio = concepto.margen;
        let fija = concepto.venta_fija;
        let v = venta::venta_concepto(l.precio, &concepto, self.gg, self.bi, d);
        let imp = |x: Decimal| ppto_core::redondear(x * q, d);
        let (mut mo, mut mat, mut sub, mut maq, mut otr) = (
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        );
        for lv in self.calc.lineas.get(hijo).into_iter().flatten() {
            match lv.naturaleza {
                Naturaleza::ManoObra => mo += lv.importe,
                Naturaleza::Material => mat += lv.importe,
                Naturaleza::Subcontrata => sub += lv.importe,
                Naturaleza::Maquinaria => maq += lv.importe,
                _ => otr += lv.importe,
            }
        }
        egui::CollapsingHeader::new(RichText::new(format!(
            "Coste {} €/{unidad}  →  Venta {} €/{unidad}   ·   Importe venta {} €",
            eur(coste),
            eur(v.venta),
            eur(imp(v.venta))
        )))
        .id_salt(("coste_venta", hijo))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(("cv", hijo))
                .striped(true)
                .num_columns(3)
                .min_col_width(110.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Concepto").weak());
                    ui.label(RichText::new(format!("€/{unidad}")).weak());
                    ui.label(RichText::new(format!("× {} = €", cant(q, self.p.decimales.medicion))).weak());
                    ui.end_row();
                    let fila = |ui: &mut Ui, t: String, u: Decimal, fuerte: bool| {
                        let (a, b, c) = (RichText::new(t), RichText::new(eur(u)), RichText::new(eur(imp(u))));
                        if fuerte {
                            ui.label(a.strong());
                            ui.label(b.strong());
                            ui.label(c.strong());
                        } else {
                            ui.label(a);
                            ui.label(b);
                            ui.label(c);
                        }
                        ui.end_row();
                    };
                    for (t, x) in [
                        ("   Mano de obra", mo),
                        ("   Material", mat),
                        ("   Subcontrata", sub),
                        ("   Maquinaria", maq),
                        ("   Otros y medios auxiliares", otr),
                    ] {
                        if !x.is_zero() {
                            fila(ui, t.into(), x, false);
                        }
                    }
                    fila(ui, "Coste directo".into(), coste, true);
                    if l.precio != coste {
                        fila(
                            ui,
                            format!("+ Costes indirectos {} %", num(self.p.costes_indirectos, 2)),
                            l.precio - coste,
                            false,
                        );
                        fila(ui, "= Precio de ejecución material".into(), l.precio, true);
                    }
                    match v.margen_propio {
                        _ if v.bloqueada => fila(ui, "+ Hasta la venta bloqueada 🔒".into(), v.margen, false),
                        Some(m) => fila(
                            ui,
                            format!("+ Sobrecoste de esta partida {} %", num(m, 2)),
                            v.margen,
                            false,
                        ),
                        None => {
                            fila(
                                ui,
                                format!("+ Gastos generales {} %", num(self.gg, 2)),
                                v.gastos_generales,
                                false,
                            );
                            fila(
                                ui,
                                format!("+ Beneficio industrial {} %", num(self.bi, 2)),
                                v.beneficio_industrial,
                                false,
                            );
                        }
                    }
                    fila(ui, "= VENTA (sin IVA)".into(), v.venta, true);
                });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sobrecoste de esta partida %").strong());
                // Relleno con el de la obra; si se escribe otro, la partida tiene el suyo
                let general = self.gg + self.bi;
                let mostrado = margen_propio.unwrap_or(general);
                if let Some(n) = self.edicion.decimal(ui, &format!("margen|{hijo}"), mostrado, 2, 60.0) {
                    let nuevo = (n != general).then_some(n);
                    if nuevo != margen_propio {
                        acciones.push(Accion::Margen(hijo.to_owned(), nuevo));
                    }
                }
                if margen_propio.is_some() {
                    ui.label(RichText::new("propio").strong());
                    if ui
                        .small_button(format!("Volver al de la obra ({} %)", num(general, 2)))
                        .clicked()
                    {
                        acciones.push(Accion::Margen(hijo.to_owned(), None));
                    }
                } else {
                    ui.label(RichText::new("el de la obra (se cambia en «Resumen y venta»)").weak());
                }
                if fija.is_some() {
                    ui.label(RichText::new("(no se usa: la venta está bloqueada)").weak());
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Venta bloqueada €/{unidad}")).strong());
                if let Some(n) = self.edicion.opcional(ui, &format!("venta_fija|{hijo}"), fija, 2, 80.0) {
                    acciones.push(Accion::VentaFija(hijo.to_owned(), n));
                }
                match fija {
                    Some(_) => {
                        ui.label(RichText::new("🔒 bloqueada").strong());
                        if ui.small_button("Desbloquear").clicked() {
                            acciones.push(Accion::VentaFija(hijo.to_owned(), None));
                        }
                    }
                    None => {
                        if ui
                            .small_button(format!("🔒 Bloquear en {} €", eur(v.venta)))
                            .on_hover_text("La partida se venderá siempre a este precio aunque cambie su coste")
                            .clicked()
                        {
                            acciones.push(Accion::VentaFija(hijo.to_owned(), Some(v.venta)));
                        }
                    }
                }
            });
            let (sc, mv) = sobrecoste_y_margen(imp(coste), imp(v.venta));
            let ganancia = imp(v.venta) - imp(coste);
            let texto = RichText::new(format!(
                "Sobrecoste {} %   ·   Margen sobre venta {} %   ·   {} {} € en esta partida",
                sc.map(|x| num(x, 2)).unwrap_or_else(|| "—".into()),
                mv.map(|x| num(x, 2)).unwrap_or_else(|| "—".into()),
                if ganancia < Decimal::ZERO { "Pierdes" } else { "Ganas" },
                eur(ganancia.abs())
            ))
            .strong();
            if ganancia < Decimal::ZERO {
                ui.label(texto.color(Color32::from_rgb(220, 80, 60)));
            } else {
                ui.label(texto);
            }
        });
    }

    /// Tabla de todas las partidas con su coste y su venta.
    pub(super) fn tabla_coste_venta(&mut self, ui: &mut Ui) {
        let d = self.p.decimales.importe;
        let mut ir: Option<(String, String)> = None;
        let (mut tc, mut tv) = (Decimal::ZERO, Decimal::ZERO);
        egui::ScrollArea::both()
            .max_height(420.0)
            .id_salt("tabla_cv")
            .show(ui, |ui| {
                egui::Grid::new("tabla_cv_grid")
                    .striped(true)
                    .num_columns(11)
                    .show(ui, |ui| {
                        for t in [
                            "Capítulo",
                            "Código",
                            "Resumen",
                            "Cantidad",
                            "Ud",
                            "Coste €/ud",
                            "Sobrecoste %",
                            "Venta €/ud",
                            "Margen s/venta %",
                            "Importe coste",
                            "Importe venta",
                        ] {
                            ui.label(RichText::new(t).strong());
                        }
                        ui.end_row();
                        for (cap, hijo) in self.orden_partidas() {
                            let Some(l) = self
                                .calc
                                .lineas
                                .get(&cap)
                                .and_then(|v| v.iter().find(|l| l.hijo == hijo))
                            else {
                                continue;
                            };
                            let Some(c) = self.p.conceptos.get(&hijo) else { continue };
                            let coste = self.calc.precios.get(&hijo).copied().unwrap_or_default();
                            let v = venta::venta_concepto(l.precio, c, self.gg, self.bi, d);
                            let ic = ppto_core::redondear(coste * l.cantidad, d);
                            let iv = ppto_core::redondear(v.venta * l.cantidad, d);
                            tc += ic;
                            tv += iv;
                            ui.label(&cap);
                            if ui
                                .link(&hijo)
                                .on_hover_text("Ver el desglose de esta partida")
                                .clicked()
                            {
                                ir = Some((cap.clone(), hijo.clone()));
                            }
                            ui.label(corto(&c.resumen, 48));
                            ui.label(cant(l.cantidad, self.p.decimales.medicion));
                            ui.label(&c.unidad);
                            ui.label(eur(coste));
                            let (sc, mv) = sobrecoste_y_margen(ic, iv);
                            let pct = |x: Option<Decimal>| x.map(|x| num(x, 2)).unwrap_or_else(|| "—".into());
                            if c.margen.is_some() {
                                ui.label(RichText::new(pct(sc)).strong())
                                    .on_hover_text("Sobrecoste propio de esta partida");
                            } else {
                                ui.label(RichText::new(pct(sc)).weak()).on_hover_text("El de la obra");
                            }
                            if v.bloqueada {
                                ui.label(RichText::new(format!("🔒 {}", eur(v.venta))).strong())
                                    .on_hover_text("Venta bloqueada");
                            } else {
                                ui.label(eur(v.venta));
                            }
                            if iv < ic {
                                ui.label(RichText::new(pct(mv)).color(Color32::from_rgb(220, 80, 60)));
                            } else {
                                ui.label(pct(mv));
                            }
                            ui.label(eur(ic));
                            ui.label(eur(iv));
                            ui.end_row();
                        }
                        let (sc, mv) = sobrecoste_y_margen(tc, tv);
                        let pct = |x: Option<Decimal>| x.map(|x| num(x, 2)).unwrap_or_else(|| "—".into());
                        ui.label("");
                        ui.label("");
                        ui.label(RichText::new("Total de la obra").strong());
                        for _ in 0..3 {
                            ui.label("");
                        }
                        ui.label(RichText::new(pct(sc)).strong());
                        ui.label("");
                        ui.label(RichText::new(pct(mv)).strong());
                        ui.label(RichText::new(eur(tc)).strong());
                        ui.label(RichText::new(eur(tv)).strong());
                        ui.end_row();
                    });
            });
        ui.label(
            RichText::new(
                "Sobrecoste = (venta − coste) / coste. Margen sobre venta = (venta − coste) / venta. Sobrecoste en negrita: propio de la partida; en gris: el de la obra.",
            )
            .weak(),
        );
        if let Some((cap, hijo)) = ir {
            self.sel = Some((cap, hijo));
            self.pestana = Pestana::Partida;
        }
    }

    pub(super) fn etiqueta_partida(&self, codigo: &str) -> String {
        match self.p.conceptos.get(codigo) {
            Some(c) => format!("{} — {}", c.codigo, corto(&c.resumen, 40)),
            None => codigo.to_owned(),
        }
    }
}
