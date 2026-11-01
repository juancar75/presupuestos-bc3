// SPDX-License-Identifier: GPL-3.0-or-later
//! Archivo: abrir, guardar, importar y exportar obras; revisiones y datos de la obra.

use super::*;

impl Aplicacion {
    pub(super) fn cargar_ejemplo(&mut self) {
        self.ciego = None;
        self.historial.limpiar();
        self.p = suelo_radiante();
        self.fichero = None;
        self.presupuesto_id = None;
        self.revisiones.clear();
        self.revision_actual = None;
        self.cambios = false;
        self.sel = Some(("C01".into(), "SR.M2".into()));
        self.ofertas = ofertas_montaje_suelo_radiante().to_vec();
        self.avisos.clear();
        self.recalcular();
        self.info("Ejemplo sintético cargado.");
    }

    /// Otra ventana del programa (proceso aparte), con una obra o vacía.
    pub(super) fn abrir_otra_ventana(&mut self, ruta: Option<PathBuf>) {
        let r = std::env::current_exe().and_then(|exe| {
            let mut c = std::process::Command::new(exe);
            if let Some(r) = &ruta {
                c.arg(r);
            }
            c.spawn()
        });
        match r {
            Ok(_) => self.info("Otra ventana abierta: copie con Ctrl+C en una y pegue con Ctrl+V en la otra."),
            Err(e) => self.error(format!("No se pudo abrir otra ventana: {e}")),
        }
    }

    /// Presupuesto nuevo con la estructura de una plantilla (sin precios).
    pub(super) fn nuevo_desde_plantilla(&mut self, t: ppto_core::plantillas::Plantilla) {
        match ppto_core::plantillas::crear(t, &format!("Nueva obra — {}", t.nombre())) {
            Ok(p) => {
                self.nuevo_presupuesto();
                self.p = p;
                self.recalcular();
                self.info(format!(
                    "Plantilla «{}» cargada: capítulos, observaciones, exclusiones y legalización. Las partidas están sin precio.",
                    t.nombre()
                ));
            }
            Err(e) => self.error(format!("No se pudo crear la plantilla: {e}")),
        }
    }

    pub(super) fn nuevo_presupuesto(&mut self) {
        self.ciego = None;
        self.historial.limpiar();
        self.p = Presupuesto::new("Presupuesto nuevo", "OBRA", "Presupuesto nuevo");
        self.fichero = None;
        self.presupuesto_id = None;
        self.revisiones.clear();
        self.revision_actual = None;
        self.cambios = false;
        self.sel = None;
        self.ofertas.clear();
        self.avisos.clear();
        self.informe = None;
        self.recalcular();
        self.info("Presupuesto vacío: clic derecho en el árbol → Nuevo capítulo (o Archivo → Nuevo desde plantilla).");
    }

    pub(super) fn dialogo_abrir(&mut self) {
        if let Some(ruta) = rfd::FileDialog::new()
            .add_filter("Presupuesto (SQLite)", &["sqlite", "db"])
            .pick_file()
        {
            self.abrir(ruta);
        }
    }

    /// Abre una obra; si otra persona (u otra ventana) la tiene abierta,
    /// primero pregunta.
    pub(super) fn abrir(&mut self, ruta: PathBuf) {
        if let Some(o) = ppto_db::bloqueo::ocupada(&ruta) {
            self.confirmar_abrir = Some((ruta, o));
            return;
        }
        self.abrir_sin_preguntar(ruta);
    }

    pub(super) fn abrir_sin_preguntar(&mut self, ruta: PathBuf) {
        self.ciego = None;
        self.historial.limpiar();
        let r = (|| -> ppto_db::Resultado<_> {
            let a = Almacen::abrir(&ruta)?;
            let revs = a.listar_revisiones()?;
            let ultima = revs.last().cloned();
            let p = match &ultima {
                Some(r) => Some((a.cargar_revision(r.id)?, a.datos_obra(r.id)?)),
                None => None,
            };
            Ok((revs, ultima, p))
        })();
        match r {
            Ok((revs, Some(ultima), Some((p, datos)))) => {
                self.p = p;
                self.aplicar_datos_obra(&datos);
                self.presupuesto_id = Some(ultima.presupuesto_id);
                self.revision_actual = Some(ultima.id);
                self.revisiones = revs;
                self.fichero = Some(ruta);
                self.cambios = false;
                self.sel = None;
                self.ofertas.clear();
                self.recalcular();
                self.info(format!(
                    "Abierta la revisión {} ({}).",
                    ultima.etiqueta, ultima.creado_en
                ));
            }
            Ok(_) => self.error("El fichero no contiene ningún presupuesto."),
            Err(e) => self.error(format!("No se pudo abrir: {e}")),
        }
    }

    pub(super) fn exportar_bc3(&mut self, longitud_codigo: Option<usize>) {
        let nombre = format!(
            "{}.bc3",
            self.p
                .nombre
                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3"])
            .set_file_name(nombre)
            .save_file()
        else {
            return;
        };
        let o = ppto_bc3::OpcionesExportacion {
            gastos_generales: Some(self.gg),
            beneficio_industrial: Some(self.bi),
            iva: Some(self.iva),
            fecha: None,
            descuentos_visibles: true,
            longitud_codigo,
        };
        match ppto_bc3::exportar_fichero(&self.p, &o, &ruta) {
            Ok(avisos) if avisos.is_empty() => self.info(format!("BC3 exportado en {}", ruta.display())),
            Ok(avisos) => self.info(format!("BC3 exportado en {} — {}", ruta.display(), avisos.join("; "))),
            Err(e) => self.error(format!("No se pudo exportar: {e}")),
        }
    }

    pub(super) fn exportar_excel(&mut self) {
        let nombre = format!(
            "{}.xlsx",
            self.p
                .nombre
                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx"])
            .set_file_name(nombre)
            .save_file()
        else {
            return;
        };
        let o = ppto_informes::OpcionesInforme {
            gastos_generales: self.gg,
            beneficio_industrial: self.bi,
            iva: self.iva,
        };
        match ppto_informes::guardar_excel(&self.p, &o, &ruta) {
            Ok(()) => self.info(format!("Informe Excel guardado en {}", ruta.display())),
            Err(e) => self.error(format!("No se pudo generar el Excel: {e}")),
        }
    }

    pub(super) fn dialogo_importar(&mut self) {
        if let Some(ruta) = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3", "BC3"])
            .pick_file()
        {
            self.importar_bc3(ruta);
        }
    }

    pub(super) fn importar_bc3(&mut self, ruta: PathBuf) {
        self.ciego = None;
        self.historial.limpiar();
        match ppto_bc3::importar_fichero(&ruta) {
            Ok(imp) => {
                self.p = imp.presupuesto.clone();
                let k = &imp.porcentajes;
                self.gg = k.gastos_generales.unwrap_or(self.gg);
                self.bi = k.beneficio_industrial.unwrap_or(self.bi);
                self.iva = k.iva.unwrap_or(self.iva);
                self.fichero = None;
                self.presupuesto_id = None;
                self.revisiones.clear();
                self.revision_actual = None;
                self.cambios = true;
                self.sel = None;
                self.ofertas.clear();
                self.avisos.clear();
                self.recalcular();
                self.sel = self.primera_partida(&self.p.raiz.clone());
                let nombre = ruta
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.info(format!(
                    "Importado {nombre}: {} conceptos, {} errores, {} avisos, {} precios que no cuadran. Guárdalo para conservarlo.",
                    self.p.conceptos.len(),
                    imp.errores(),
                    imp.avisos(),
                    imp.discrepancias.len()
                ));
                self.ver_informe = imp.errores() + imp.avisos() + imp.discrepancias.len() > 0;
                self.informe = Some(imp);
            }
            Err(e) => self.error(format!("No se pudo importar: {e}")),
        }
    }

    pub(super) fn cambiar_revision(&mut self, id: i64) {
        let Some(ruta) = self.fichero.clone() else { return };
        match Almacen::abrir(&ruta).and_then(|a| Ok((a.cargar_revision(id)?, a.datos_obra(id)?))) {
            Ok((p, datos)) => {
                self.p = p;
                self.aplicar_datos_obra(&datos);
                self.historial.limpiar();
                self.revision_actual = Some(id);
                self.cambios = false;
                self.recalcular();
                let et = self.etiqueta_actual();
                self.info(format!("Revisión {et} cargada."));
            }
            Err(e) => self.error(format!("No se pudo cargar la revisión: {e}")),
        }
    }

    pub(super) fn guardar(&mut self, pedir_ruta: bool) {
        let ruta = match (&self.fichero, pedir_ruta) {
            (Some(r), false) => r.clone(),
            _ => match rfd::FileDialog::new()
                .add_filter("Presupuesto (SQLite)", &["sqlite"])
                .set_file_name("presupuesto.sqlite")
                .save_file()
            {
                Some(r) => {
                    self.presupuesto_id = None;
                    r
                }
                None => return,
            },
        };
        let autor = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "usuario".into());
        let r = (|| -> ppto_db::Resultado<_> {
            let mut a = Almacen::abrir(&ruta)?;
            let pid = match self.presupuesto_id {
                Some(id) => id,
                None => a.crear_presupuesto(&self.p.nombre)?,
            };
            let etiqueta = a.siguiente_etiqueta(pid)?;
            let rev = a.guardar_revision(pid, &etiqueta, &autor, &self.p)?;
            a.guardar_datos_obra(rev, &self.datos_obra())?;
            for n in &self.notas_revision {
                a.anotar(rev, &autor, "dificultad", n)?;
            }
            Ok((pid, rev, etiqueta, a.listar_revisiones()?))
        })();
        match r {
            Ok((pid, rev, etiqueta, revs)) => {
                self.notas_revision.clear();
                self.fichero = Some(ruta);
                self.presupuesto_id = Some(pid);
                self.revision_actual = Some(rev);
                self.revisiones = revs;
                self.cambios = false;
                self.info(format!(
                    "Guardado como revisión {etiqueta}. Las revisiones anteriores no cambian."
                ));
            }
            Err(e) => self.error(format!("No se pudo guardar: {e}")),
        }
    }

    /// Datos de la obra que se guardan con cada revisión además del
    /// presupuesto: GG, BI e IVA del resumen y el Excel del ciego.
    pub(super) fn datos_obra(&self) -> BTreeMap<String, String> {
        let mut d = BTreeMap::from([
            ("gg".to_owned(), self.gg.to_string()),
            ("bi".to_owned(), self.bi.to_string()),
            ("iva".to_owned(), self.iva.to_string()),
            ("retencion".to_owned(), self.retencion.to_string()),
        ]);
        if let Some(c) = &self.ciego {
            d.insert("ciego".into(), c.ruta.display().to_string());
        }
        d
    }

    /// Recupera los datos de la obra al abrir (los ficheros antiguos no
    /// los tienen: se dejan los valores actuales).
    pub(super) fn aplicar_datos_obra(&mut self, d: &BTreeMap<String, String>) {
        let dec = |k: &str| d.get(k).and_then(|v| v.parse::<Decimal>().ok());
        if let Some(v) = dec("gg") {
            self.gg = v;
        }
        if let Some(v) = dec("bi") {
            self.bi = v;
        }
        if let Some(v) = dec("iva") {
            self.iva = v;
        }
        self.retencion = dec("retencion").unwrap_or_default();
        self.ciego = None;
        if let Some(r) = d.get("ciego").map(PathBuf::from) {
            match ppto_ciego::leer(&r) {
                Ok(c) => self.ciego = Some(ciego::CiegoAbierto { ruta: r, ciego: c }),
                // Movido o borrado: «Devolver valorado» lo pedirá
                Err(_) => self.info(format!(
                    "No se encuentra el Excel del ciego ({}): al devolverlo valorado se le pedirá.",
                    r.display()
                )),
            }
        }
    }

    pub(super) fn etiqueta_actual(&self) -> String {
        self.revisiones
            .iter()
            .find(|r| Some(r.id) == self.revision_actual)
            .map(|r| r.etiqueta.clone())
            .unwrap_or_else(|| "sin guardar".into())
    }
}

impl Aplicacion {
    /// Mantiene el aviso de «obra abierta» de la obra en pantalla (llamar en
    /// cada fotograma).
    pub(super) fn aviso_obra_abierta(&mut self, ctx: &egui::Context) {
        let t = ctx.input(|i| i.time);
        match (&self.fichero, &mut self.aviso_abierta) {
            (Some(f), Some(a)) if a.0.obra == *f => {
                if t - a.1 >= 60.0 {
                    a.0.latir();
                    a.1 = t;
                }
            }
            (Some(f), _) => self.aviso_abierta = Some((ppto_db::bloqueo::Aviso::poner(f), t)),
            (None, _) => self.aviso_abierta = None,
        }
        if self.aviso_abierta.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
        }
    }

    pub(super) fn ventana_confirmar_abrir(&mut self, ctx: &egui::Context) {
        let Some((ruta, o)) = self.confirmar_abrir.clone() else {
            return;
        };
        let mut decision: Option<bool> = None;
        let nombre = ruta
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let desde = ppto_db::bloqueo::ahora().saturating_sub(o.desde) / 60;
        egui::Window::new("Obra abierta en otro sitio")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("«{nombre}» la tiene abierta {}", o.quien())).strong());
                ui.label(format!("desde hace {desde} min."));
                ui.add_space(4.0);
                ui.label(
                    "Si los dos guardáis, cada uno crea su revisión y los cambios no se juntan. \
                     Mejor que la cierre antes, o ábrala solo para consultar.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Abrir igualmente").clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancelar").clicked() {
                        decision = Some(false);
                    }
                });
            });
        match decision {
            Some(true) => {
                self.confirmar_abrir = None;
                self.abrir_sin_preguntar(ruta);
            }
            Some(false) => self.confirmar_abrir = None,
            None => {}
        }
    }
}
