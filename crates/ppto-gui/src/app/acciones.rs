// SPDX-License-Identifier: GPL-3.0-or-later
//! Aplicar los cambios pedidos desde la interfaz (con deshacer y rehacer).

use super::*;

impl Aplicacion {
    pub(super) fn aplicar(&mut self, acciones: Vec<Accion>) {
        for a in acciones {
            let guardado = match describir(&a) {
                Some((que, grupo)) => self.historial.antes(&self.p, que, grupo),
                None => {
                    if !matches!(
                        a,
                        Accion::Seleccionar(..)
                            | Accion::Navegar(_)
                            | Accion::SiguienteSinPrecio
                            | Accion::CapituloActivo(_)
                    ) {
                        self.historial.grupo = None;
                    }
                    false
                }
            };
            let r = match a {
                Accion::Seleccionar(padre, hijo) => {
                    self.capitulo_activo = Some(padre.clone());
                    self.sel = Some((padre, hijo));
                    self.pestana = Pestana::Partida;
                    self.avisos.clear();
                    continue;
                }
                Accion::Texto(codigo, texto) => {
                    if let Some(c) = self.p.conceptos.get_mut(&codigo) {
                        c.texto = (!texto.trim().is_empty()).then_some(texto);
                    }
                    self.cambios = true;
                    continue;
                }
                Accion::Ci(v) => {
                    if v < Decimal::ZERO {
                        self.error("Los costes indirectos no pueden ser negativos.");
                        continue;
                    }
                    self.p.costes_indirectos = v;
                    Ok(())
                }
                Accion::OpcionesCi(o) => {
                    self.p.opciones_ci = o;
                    Ok(())
                }
                Accion::Ir(codigo) => {
                    let padre = self
                        .p
                        .conceptos
                        .values()
                        .find(|c| c.descomposicion.iter().any(|l| l.hijo == codigo))
                        .map(|c| c.codigo.clone());
                    if let Some(padre) = padre {
                        self.sel = Some((padre, codigo));
                        self.pestana = Pestana::Partida;
                    }
                    continue;
                }
                Accion::QuitarOferta(i) => {
                    if i < self.ofertas.len() {
                        self.ofertas.remove(i);
                    }
                    continue;
                }
                Accion::Resumen(codigo, texto) => {
                    if let Some(c) = self.p.conceptos.get_mut(&codigo) {
                        c.resumen = texto;
                    }
                    self.cambios = true;
                    continue;
                }
                Accion::AplicarRatio(paq) => self.p.aplicar_ratio(&paq).map(|inf| {
                    let a = inf.ahorro();
                    self.info(format!(
                        "Subcontrata {} aplicada: {} {} × {} € = {} € en {} partidas; {} {} €.",
                        paq.codigo,
                        num(inf.medidor, 2),
                        paq.unidad,
                        eur(paq.precio),
                        eur(inf.importe),
                        inf.reparto.len(),
                        if a >= Decimal::ZERO { "ahorro" } else { "sobrecoste" },
                        eur(a.abs())
                    ));
                }),
                Accion::Dificultad {
                    ambito,
                    coeficiente,
                    motivo,
                } => self.p.aplicar_dificultad(&ambito, coeficiente).map(|inf| {
                    let nota = dificultad::describir(&ambito, coeficiente, &motivo);
                    self.info(format!(
                        "Aplicado {nota}: de {} h a {} h de mano de obra propia; coste directo de {} € a {} €.{}",
                        num(inf.total_antes(), 2),
                        num(inf.total_despues(), 2),
                        eur(inf.coste_antes),
                        eur(inf.coste_despues),
                        if inf.omitidas.is_empty() {
                            String::new()
                        } else {
                            format!(" Sin tocar (compartidas): {}.", inf.omitidas.join(", "))
                        }
                    ));
                    self.notas_revision.push(nota);
                }),
                Accion::AceptarPropuesta(id) => match self.aceptar_propuesta(id) {
                    Ok(t) => {
                        self.info(t);
                        Ok(())
                    }
                    Err(e) => Err(ppto_core::ErrorMotor::ValorInvalido(e)),
                },
                Accion::Margen(c, m) => self.p.fijar_margen(&c, m),
                Accion::VentaFija(c, v) => self.p.fijar_venta(&c, v),
                Accion::Descuento(fab, pct) => self.p.fijar_descuento(&fab, pct).map(|v| {
                    self.info(format!(
                        "Descuento {} {} %: {} recursos recalculados.",
                        ppto_core::descuentos::normalizar(&fab),
                        num(pct, 2),
                        v.len()
                    ));
                }),
                Accion::Pvp(codigo, pvp) => {
                    let fab = self.p.conceptos.get(&codigo).and_then(|c| c.tarifa.clone());
                    match fab {
                        Some(t) => self
                            .p
                            .fijar_tarifa_grupo(&codigo, &t.fabricante, t.grupo.as_deref(), pvp),
                        None => Err(ppto_core::ErrorMotor::ValorInvalido(format!(
                            "«{codigo}» no tiene tarifa"
                        ))),
                    }
                }
                Accion::GrupoTarifa(codigo, grupo) => self.p.fijar_grupo(&codigo, &grupo),
                Accion::QuitarDescuento(k) => {
                    let v = self.p.quitar_descuento(&k);
                    self.info(format!("Descuento {k} quitado: {} recursos recalculados.", v.len()));
                    Ok(())
                }
                Accion::ReaplicarAceptadas => {
                    let (n, fallos) = self.reaplicar_aceptadas();
                    self.info(format!(
                        "{n} propuestas aceptadas vueltas a aplicar.{}",
                        if fallos.is_empty() {
                            String::new()
                        } else {
                            format!(" No se pudieron aplicar: {}", fallos.join(", "))
                        }
                    ));
                    Ok(())
                }
                Accion::RechazarPropuesta(id) => {
                    self.enlace.marcar(id, ppto_core::propuesta::EstadoPropuesta::Rechazada);
                    self.info(format!("Propuesta #{id} rechazada."));
                    continue;
                }
                Accion::Navegar(d) => {
                    self.navegar(d, false);
                    continue;
                }
                Accion::CapituloActivo(c) => {
                    self.capitulo_activo = Some(c);
                    continue;
                }
                Accion::Portapapeles(x) => {
                    if let Some((o, h, cortar)) = &x {
                        // Al copiar, también al portapapeles del sistema: se puede
                        // pegar con Ctrl+V en otra ventana del programa (otra obra).
                        if !*cortar && let Ok(f) = ppto_core::fragmento::copiar(&self.p, o, std::slice::from_ref(h)) {
                            self.copiar_al_sistema = Some(f.a_texto());
                        }
                        self.info(format!(
                            "{h} {}. Clic derecho sobre el capítulo de destino → «Pegar aquí»{}.",
                            if *cortar { "cortada" } else { "copiada" },
                            if *cortar { "" } else { ", o Ctrl+V en otra ventana del programa" }
                        ));
                    }
                    self.portapapeles = x;
                    continue;
                }
                Accion::SiguienteSinPrecio => {
                    self.navegar(1, true);
                    continue;
                }
                Accion::Alta(t) => {
                    self.abrir_alta(t);
                    continue;
                }
                Accion::Insertar(t, antes) => {
                    self.abrir_alta(t);
                    if let Some(a) = self.alta.as_mut() {
                        a.antes = Some(antes);
                    }
                    continue;
                }
                Accion::Colocar(padre, hijo, antes) => {
                    let esta = self
                        .p
                        .conceptos
                        .get(&padre)
                        .is_some_and(|c| c.descomposicion.iter().any(|l| l.hijo == hijo));
                    if !esta {
                        continue;
                    }
                    self.p.trasladar_linea(&padre, &hijo, &padre, Some(&antes))
                }
                Accion::Unidad(c, u) => self.p.fijar_unidad(&c, &u),
                Accion::NuevoCapitulo { padre, codigo, resumen } => {
                    self.p.nuevo_capitulo(&padre, &codigo, &resumen).map(|()| {
                        let t = if padre == self.p.raiz {
                            "Capítulo"
                        } else {
                            "Subcapítulo"
                        };
                        self.info(format!("{t} {codigo} creado en {padre}."));
                    })
                }
                Accion::NuevaPartida {
                    capitulo,
                    codigo,
                    unidad,
                    resumen,
                } => self.p.nueva_partida(&capitulo, &codigo, &unidad, &resumen).map(|()| {
                    self.sel = Some((capitulo, codigo.clone()));
                    self.pestana = Pestana::Partida;
                    self.info(format!(
                        "Partida {codigo} creada: añada sus líneas con «Añadir línea…»."
                    ));
                }),
                Accion::NuevoRecurso {
                    codigo,
                    unidad,
                    resumen,
                    naturaleza,
                    precio,
                    en,
                    cantidad,
                } => self
                    .p
                    .nuevo_recurso(&codigo, &unidad, &resumen, naturaleza, precio)
                    .and_then(|()| {
                        self.p.anadir_linea(&en, &codigo, cantidad).inspect_err(|_| {
                            let _ = self.p.borrar_concepto(&codigo);
                        })
                    }),
                Accion::AnadirLinea(padre, hijo, q) => self.p.anadir_linea(&padre, &hijo, q),
                Accion::QuitarLinea(padre, hijo, purgar) => {
                    let r = if purgar {
                        self.p.quitar_y_purgar(&padre, &hijo).map(|b| {
                            if !b.is_empty() {
                                self.info(format!("Borrados por quedar sin uso: {}", b.join(", ")));
                            }
                        })
                    } else {
                        self.p.quitar_linea(&padre, &hijo)
                    };
                    if r.is_ok()
                        && self
                            .sel
                            .as_ref()
                            .is_some_and(|(sp, sh)| (sp == &padre && sh == &hijo) || !self.p.conceptos.contains_key(sh))
                    {
                        self.sel = None;
                    }
                    r
                }
                Accion::Mover(padre, hijo, arriba) => self.p.mover_linea(&padre, &hijo, arriba),
                Accion::Trasladar(origen, hijo, destino, antes) => self
                    .p
                    .trasladar_linea(&origen, &hijo, &destino, antes.as_deref())
                    .map(|()| {
                        if self.sel.as_ref().is_some_and(|(sp, sh)| sp == &origen && sh == &hijo) {
                            self.sel = Some((destino.clone(), hijo.clone()));
                        }
                        if origen != destino {
                            self.info(format!("{hijo} movida de {origen} a {destino}."));
                        }
                        if self.portapapeles.as_ref().is_some_and(|(o, h, c)| *c && o == &origen && h == &hijo) {
                            self.portapapeles = None;
                        }
                    }),
                Accion::Copiar(origen, hijo, destino) => self.p.copiar_linea(&origen, &hijo, &destino).map(|()| {
                    self.info(format!(
                        "{hijo} copiada en {destino}. Es la misma partida: un cambio de precio afecta a las dos; la cantidad es de cada capítulo."
                    ));
                }),
                Accion::CopiarDeBase {
                    origen,
                    nombre,
                    codigos,
                    partida,
                } => self.copiar_de_base(&origen, &nombre, &codigos, partida.as_deref()),
                Accion::PegarFragmento(f, destino) => ppto_core::fragmento::pegar(&mut self.p, &f, &destino).map(|inf| {
                    let mut m = format!("Pegado de «{}» en {destino}: {}", f.obra, inf.pegados.join(", "));
                    if !inf.renombrados.is_empty() {
                        let r: Vec<String> = inf.renombrados.iter().map(|(a, b)| format!("{a} → {b}")).collect();
                        m += &format!(". Ya había otra con el mismo código: {}", r.join(", "));
                    }
                    if !inf.recursos_de_esta_obra.is_empty() {
                        m += &format!(
                            ". Recursos con otro precio en esta obra (se usa el de esta obra): {}",
                            inf.recursos_de_esta_obra.join(", ")
                        );
                    }
                    if let Some(h) = inf.pegados.last() {
                        self.sel = Some((destino.clone(), h.clone()));
                        self.desplazar_arbol = true;
                    }
                    self.info(format!("{m}."));
                }),
                Accion::Obra(op) => op.aplicar(&mut self.p).map(|m| {
                    if let Some(m) = m {
                        self.info(m);
                    }
                }),
                Accion::Precio(c, v) => self.p.fijar_precio(&c, v),
                Accion::Origen(c, o, aplicar) => self.p.fijar_origen(&c, o.map(|b| *b), aplicar),
                Accion::Rendimiento(padre, hijo, v) => self.p.fijar_rendimiento(&padre, &hijo, v),
                Accion::Medicion(padre, hijo, m) => self.p.actualizar_medicion(&padre, &hijo, m).map(|av| {
                    self.avisos = av;
                }),
            };
            match r {
                Ok(()) => self.marcar_cambio(),
                Err(e) => {
                    // El motor no cambia nada si falla: fuera la copia
                    if guardado {
                        self.historial.deshacer.pop();
                        self.historial.grupo = None;
                    }
                    self.error(e.to_string());
                }
            }
        }
    }

    /// Copia conceptos de una base a la obra y, con `partida`, añade los
    /// recursos como líneas suyas (cantidad 1). Todo o nada.
    pub(super) fn copiar_de_base(
        &mut self,
        origen: &Presupuesto,
        nombre: &str,
        codigos: &[String],
        partida: Option<&str>,
    ) -> Result<(), ppto_core::ErrorMotor> {
        let copia = self.p.clone();
        let r = (|| {
            let (nuevos, existentes) = self.p.copiar_conceptos(origen, codigos)?;
            let mut lineas = 0;
            if let Some(pt) = partida {
                for c in codigos {
                    let es_recurso = self
                        .p
                        .conceptos
                        .get(c)
                        .is_some_and(|x| x.naturaleza != Naturaleza::Partida);
                    let ya = self.p.concepto(pt)?.descomposicion.iter().any(|l| &l.hijo == c);
                    if es_recurso && !ya {
                        self.p.anadir_linea(pt, c, Decimal::ONE)?;
                        lineas += 1;
                    }
                }
            }
            Ok::<_, ppto_core::ErrorMotor>((nuevos, existentes, lineas))
        })();
        match r {
            Ok((nuevos, existentes, lineas)) => {
                let mut m = format!("{} conceptos copiados de {nombre}", nuevos.len());
                if !existentes.is_empty() {
                    m += &format!("; {} ya estaban en la obra y no se han tocado", existentes.len());
                }
                if let Some(pt) = partida {
                    m += &format!("; {lineas} líneas añadidas a {pt}");
                } else {
                    m += ". Están disponibles en «+ Añadir línea…» de cualquier partida";
                }
                self.info(format!("{m}."));
                Ok(())
            }
            Err(e) => {
                self.p = copia;
                Err(e)
            }
        }
    }

    pub(super) fn deshacer(&mut self) {
        if let Some((p, que)) = self.historial.deshacer.pop() {
            let actual = std::mem::replace(&mut self.p, p);
            if let Some(i) = self.notas_revision.iter().rposition(|n| *n == que) {
                self.notas_revision.remove(i);
            }
            if let Some(id) = propuestas::id_en_etiqueta(&que) {
                self.enlace.marcar(id, ppto_core::propuesta::EstadoPropuesta::Pendiente);
            }
            self.historial.rehacer.push((actual, que.clone()));
            self.tras_historial(format!("Deshecho: {que}."));
        }
    }

    pub(super) fn rehacer(&mut self) {
        if let Some((p, que)) = self.historial.rehacer.pop() {
            let actual = std::mem::replace(&mut self.p, p);
            if que.starts_with("dificultad ") {
                self.notas_revision.push(que.clone());
            }
            if let Some(id) = propuestas::id_en_etiqueta(&que) {
                self.enlace.marcar(id, ppto_core::propuesta::EstadoPropuesta::Aceptada);
            }
            self.historial.deshacer.push((actual, que.clone()));
            self.tras_historial(format!("Rehecho: {que}."));
        }
    }

    pub(super) fn tras_historial(&mut self, mensaje: String) {
        self.historial.grupo = None;
        self.edicion = Edicion::default();
        if let Some((padre, hijo)) = &self.sel
            && !self
                .p
                .conceptos
                .get(padre)
                .is_some_and(|c| c.descomposicion.iter().any(|l| &l.hijo == hijo))
        {
            self.sel = None;
        }
        self.marcar_cambio();
        self.info(mensaje);
    }
}
