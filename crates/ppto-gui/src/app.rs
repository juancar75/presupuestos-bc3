// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana principal: árbol de capítulos, partida (descompuesto y mediciones),
//! recursos, subcontratación y resumen.

mod alta;
mod asistente;
mod bases;
mod biblioteca;
mod ciego;
mod dificultad;
mod legalizacion;
mod pdf;
mod propuestas;
mod ratio;
mod tarifas;
mod tema;
mod vistas;

use crate::celdas::Edicion;
use alta::{Alta, TipoAlta};
use eframe::egui::{self, Color32, RichText, Ui};
use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::{ofertas_montaje_suelo_radiante, suelo_radiante};
use ppto_core::explosion::Explosion;
use ppto_core::formato::{cant, eur, num};
use ppto_core::medicion::{LineaMedicion, Medicion, TipoLinea};
use ppto_core::presupuesto::LineaValorada;
use ppto_core::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo, ResultadoEscenario, simular};
use ppto_core::{Decimal, Presupuesto, venta};
use ppto_db::{Almacen, InfoRevision};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

const ANCHO_NUM: f32 = 78.0;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Pestana {
    Partida,
    Presupuesto,
    Precios,
    Mediciones,
    Recursos,
    Subcontratar,
    Resumen,
}

/// Cambios pedidos desde la interfaz; se aplican al final de cada fotograma.
enum Accion {
    Seleccionar(String, String),
    Precio(String, Decimal),
    Rendimiento(String, String, Decimal),
    Medicion(String, String, Medicion),
    Resumen(String, String),
    QuitarOferta(usize),
    Ir(String),
    Ci(Decimal),
    Texto(String, String),
    OpcionesCi(ppto_core::presupuesto::OpcionesCi),
    Unidad(String, String),
    NuevoCapitulo {
        padre: String,
        codigo: String,
        resumen: String,
    },
    NuevaPartida {
        capitulo: String,
        codigo: String,
        unidad: String,
        resumen: String,
    },
    /// Crea un recurso y lo añade como línea de `en`; si algo falla, no queda nada.
    NuevoRecurso {
        codigo: String,
        unidad: String,
        resumen: String,
        naturaleza: Naturaleza,
        precio: Decimal,
        en: String,
        cantidad: Decimal,
    },
    AnadirLinea(String, String, Decimal),
    /// Quitar línea (padre, hijo); `true` = borrar también lo que quede sin usar.
    QuitarLinea(String, String, bool),
    Mover(String, String, bool),
    /// Arrastrar y soltar: (origen, hijo, destino, delante de).
    Trasladar(String, String, String, Option<String>),
    /// Copiar y pegar: (origen, hijo, destino). La misma partida en otro sitio.
    Copiar(String, String, String),
    /// Copiar conceptos de una base de precios a la obra (y, si se indica,
    /// añadir los recursos como líneas de esa partida con cantidad 1).
    CopiarDeBase {
        origen: std::sync::Arc<Presupuesto>,
        nombre: String,
        codigos: Vec<String>,
        partida: Option<String>,
    },
    /// Cortar o copiar (origen, hijo, `true` = cortar); `None` = vaciar.
    Portapapeles(Option<(String, String, bool)>),
    Alta(TipoAlta),
    AplicarRatio(ppto_core::ratio::PaqueteRatio),
    AceptarPropuesta(u64),
    /// Volver a aplicar las propuestas ya aceptadas cuyas partidas están
    /// sin precio (p. ej. tras abrir de nuevo el Excel ciego).
    ReaplicarAceptadas,
    /// Descuento de un fabricante (puntos).
    Descuento(String, Decimal),
    /// Margen propio de una partida (% sobre su precio; `None` = el general).
    Margen(String, Option<Decimal>),
    /// Precio de venta bloqueado de una partida (`None` = desbloquear).
    VentaFija(String, Option<Decimal>),
    /// PVP de tarifa de un recurso ligado a un fabricante.
    Pvp(String, Decimal),
    /// Grupo de producto de un recurso con tarifa (vacío = sin grupo).
    GrupoTarifa(String, String),
    /// Quitar un descuento («S&P» o «S&P/E08»).
    QuitarDescuento(String),
    RechazarPropuesta(u64),
    Dificultad {
        ambito: String,
        coeficiente: Decimal,
        motivo: String,
    },
    /// Ir a la partida anterior (-1) o siguiente (+1) del presupuesto.
    Navegar(i32),
    SiguienteSinPrecio,
}

/// Resultados del motor para el estado actual del presupuesto.
#[derive(Default)]
struct Calculo {
    error: Option<String>,
    pem: Decimal,
    coste_directo: Decimal,
    lineas: HashMap<String, Vec<LineaValorada>>,
    precios: HashMap<String, Decimal>,
    cantidades: BTreeMap<String, Decimal>,
    explosion: Option<Explosion>,
}

impl Calculo {
    fn de(p: &Presupuesto) -> Self {
        let mut c = Calculo::default();
        if let Err(e) = p.validar() {
            c.error = Some(e.to_string());
            return c;
        }
        let r: Result<(), ppto_core::ErrorMotor> = (|| {
            let v = p.valorar()?;
            c.precios = v.precios;
            c.lineas = v.lineas;
            c.pem = v.pem;
            c.coste_directo = v.coste_directo;
            c.cantidades = p.cantidades_partidas()?;
            c.explosion = Some(p.explosion_recursos()?);
            Ok(())
        })();
        if let Err(e) = r {
            c.error = Some(e.to_string());
        }
        c
    }
}

/// Formulario de nueva oferta de subcontrata.
struct FormOferta {
    codigo: String,
    descripcion: String,
    partida: String,
    recursos: BTreeMap<String, bool>,
    alzado: bool,
    unidad: String,
    precio: Decimal,
    factor: Decimal,
    importe: Decimal,
}

impl Default for FormOferta {
    fn default() -> Self {
        Self {
            codigo: "SUB-1".into(),
            descripcion: String::new(),
            partida: String::new(),
            recursos: BTreeMap::new(),
            alzado: false,
            unidad: "m2".into(),
            precio: Decimal::ZERO,
            factor: Decimal::ONE,
            importe: Decimal::ZERO,
        }
    }
}

pub struct Aplicacion {
    p: Presupuesto,
    calc: Calculo,
    fichero: Option<PathBuf>,
    presupuesto_id: Option<i64>,
    revisiones: Vec<InfoRevision>,
    revision_actual: Option<i64>,
    cambios: bool,
    titulo: String,
    sel: Option<(String, String)>,
    pestana: Pestana,
    /// Capítulos plegados en la vista «Presupuesto».
    plegados: std::collections::HashSet<String>,
    filtro_precios: vistas::FiltroPrecios,
    filtro_mediciones: String,
    edicion: Edicion,
    avisos: Vec<String>,
    mensaje: Option<(String, bool)>,
    gg: Decimal,
    bi: Decimal,
    iva: Decimal,
    ofertas: Vec<PaqueteTrabajo>,
    form: FormOferta,
    informe: Option<ppto_bc3::Importacion>,
    ver_informe: bool,
    ver_acerca: bool,
    alta: Option<Alta>,
    ventana_pdf: Option<pdf::VentanaPdf>,
    ciego: Option<ciego::CiegoAbierto>,
    buscar: String,
    /// Poner el cursor en el buscador del árbol (Ctrl+F) en el próximo fotograma.
    enfocar_buscar: bool,
    /// Llevar el árbol hasta la partida elegida en el próximo fotograma.
    desplazar_arbol: bool,
    /// Partida cortada o copiada: (capítulo, partida, `true` = cortar).
    portapapeles: Option<(String, String, bool)>,
    historial: Historial,
    ratio: ratio::FormRatio,
    /// Tarifas de subcontratas del usuario (fuera del repositorio).
    tarifas: ppto_tarifas::Biblioteca,
    ventana_tarifas: Option<tarifas::VentanaTarifas>,
    ventana_biblioteca: Option<biblioteca::VentanaBiblioteca>,
    ventana_bases: Option<bases::VentanaBases>,
    ventana_legalizacion: Option<legalizacion::VentanaLegalizacion>,
    dificultad: dificultad::FormDificultad,
    /// Cambios que se anotan en la auditoría al guardar la revisión.
    notas_revision: Vec<String>,
    /// Obra publicada para la IA y bandeja de propuestas.
    enlace: propuestas::Enlace,
    /// Panel del asistente (conversación con la IA por obra).
    asistente: asistente::PanelAsistente,
}

/// Deshacer / rehacer: copias completas del presupuesto antes de cada cambio.
#[derive(Default)]
struct Historial {
    deshacer: Vec<(Presupuesto, String)>,
    rehacer: Vec<(Presupuesto, String)>,
    /// Escritura en curso (resumen, texto, unidad de un concepto): las
    /// pulsaciones seguidas en el mismo campo se deshacen de una vez.
    grupo: Option<String>,
}

const MAX_HISTORIAL: usize = 200;

impl Historial {
    fn limpiar(&mut self) {
        *self = Self::default();
    }

    /// Guarda el estado anterior a un cambio. Devuelve `true` si se ha guardado.
    fn antes(&mut self, p: &Presupuesto, que: String, grupo: Option<String>) -> bool {
        if grupo.is_some() && grupo == self.grupo {
            return false;
        }
        self.grupo = grupo;
        self.deshacer.push((p.clone(), que));
        if self.deshacer.len() > MAX_HISTORIAL {
            self.deshacer.remove(0);
        }
        self.rehacer.clear();
        true
    }
}

/// Descripción para «Deshacer …» y, en la escritura de textos, el grupo.
fn describir(a: &Accion) -> Option<(String, Option<String>)> {
    Some(match a {
        Accion::Precio(c, _) => (format!("precio de {c}"), None),
        Accion::Rendimiento(p, h, _) => (format!("cantidad de {h} en {p}"), None),
        Accion::Medicion(_, h, _) => (format!("medición de {h}"), None),
        Accion::Resumen(c, _) => (format!("resumen de {c}"), Some(format!("resumen|{c}"))),
        Accion::Texto(c, _) => (format!("texto de {c}"), Some(format!("texto|{c}"))),
        Accion::Unidad(c, _) => (format!("unidad de {c}"), Some(format!("unidad|{c}"))),
        Accion::Ci(_) => ("costes indirectos".into(), None),
        Accion::OpcionesCi(_) => ("opciones de costes indirectos".into(), None),
        Accion::NuevoCapitulo { codigo, .. } => (format!("nuevo capítulo {codigo}"), None),
        Accion::NuevaPartida { codigo, .. } => (format!("nueva partida {codigo}"), None),
        Accion::NuevoRecurso { codigo, .. } => (format!("nuevo {codigo}"), None),
        Accion::AnadirLinea(p, h, _) => (format!("añadir {h} a {p}"), None),
        Accion::QuitarLinea(p, h, _) => (format!("quitar {h} de {p}"), None),
        Accion::Mover(p, h, _) => (format!("mover {h} en {p}"), None),
        Accion::Trasladar(o, h, d, _) => (format!("mover {h} de {o} a {d}"), None),
        Accion::Copiar(_, h, d) => (format!("copiar {h} a {d}"), None),
        Accion::CopiarDeBase { nombre, codigos, .. } => {
            (format!("copiar {} conceptos de {nombre}", codigos.len()), None)
        }
        Accion::AplicarRatio(p) => (format!("subcontrata {}", p.codigo), None),
        Accion::AceptarPropuesta(id) => (format!("propuesta IA #{id}"), None),
        Accion::ReaplicarAceptadas => ("reaplicar propuestas aceptadas".into(), None),
        Accion::Descuento(f, _) => (format!("descuento {f}"), None),
        Accion::Margen(c, _) => (format!("margen de {c}"), None),
        Accion::VentaFija(c, _) => (format!("venta bloqueada de {c}"), None),
        Accion::Pvp(c, _) => (format!("PVP de {c}"), None),
        Accion::GrupoTarifa(c, _) => (format!("grupo de {c}"), None),
        Accion::QuitarDescuento(k) => (format!("quitar descuento {k}"), None),
        Accion::Dificultad {
            ambito,
            coeficiente,
            motivo,
        } => (dificultad::describir(ambito, *coeficiente, motivo), None),
        _ => return None,
    })
}

impl Aplicacion {
    pub fn new(cc: &eframe::CreationContext<'_>, inicial: Option<PathBuf>) -> Self {
        let mut estilo = (*cc.egui_ctx.global_style()).clone();
        estilo.spacing.item_spacing = egui::vec2(8.0, 5.0);
        cc.egui_ctx.set_global_style(estilo);
        tema::aplicar_guardado(&cc.egui_ctx);
        let mut app = Self::sin_ventana();
        app.enlace = propuestas::Enlace::en_disco();
        app.asistente = asistente::PanelAsistente::en_disco();
        if let Some(r) = tarifas::ruta_tarifas() {
            app.tarifas = ppto_tarifas::Biblioteca::cargar(&r);
        }
        // Pestaña inicial (útil para capturas y pruebas): PPTO_GUI_PESTANA=recursos|subcontratar|resumen
        app.pestana = match std::env::var("PPTO_GUI_PESTANA").as_deref() {
            Ok("recursos") => Pestana::Recursos,
            Ok("subcontratar") => Pestana::Subcontratar,
            Ok("resumen") => Pestana::Resumen,
            Ok("presupuesto") => Pestana::Presupuesto,
            Ok("precios") => Pestana::Precios,
            Ok("mediciones") => Pestana::Mediciones,
            _ => Pestana::Partida,
        };
        if let Some(ruta) = inicial {
            let es_bc3 = ruta.extension().is_some_and(|e| e.eq_ignore_ascii_case("bc3"));
            if es_bc3 {
                app.importar_bc3(ruta)
            } else {
                app.abrir(ruta)
            }
        }
        app
    }

    /// Estado inicial (ejemplo cargado) sin depender de la ventana: lo usan las pruebas.
    fn sin_ventana() -> Self {
        let mut app = Self {
            p: suelo_radiante(),
            calc: Calculo::default(),
            plegados: Default::default(),
            filtro_precios: Default::default(),
            filtro_mediciones: String::new(),
            fichero: None,
            presupuesto_id: None,
            revisiones: Vec::new(),
            revision_actual: None,
            cambios: false,
            titulo: String::new(),
            sel: Some(("C01".into(), "SR.M2".into())),
            pestana: Pestana::Partida,
            edicion: Edicion::default(),
            avisos: Vec::new(),
            mensaje: Some(("Ejemplo sintético de suelo radiante cargado. Las celdas con recuadro se pueden editar (Intro para confirmar).".into(), false)),
            gg: venta::GG_HABITUAL,
            bi: venta::BI_HABITUAL,
            iva: venta::IVA_GENERAL,
            ofertas: ofertas_montaje_suelo_radiante().to_vec(),
            form: FormOferta::default(),
            informe: None,
            ver_informe: false,
            ver_acerca: false,
            alta: None,
            ventana_pdf: None,
            ciego: None,
            buscar: String::new(),
            enfocar_buscar: false,
            desplazar_arbol: false,
            portapapeles: None,
            historial: Historial::default(),
            ratio: ratio::FormRatio::default(),
            tarifas: ppto_tarifas::Biblioteca::default(),
            ventana_tarifas: None,
            ventana_biblioteca: None,
            ventana_bases: None,
            ventana_legalizacion: None,
            dificultad: dificultad::FormDificultad::default(),
            notas_revision: Vec::new(),
            enlace: propuestas::Enlace::sin_disco(),
            asistente: asistente::PanelAsistente::sin_disco(),
        };
        app.recalcular();
        app
    }

    fn recalcular(&mut self) {
        self.calc = Calculo::de(&self.p);
        self.enlace.sucia = true;
    }

    fn marcar_cambio(&mut self) {
        self.cambios = true;
        self.recalcular();
    }

    fn cargar_ejemplo(&mut self) {
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

    fn nuevo_presupuesto(&mut self) {
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
        self.info("Presupuesto vacío: cree un capítulo con «+ Capítulo» en el árbol.");
    }

    /// Partidas en el orden del presupuesto (capítulo, partida), recorriendo
    /// capítulos y subcapítulos.
    fn orden_partidas(&self) -> Vec<(String, String)> {
        fn rec(app: &Aplicacion, cap: &str, out: &mut Vec<(String, String)>) {
            for l in app.calc.lineas.get(cap).into_iter().flatten() {
                if app
                    .p
                    .conceptos
                    .get(&l.hijo)
                    .is_some_and(|c| c.naturaleza == Naturaleza::Capitulo)
                {
                    rec(app, &l.hijo, out);
                } else {
                    out.push((cap.to_owned(), l.hijo.clone()));
                }
            }
        }
        let mut out = Vec::new();
        rec(self, &self.p.raiz, &mut out);
        out
    }

    /// Mueve la selección `d` partidas; con `sin_precio`, a la siguiente sin precio.
    fn navegar(&mut self, d: i32, sin_precio: bool) {
        let orden = self.orden_partidas();
        if orden.is_empty() {
            return;
        }
        let actual = self.sel.as_ref().and_then(|s| orden.iter().position(|x| x == s));
        let n = orden.len();
        let destino = if sin_precio {
            let inicio = actual.map_or(0, |i| i + 1);
            (0..n)
                .map(|k| (inicio + k) % n)
                .find(|&i| self.calc.precios.get(&orden[i].1).is_none_or(|p| p.is_zero()))
        } else {
            Some(match actual {
                None => 0,
                Some(i) => {
                    (i64::try_from(i).unwrap_or(0) + i64::from(d)).clamp(0, i64::try_from(n - 1).unwrap_or(0)) as usize
                }
            })
        };
        match destino {
            Some(i) => {
                self.sel = Some(orden[i].clone());
                self.pestana = Pestana::Partida;
                self.avisos.clear();
                self.desplazar_arbol = true;
            }
            None => self.info("Todas las partidas tienen precio."),
        }
    }

    fn info(&mut self, t: impl Into<String>) {
        self.mensaje = Some((t.into(), false));
    }

    fn error(&mut self, t: impl Into<String>) {
        self.mensaje = Some((t.into(), true));
    }

    // ----------------------------------------------------------------- ficheros

    fn dialogo_abrir(&mut self) {
        if let Some(ruta) = rfd::FileDialog::new()
            .add_filter("Presupuesto (SQLite)", &["sqlite", "db"])
            .pick_file()
        {
            self.abrir(ruta);
        }
    }

    fn abrir(&mut self, ruta: PathBuf) {
        self.ciego = None;
        self.historial.limpiar();
        let r = (|| -> ppto_db::Resultado<_> {
            let a = Almacen::abrir(&ruta)?;
            let revs = a.listar_revisiones()?;
            let ultima = revs.last().cloned();
            let p = match &ultima {
                Some(r) => Some(a.cargar_revision(r.id)?),
                None => None,
            };
            Ok((revs, ultima, p))
        })();
        match r {
            Ok((revs, Some(ultima), Some(p))) => {
                self.p = p;
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

    /// Primera partida del árbol (en profundidad), para no dejar la vista vacía.
    fn primera_partida(&self, capitulo: &str) -> Option<(String, String)> {
        for l in &self.p.conceptos.get(capitulo)?.descomposicion {
            match self.p.conceptos.get(&l.hijo).map(|c| c.naturaleza) {
                Some(Naturaleza::Capitulo) => {
                    if let Some(r) = self.primera_partida(&l.hijo) {
                        return Some(r);
                    }
                }
                Some(_) => return Some((capitulo.to_owned(), l.hijo.clone())),
                None => {}
            }
        }
        None
    }

    fn exportar_bc3(&mut self, longitud_codigo: Option<usize>) {
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

    fn exportar_excel(&mut self) {
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

    fn dialogo_importar(&mut self) {
        if let Some(ruta) = rfd::FileDialog::new()
            .add_filter("FIEBDC-3 (BC3)", &["bc3", "BC3"])
            .pick_file()
        {
            self.importar_bc3(ruta);
        }
    }

    fn importar_bc3(&mut self, ruta: PathBuf) {
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

    /// Ventana «Acerca de»: versión instalada y dónde está cada cosa.
    fn ventana_acerca(&mut self, ctx: &egui::Context) {
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

    fn ventana_informe(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
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

    fn cambiar_revision(&mut self, id: i64) {
        let Some(ruta) = self.fichero.clone() else { return };
        match Almacen::abrir(&ruta).and_then(|a| a.cargar_revision(id)) {
            Ok(p) => {
                self.p = p;
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

    fn guardar(&mut self, pedir_ruta: bool) {
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

    fn etiqueta_actual(&self) -> String {
        self.revisiones
            .iter()
            .find(|r| Some(r.id) == self.revision_actual)
            .map(|r| r.etiqueta.clone())
            .unwrap_or_else(|| "sin guardar".into())
    }

    // ---------------------------------------------------------------- acciones

    fn aplicar(&mut self, acciones: Vec<Accion>) {
        for a in acciones {
            let guardado = match describir(&a) {
                Some((que, grupo)) => self.historial.antes(&self.p, que, grupo),
                None => {
                    if !matches!(
                        a,
                        Accion::Seleccionar(..) | Accion::Navegar(_) | Accion::SiguienteSinPrecio
                    ) {
                        self.historial.grupo = None;
                    }
                    false
                }
            };
            let r = match a {
                Accion::Seleccionar(padre, hijo) => {
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
                Accion::Portapapeles(x) => {
                    if let Some((_, h, cortar)) = &x {
                        self.info(format!(
                            "{h} {}. Clic derecho sobre el capítulo de destino → «Pegar aquí».",
                            if *cortar { "cortada" } else { "copiada" }
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
                Accion::Precio(c, v) => self.p.fijar_precio(&c, v),
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
    fn copiar_de_base(
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

    fn deshacer(&mut self) {
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

    fn rehacer(&mut self) {
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

    fn tras_historial(&mut self, mensaje: String) {
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

    // --------------------------------------------------------------------- UI

    /// Barra superior: menús (Archivo, Edición, Precios, IA, Ver, Ayuda) y, a
    /// la vista, lo de uso diario (deshacer, guardar, asistente, PEM).
    fn barra_superior(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Archivo", |ui| self.menu_archivo(ui));
            ui.menu_button("Edición", |ui| self.menu_edicion(ui, acciones));
            ui.menu_button("Precios", |ui| self.menu_precios(ui));
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
                ui.hyperlink_to("Novedades", "https://github.com/juancar75/presupuestos-bc3/releases");
            });
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

    fn menu_archivo(&mut self, ui: &mut Ui) {
        let boton = |ui: &mut Ui, texto: &str, atajo: &str| ui.add(egui::Button::new(texto).shortcut_text(atajo));
        if boton(ui, "Nuevo", "Ctrl+N")
            .on_hover_text("Presupuesto vacío")
            .clicked()
        {
            ui.close();
            self.nuevo_presupuesto();
        }
        if boton(ui, "📂 Abrir…", "Ctrl+O").clicked() {
            ui.close();
            self.dialogo_abrir();
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

    fn menu_edicion(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
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

    fn menu_precios(&mut self, ui: &mut Ui) {
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

    fn menu_ia(&mut self, ui: &mut Ui) {
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

    fn arbol(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
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
        let campo = ui.add(
            egui::TextEdit::singleline(&mut self.buscar)
                .hint_text("Buscar partida por código o texto… (Ctrl+F)")
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.enfocar_buscar) {
            campo.request_focus();
        }
        let buscar = self.buscar.trim().to_lowercase();
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
    fn resultados(&self, ui: &mut Ui, buscar: &str, acciones: &mut Vec<Accion>) {
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
                egui::Button::selectable(elegido, format!("{}  {}", c.codigo, corto(&c.resumen, 34)))
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

    fn rama(&self, ui: &mut Ui, capitulo: &str, acciones: &mut Vec<Accion>) {
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
                            egui::Button::new(RichText::new(format!("{}  {}", c.codigo, c.resumen)).strong())
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
                    egui::Button::selectable(elegido, format!("{}  {}", c.codigo, corto(&c.resumen, 34)))
                        .wrap_mode(egui::TextWrapMode::Truncate)
                        .sense(egui::Sense::click_and_drag()),
                );
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

    fn pestana_partida(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
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
                    ui.label(RichText::new(format!("⚠ {a}")).color(Color32::from_rgb(220, 150, 40)));
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
                ui.label(RichText::new(format!("⚠ {a}")).color(Color32::from_rgb(220, 150, 40)));
            }
        });
    }

    fn hoja_medicion(
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
            ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(220, 80, 60)));
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

    fn pestana_recursos(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
        let Some(e) = &self.calc.explosion else { return };
        let e = e.clone();
        let dec = self.p.decimales;
        ui.label("Cantidades totales necesarias para ejecutar el presupuesto. Los precios se pueden editar aquí.");
        self.resumen_recursos(ui);
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
            ] {
                let grupo: Vec<_> = e.por_naturaleza(nat).cloned().collect();
                if grupo.is_empty() {
                    continue;
                }
                let subtotal: Decimal = grupo.iter().map(|r| r.importe).sum();
                ui.label(
                    RichText::new(format!("{}  —  {} €", nombre_naturaleza(nat), eur(subtotal)))
                        .strong()
                        .color(color_naturaleza(nat, ui.visuals().dark_mode)),
                );
                egui::Grid::new(("recursos", nombre_naturaleza(nat)))
                    .striped(true)
                    .num_columns(7)
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

    fn pestana_subcontratar(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
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

    fn pestana_resumen(&mut self, ui: &mut Ui, acciones: &mut Vec<Accion>) {
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
            ] {
                ui.label(nombre);
                let v = match clave {
                    "gg" => &mut self.gg,
                    "bi" => &mut self.bi,
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
        let r = venta::resumen(pem, self.gg, self.bi, self.iva, 2);
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
                    ("Presupuesto base".into(), r.base, true),
                    (format!("{} % IVA", num(self.iva, 2)), r.iva, false),
                    ("TOTAL".into(), r.total, true),
                ];
                for (t, v, fuerte) in filas {
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

    /// Coste y venta de la partida seleccionada, paso a paso: coste directo
    /// (por naturaleza) → costes indirectos → PEM → GG → BI → venta.
    fn coste_y_venta(&mut self, ui: &mut Ui, hijo: &str, unidad: &str, l: &LineaValorada, acciones: &mut Vec<Accion>) {
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
    fn tabla_coste_venta(&mut self, ui: &mut Ui) {
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

    fn etiqueta_partida(&self, codigo: &str) -> String {
        match self.p.conceptos.get(codigo) {
            Some(c) => format!("{} — {}", c.codigo, corto(&c.resumen, 40)),
            None => codigo.to_owned(),
        }
    }
}

/// Sobrecoste ((venta − coste) / coste) y margen sobre venta
/// ((venta − coste) / venta), en %, de unos importes.
fn sobrecoste_y_margen(coste: Decimal, venta: Decimal) -> (Option<Decimal>, Option<Decimal>) {
    let b = venta - coste;
    let pct = |base: Decimal| (!base.is_zero()).then(|| b / base * Decimal::ONE_HUNDRED);
    (pct(coste), pct(venta))
}

impl eframe::App for Aplicacion {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let titulo = format!(
            "{}{} — {} — presupuestos-bc3 {VERSION}",
            if self.cambios { "● " } else { "" },
            self.p.nombre,
            self.fichero
                .as_ref()
                .and_then(|f| f.file_name())
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sin guardar".into())
        );
        if titulo != self.titulo {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(titulo.clone()));
            self.titulo = titulo;
        }
        let ctx0 = ui.ctx().clone();
        self.sincronizar_ia(&ctx0);

        let mut acciones = Vec::new();
        // Ctrl+Z / Ctrl+Y (o Ctrl+Mayús+Z): deshacer / rehacer, salvo mientras
        // se escribe en un campo (allí deshace el propio campo).
        if !ui.ctx().egui_wants_keyboard_input() {
            let (rehacer, deshacer) = ui.ctx().input_mut(|i| {
                let r = i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
                    || i.consume_key(egui::Modifiers::CTRL, egui::Key::Y);
                (r, i.consume_key(egui::Modifiers::CTRL, egui::Key::Z))
            });
            if deshacer {
                self.deshacer();
            }
            if rehacer {
                self.rehacer();
            }
        }
        // Atajos de Archivo y Ctrl+F (buscar)
        let (nuevo, abrir, guardar_como, guardar, buscar, pdf) = ui.ctx().input_mut(|i| {
            let c = egui::Modifiers::CTRL;
            let cs = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
            let gc = i.consume_key(cs, egui::Key::S);
            (
                i.consume_key(c, egui::Key::N),
                i.consume_key(c, egui::Key::O),
                gc,
                i.consume_key(c, egui::Key::S),
                i.consume_key(c, egui::Key::F),
                i.consume_key(c, egui::Key::P),
            )
        });
        if pdf {
            self.abrir_pdf();
        }
        // Ctrl+1…Ctrl+6: vistas
        if let Some(p) = ui.ctx().input_mut(|i| {
            vistas::VISTAS
                .iter()
                .find(|(_, _, k)| i.consume_key(egui::Modifiers::CTRL, *k))
                .map(|(p, _, _)| *p)
        }) {
            self.pestana = p;
        }
        if nuevo {
            self.nuevo_presupuesto();
        }
        if abrir {
            self.dialogo_abrir();
        }
        if guardar_como {
            self.guardar(true);
        } else if guardar && (self.cambios || self.fichero.is_none()) {
            self.guardar(false);
        }
        if buscar {
            self.enfocar_buscar = true;
        }
        // Ctrl+↓ / Ctrl+↑: partida siguiente / anterior; Ctrl+Mayús+↓: siguiente sin precio
        let (sin_precio, bajar, subir) = ui.ctx().input_mut(|i| {
            let sin_precio = i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::ArrowDown);
            (
                sin_precio,
                i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowUp),
            )
        });
        if sin_precio {
            acciones.push(Accion::SiguienteSinPrecio);
        } else if bajar {
            acciones.push(Accion::Navegar(1));
        }
        if subir {
            acciones.push(Accion::Navegar(-1));
        }
        egui::Panel::top("barra").show(ui, |ui| {
            ui.add_space(4.0);
            self.barra_superior(ui, &mut acciones);
            ui.add_space(2.0);
            if std::mem::take(&mut self.enlace.reaplicar) {
                acciones.push(Accion::ReaplicarAceptadas);
            }
        });
        egui::Panel::bottom("estado").show(ui, |ui| {
            ui.horizontal(|ui| match (&self.calc.error, &self.mensaje) {
                (Some(e), _) => {
                    let e = e.replace('→', "->");
                    ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(220, 80, 60)));
                }
                (None, Some((m, true))) => {
                    ui.label(RichText::new(format!("⚠ {m}")).color(Color32::from_rgb(220, 80, 60)));
                }
                (None, Some((m, false))) => {
                    ui.label(m);
                }
                _ => {}
            });
        });
        self.panel_asistente(ui);
        egui::Panel::left("arbol")
            .resizable(true)
            .default_size(330.0)
            .show(ui, |ui| {
                self.arbol(ui, &mut acciones);
            });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (n, (p, t, _)) in vistas::VISTAS.iter().enumerate() {
                    let t = if *p == Pestana::Partida { "Partida" } else { t };
                    ui.selectable_value(&mut self.pestana, *p, RichText::new(t).size(15.0))
                        .on_hover_text(format!("Ctrl+{}", n + 1));
                }
            });
            ui.separator();
            match self.pestana {
                Pestana::Partida => self.pestana_partida(ui, &mut acciones),
                Pestana::Presupuesto => self.pestana_presupuesto(ui, &mut acciones),
                Pestana::Precios => self.pestana_precios(ui, &mut acciones),
                Pestana::Mediciones => self.pestana_mediciones(ui, &mut acciones),
                Pestana::Recursos => self.pestana_recursos(ui, &mut acciones),
                Pestana::Subcontratar => self.pestana_subcontratar(ui, &mut acciones),
                Pestana::Resumen => self.pestana_resumen(ui, &mut acciones),
            }
        });
        let ctx = ui.ctx().clone();
        self.ventana_informe(&ctx, &mut acciones);
        self.ventana_acerca(&ctx);
        self.ventana_alta(&ctx, &mut acciones);
        self.ventana_pdf(&ctx);
        self.ventana_tarifas(&ctx);
        self.ventana_biblioteca(&ctx);
        self.ventana_bases(&ctx, &mut acciones);
        self.ventana_legalizacion(&ctx);
        self.ventana_propuestas(&ctx, &mut acciones);
        if !acciones.is_empty() {
            self.aplicar(acciones);
        }
    }
}

/// Lo que se arrastra en el árbol: una línea (`hijo`) de un capítulo (`origen`).
/// Versión del programa (la de Cargo.toml).
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Memoria temporal: rectángulo del campo «Título» del menú del capítulo.
const CAMPO_TITULO: &str = "campo_titulo_capitulo";

#[derive(Clone)]
struct Arrastre {
    origen: String,
    hijo: String,
}

/// Opciones comunes a cualquier línea del árbol.
fn menu_linea(ui: &mut Ui, padre: &str, hijo: &str, es_capitulo: bool, acciones: &mut Vec<Accion>) {
    if ui
        .button("Cortar")
        .on_hover_text("Para moverla: luego clic derecho en el capítulo de destino → «Pegar aquí»")
        .clicked()
    {
        acciones.push(Accion::Portapapeles(Some((padre.to_owned(), hijo.to_owned(), true))));
    }
    if !es_capitulo
        && ui
            .button("Copiar")
            .on_hover_text("Para ponerla también en otro capítulo, con su cantidad")
            .clicked()
    {
        acciones.push(Accion::Portapapeles(Some((padre.to_owned(), hijo.to_owned(), false))));
    }
    ui.separator();
    if ui.button("Subir").clicked() {
        acciones.push(Accion::Mover(padre.to_owned(), hijo.to_owned(), true));
    }
    if ui.button("Bajar").clicked() {
        acciones.push(Accion::Mover(padre.to_owned(), hijo.to_owned(), false));
    }
    ui.separator();
    if ui
        .button(format!("Quitar {hijo} de {padre}"))
        .on_hover_text("Lo que quede sin usar en ningún otro sitio se borra")
        .clicked()
    {
        acciones.push(Accion::QuitarLinea(padre.to_owned(), hijo.to_owned(), true));
    }
}

fn corto(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

fn nombre_naturaleza(n: Naturaleza) -> &'static str {
    match n {
        Naturaleza::Capitulo => "Capítulo",
        Naturaleza::Partida => "Partida",
        Naturaleza::ManoObra => "Mano de obra",
        Naturaleza::Maquinaria => "Maquinaria",
        Naturaleza::Material => "Material",
        Naturaleza::Subcontrata => "Subcontrata",
        Naturaleza::Porcentaje => "Porcentaje",
        Naturaleza::Otros => "Otros",
    }
}

fn color_naturaleza(n: Naturaleza, oscuro: bool) -> Color32 {
    match n {
        Naturaleza::ManoObra => Color32::from_rgb(90, 150, 230),
        Naturaleza::Material => Color32::from_rgb(80, 175, 110),
        Naturaleza::Maquinaria => Color32::from_rgb(200, 140, 60),
        Naturaleza::Subcontrata => Color32::from_rgb(170, 110, 210),
        Naturaleza::Porcentaje => Color32::GRAY,
        _ if oscuro => Color32::LIGHT_GRAY,
        _ => Color32::from_gray(60),
    }
}

#[cfg(test)]
mod pruebas {
    //! Edición desde la interfaz (P-014): acciones y pintado sin ventana.
    use super::*;
    use eframe::egui::{Context, RawInput};
    use rust_decimal_macros::dec;

    /// Pinta árbol, pestaña Partida y ventana de alta; devuelve las acciones pedidas.
    fn pintar(app: &mut Aplicacion) -> Vec<Accion> {
        let ctx = Context::default();
        let mut acciones = Vec::new();
        for _ in 0..2 {
            let mut salida = ctx.run_ui(RawInput::default(), |ui| {
                egui::Panel::left("a").show(ui, |ui| app.arbol(ui, &mut acciones));
                egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_partida(ui, &mut acciones));
                let c = ui.ctx().clone();
                app.ventana_alta(&c, &mut acciones);
            });
            salida.textures_delta.clear();
        }
        acciones
    }

    #[test]
    fn presupuesto_nuevo_completo_desde_la_interfaz() {
        // Mismo caso que el motor: P1 = 0,5 × 20 + 2 × 3,10 + 3 % = 16,69; 12 m → 200,28 €
        let mut app = Aplicacion::sin_ventana();
        app.nuevo_presupuesto();
        assert_eq!(app.calc.pem, Decimal::ZERO);
        pintar(&mut app);
        app.aplicar(vec![Accion::NuevoCapitulo {
            padre: "OBRA".into(),
            codigo: "C01".into(),
            resumen: "Fontanería".into(),
        }]);
        app.aplicar(vec![Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P1".into(),
            unidad: "m".into(),
            resumen: "Tubería".into(),
        }]);
        assert_eq!(app.sel, Some(("C01".into(), "P1".into())));
        pintar(&mut app);
        let recurso = |codigo: &str, n, precio, q| Accion::NuevoRecurso {
            codigo: codigo.into(),
            unidad: "u".into(),
            resumen: codigo.into(),
            naturaleza: n,
            precio,
            en: "P1".into(),
            cantidad: q,
        };
        app.aplicar(vec![
            recurso("MO1", Naturaleza::ManoObra, dec!(20), dec!(0.5)),
            recurso("MT1", Naturaleza::Material, dec!(3.10), dec!(2)),
            recurso("%MA", Naturaleza::Porcentaje, dec!(0), dec!(3)),
            Accion::Rendimiento("C01".into(), "P1".into(), dec!(12)),
        ]);
        assert_eq!(app.calc.error, None);
        assert_eq!(app.calc.pem, dec!(200.28));
        assert!(app.cambios);
        pintar(&mut app);

        // Un recurso con código ya usado no se cuelga por error en lugar del nuevo
        app.aplicar(vec![recurso("MO1", Naturaleza::Material, dec!(99), dec!(1))]);
        assert!(app.mensaje.as_ref().is_some_and(|(m, e)| *e && m.contains("duplicado")));
        assert_eq!(app.calc.pem, dec!(200.28));
        // Un recurso nuevo que no puede colgarse no se queda huérfano
        app.aplicar(vec![Accion::NuevoRecurso {
            codigo: "MO9".into(),
            unidad: "h".into(),
            resumen: "x".into(),
            naturaleza: Naturaleza::ManoObra,
            precio: dec!(1),
            en: "C01".into(),
            cantidad: dec!(1),
        }]);
        assert!(!app.p.conceptos.contains_key("MO9"));

        // Quitar la partida del árbol: se borra con todo lo que solo usaba ella
        app.aplicar(vec![Accion::QuitarLinea("C01".into(), "P1".into(), true)]);
        assert_eq!(app.calc.pem, Decimal::ZERO);
        assert_eq!(app.sel, None);
        assert!(!app.p.conceptos.contains_key("MO1"));
        pintar(&mut app);
    }

    #[test]
    fn ventana_de_alta_y_lineas_del_descompuesto() {
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::Alta(TipoAlta::Linea { padre: "SR.M2".into() })]);
        assert!(app.alta.as_ref().is_some_and(|a| a.codigo == "R0001"));
        pintar(&mut app);
        assert!(app.alta.is_some(), "la ventana sigue abierta hasta confirmar");
        // Quitar y volver a poner el colector cambia el PEM y lo recupera
        app.aplicar(vec![Accion::QuitarLinea("C01".into(), "SR.COL8".into(), false)]);
        assert_eq!(app.calc.pem, dec!(6276.90));
        app.aplicar(vec![Accion::AnadirLinea("C01".into(), "SR.COL8".into(), dec!(3))]);
        assert_eq!(app.calc.pem, dec!(7005.15));
        // Mover y cambiar unidad
        let antes: Vec<_> = app.p.conceptos["SR.M2"]
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect();
        app.aplicar(vec![Accion::Mover("SR.M2".into(), antes[1].clone(), true)]);
        assert_eq!(app.p.conceptos["SR.M2"].descomposicion[0].hijo, antes[1]);
        app.aplicar(vec![Accion::Unidad("SR.M2".into(), " m² ".into())]);
        assert_eq!(app.p.conceptos["SR.M2"].unidad, "m²");
        pintar(&mut app);
    }

    #[test]
    fn subpartidas_desde_la_interfaz() {
        // Mismo caso que el motor: AUX2 10,00 → AUX1 23,10 → P1 25,10; 10 m → 251,00 €
        let mut app = Aplicacion::sin_ventana();
        app.nuevo_presupuesto();
        app.aplicar(vec![
            Accion::NuevoCapitulo {
                padre: "OBRA".into(),
                codigo: "C01".into(),
                resumen: "Cap".into(),
            },
            Accion::NuevaPartida {
                capitulo: "C01".into(),
                codigo: "P1".into(),
                unidad: "m".into(),
                resumen: "Partida".into(),
            },
        ]);
        let nuevo = |codigo: &str, n, precio, en: &str, q| Accion::NuevoRecurso {
            codigo: codigo.into(),
            unidad: "ud".into(),
            resumen: codigo.into(),
            naturaleza: n,
            precio,
            en: en.into(),
            cantidad: q,
        };
        app.aplicar(vec![
            nuevo("AUX1", Naturaleza::Partida, dec!(0), "P1", dec!(1)),
            nuevo("MO", Naturaleza::ManoObra, dec!(20), "P1", dec!(0.1)),
        ]);
        // Entrar en la subpartida (clic en su código) y descomponerla
        app.aplicar(vec![Accion::Seleccionar("P1".into(), "AUX1".into())]);
        pintar(&mut app);
        app.aplicar(vec![
            nuevo("AUX2", Naturaleza::Partida, dec!(0), "AUX1", dec!(2)),
            nuevo("MT", Naturaleza::Material, dec!(3.10), "AUX1", dec!(1)),
            Accion::Seleccionar("AUX1".into(), "AUX2".into()),
            Accion::AnadirLinea("AUX2".into(), "MO".into(), dec!(0.5)),
            Accion::Rendimiento("C01".into(), "P1".into(), dec!(10)),
        ]);
        pintar(&mut app);
        assert_eq!(app.calc.error, None);
        assert_eq!(app.calc.precios["AUX1"], dec!(23.10));
        assert_eq!(app.calc.pem, dec!(251.00));
        assert_eq!(app.p.ruta("AUX2"), ["OBRA", "C01", "P1", "AUX1", "AUX2"]);
    }

    #[test]
    fn arrastrar_partida_a_otro_capitulo() {
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::NuevoCapitulo {
            padre: app.p.raiz.clone(),
            codigo: "C02".into(),
            resumen: "Otro".into(),
        }]);
        app.aplicar(vec![Accion::Trasladar(
            "C01".into(),
            "SR.M2".into(),
            "C02".into(),
            None,
        )]);
        assert_eq!(app.calc.pem, dec!(7005.15));
        assert_eq!(
            app.sel,
            Some(("C02".into(), "SR.M2".into())),
            "la selección sigue a la partida"
        );
        assert_eq!(app.calc.lineas["C02"][0].importe, dec!(6276.90));
        pintar(&mut app);
    }

    #[test]
    fn subcapitulos_con_codigo_bajo_su_padre() {
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::Alta(TipoAlta::Capitulo {
            padre: app.p.raiz.clone(),
        })]);
        assert_eq!(app.alta.as_ref().unwrap().codigo, "C02");
        app.aplicar(vec![Accion::Alta(TipoAlta::Capitulo { padre: "C01".into() })]);
        assert_eq!(app.alta.as_ref().unwrap().codigo, "C01.01");
        pintar(&mut app);
    }

    #[test]
    fn precio_directo_de_partida_sin_descomponer() {
        // Como al valorar un ciego: 12 m a 15,50 €/m = 186,00
        let mut app = Aplicacion::sin_ventana();
        app.nuevo_presupuesto();
        app.aplicar(vec![
            Accion::NuevoCapitulo {
                padre: "OBRA".into(),
                codigo: "C01".into(),
                resumen: "Cap".into(),
            },
            Accion::NuevaPartida {
                capitulo: "C01".into(),
                codigo: "P1".into(),
                unidad: "m".into(),
                resumen: "Tubería".into(),
            },
            Accion::Rendimiento("C01".into(), "P1".into(), dec!(12)),
            Accion::Precio("P1".into(), dec!(15.50)),
        ]);
        assert_eq!(app.calc.pem, dec!(186.00));
        pintar(&mut app);
    }

    #[test]
    fn navegar_entre_partidas() {
        // Ejemplo: SR.M2 y SR.COL8 en C01; se añade C02 con P9 sin precio
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![
            Accion::NuevoCapitulo {
                padre: app.p.raiz.clone(),
                codigo: "C02".into(),
                resumen: "Otro".into(),
            },
            Accion::NuevaPartida {
                capitulo: "C02".into(),
                codigo: "P9".into(),
                unidad: "u".into(),
                resumen: "Sin precio".into(),
            },
        ]);
        assert_eq!(
            app.orden_partidas(),
            [
                ("C01".to_owned(), "SR.M2".to_owned()),
                ("C01".into(), "SR.COL8".into()),
                ("C02".into(), "P9".into())
            ]
        );
        app.aplicar(vec![Accion::Seleccionar("C01".into(), "SR.M2".into())]);
        app.aplicar(vec![Accion::Navegar(1)]);
        assert_eq!(app.sel, Some(("C01".into(), "SR.COL8".into())));
        app.aplicar(vec![Accion::Navegar(5)]);
        assert_eq!(app.sel, Some(("C02".into(), "P9".into())), "no se pasa del final");
        app.aplicar(vec![Accion::Navegar(-1)]);
        assert_eq!(app.sel, Some(("C01".into(), "SR.COL8".into())));
        app.aplicar(vec![Accion::SiguienteSinPrecio]);
        assert_eq!(app.sel, Some(("C02".into(), "P9".into())));
        assert!(app.desplazar_arbol);
        pintar(&mut app);
        app.buscar = "colector".into();
        pintar(&mut app);
    }

    /// Clic real con el ratón en una partida del árbol (regresión: el
    /// arrastre no debe anular el clic).
    #[test]
    fn clic_en_partida_del_arbol() {
        use eframe::egui::{Event, PointerButton, Pos2};
        let mut elegidas = std::collections::BTreeSet::new();
        for y in (60..420).step_by(6) {
            let mut app = Aplicacion::sin_ventana();
            app.sel = None;
            let ctx = Context::default();
            let mut acciones = Vec::new();
            let pos = Pos2::new(90.0, y as f32);
            let boton = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            for e in [
                vec![],
                vec![Event::PointerMoved(pos)],
                vec![boton(true)],
                vec![boton(false)],
                vec![],
            ] {
                let entrada = RawInput {
                    events: e,
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(entrada, |ui| {
                    egui::Panel::left("a")
                        .default_size(330.0)
                        .show(ui, |ui| app.arbol(ui, &mut acciones));
                });
                out.textures_delta.clear();
            }
            for a in acciones {
                if let Accion::Seleccionar(_, h) = a {
                    elegidas.insert(h);
                }
            }
        }
        assert_eq!(elegidas.into_iter().collect::<Vec<_>>(), ["SR.COL8", "SR.M2"]);
    }

    /// Arrastrar con el ratón SR.COL8 encima de SR.M2 la coloca delante.
    /// Arrastrar SR.COL8 desde `desde` hasta `hasta` en un árbol con un
    /// subcapítulo vacío C01.01; devuelve las acciones.
    fn arrastrar_a_subcapitulo(desde: f32, hasta: f32) -> Vec<Accion> {
        use eframe::egui::{Event, PointerButton, Pos2};
        let mut app = Aplicacion::sin_ventana();
        app.p.nuevo_capitulo("C01", "C01.01", "Planta baja").unwrap();
        let raiz = app.p.raiz.clone();
        app.p.nuevo_capitulo(&raiz, "C02", "Colectores").unwrap();
        app.recalcular();
        let ctx = Context::default();
        let mut acc = Vec::new();
        let boton = |y, pressed| Event::PointerButton {
            pos: Pos2::new(90.0, y),
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let mut eventos = vec![
            vec![],
            vec![Event::PointerMoved(Pos2::new(90.0, desde))],
            vec![boton(desde, true)],
        ];
        for k in 1..=10 {
            eventos.push(vec![Event::PointerMoved(Pos2::new(
                90.0,
                desde + (hasta - desde) * k as f32 / 10.0,
            ))]);
        }
        eventos.push(vec![boton(hasta, false)]);
        eventos.push(vec![]);
        for e in eventos {
            let entrada = RawInput {
                events: e,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(entrada, |ui| {
                egui::Panel::left("a")
                    .default_size(330.0)
                    .show(ui, |ui| app.arbol(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
        acc
    }

    #[test]
    fn cortar_y_copiar_a_un_subcapitulo() {
        let mut app = Aplicacion::sin_ventana();
        app.p.nuevo_capitulo("C01", "C01.01", "Planta baja").unwrap();
        app.recalcular();
        // Copiar SR.COL8 (3 ud × 242,75 = 728,25) a C01.01: el PEM pasa de 7.005,15 a 7.733,40
        app.aplicar(vec![
            Accion::Portapapeles(Some(("C01".into(), "SR.COL8".into(), false))),
            Accion::Copiar("C01".into(), "SR.COL8".into(), "C01.01".into()),
        ]);
        assert_eq!(app.p.pem().unwrap(), dec!(7733.40));
        assert!(app.portapapeles.is_some(), "copiar deja la partida para pegar otra vez");
        // Cortar SR.M2 y pegar en C01.01: el PEM no cambia y se vacía el portapapeles
        app.aplicar(vec![
            Accion::Portapapeles(Some(("C01".into(), "SR.M2".into(), true))),
            Accion::Trasladar("C01".into(), "SR.M2".into(), "C01.01".into(), None),
        ]);
        assert_eq!(app.p.pem().unwrap(), dec!(7733.40));
        assert!(app.p.mediciones.contains_key(&("C01.01".into(), "SR.M2".into())));
        assert!(app.portapapeles.is_none());
    }

    #[test]
    fn arrastrar_partida_a_un_subcapitulo() {
        // Buscar la y de SR.COL8 (la que, al soltar sin moverse, no traslada nada)
        // y comprobar que alguna posición de soltar la lleva a C01.01.
        let ys: Vec<f32> = (40..500).step_by(4).map(|y| y as f32).collect();
        let desde = ys.iter().copied().find(|&y| {
            arrastrar_a_subcapitulo(y, y + 1.0)
                .iter()
                .any(|a| matches!(a, Accion::Seleccionar(_, h) if h == "SR.COL8"))
        });
        let desde = desde.expect("no se encontró SR.COL8 en el árbol");
        let llega = ys.iter().any(|&h| {
            arrastrar_a_subcapitulo(desde, h)
                .iter()
                .any(|a| matches!(a, Accion::Trasladar(o, x, d, _) if o == "C01" && x == "SR.COL8" && d == "C01.01"))
        });
        assert!(llega, "no se puede soltar una partida en un subcapítulo vacío");
    }

    /// Clics sucesivos en el árbol (x = 90) con la misma aplicación.
    fn clics(ys: &[f32]) -> Vec<Accion> {
        use eframe::egui::{Event, PointerButton, Pos2};
        let mut app = Aplicacion::sin_ventana();
        let ctx = Context::default();
        let mut acc = Vec::new();
        let mut eventos = vec![vec![]];
        for &y in ys {
            let p = Pos2::new(90.0, y);
            let b = |pressed| Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            eventos.extend([
                vec![Event::PointerMoved(p)],
                vec![b(true)],
                vec![b(false)],
                vec![],
                vec![],
            ]);
        }
        for e in eventos {
            let entrada = RawInput {
                events: e,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(entrada, |ui| {
                egui::Panel::left("a")
                    .default_size(330.0)
                    .show(ui, |ui| app.arbol(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
        acc
    }

    #[test]
    fn clic_en_el_titulo_contrae_el_capitulo() {
        let elige = |acc: &[Accion]| {
            acc.iter()
                .any(|a| matches!(a, Accion::Seleccionar(_, h) if h == "SR.COL8"))
        };
        let ys: Vec<f32> = (40..500).step_by(4).map(|y| y as f32).collect();
        let y_col8 = ys.iter().copied().find(|&y| elige(&clics(&[y]))).unwrap();
        // Algún clic por encima (el título de C01) oculta SR.COL8, y un segundo
        // clic en el mismo sitio la vuelve a mostrar.
        let titulo = ys
            .iter()
            .copied()
            .filter(|&y| y < y_col8 - 20.0)
            .find(|&y| !elige(&clics(&[y, y_col8])));
        let titulo = titulo.expect("el clic en el título no contrae el capítulo");
        assert!(
            elige(&clics(&[titulo, titulo, y_col8])),
            "un segundo clic debe volver a abrirlo"
        );
    }

    #[test]
    fn poner_titulo_a_un_subcapitulo_pinchando_en_el_campo() {
        use eframe::egui::{Event, Key, PointerButton, Pos2};
        // Capítulo con título (el campo no recibe el foco solo): clic derecho,
        // clic en el campo, borrar, escribir y Enter.
        let mut hecho = false;
        for y in (40..200).step_by(4) {
            let mut app = Aplicacion::sin_ventana();
            app.p.nuevo_capitulo("C01", "C01.01", "Viejo").unwrap();
            app.recalcular();
            let ctx = Context::default();
            let mut acc = Vec::new();
            let frame = |e: Vec<Event>, app: &mut Aplicacion, acc: &mut Vec<Accion>| {
                let entrada = RawInput {
                    events: e,
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(entrada, |ui| {
                    egui::Panel::left("a")
                        .default_size(330.0)
                        .show(ui, |ui| app.arbol(ui, acc));
                });
                out.textures_delta.clear();
            };
            let boton = |pos, button, pressed| Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            };
            let p = Pos2::new(60.0, y as f32);
            frame(vec![], &mut app, &mut acc);
            frame(vec![Event::PointerMoved(p)], &mut app, &mut acc);
            frame(vec![boton(p, PointerButton::Secondary, true)], &mut app, &mut acc);
            frame(vec![boton(p, PointerButton::Secondary, false)], &mut app, &mut acc);
            frame(vec![], &mut app, &mut acc);
            let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(CAMPO_TITULO))) else {
                continue;
            };
            // Mover hasta el campo y pinchar (pulsar y soltar en fotogramas distintos)
            let q = r.center();
            for k in 1..=5 {
                frame(
                    vec![Event::PointerMoved(p + (q - p) * k as f32 / 5.0)],
                    &mut app,
                    &mut acc,
                );
            }
            frame(vec![boton(q, PointerButton::Primary, true)], &mut app, &mut acc);
            frame(vec![], &mut app, &mut acc);
            frame(vec![boton(q, PointerButton::Primary, false)], &mut app, &mut acc);
            frame(vec![], &mut app, &mut acc);
            // ¿Sigue dibujándose el menú? (la marca se pone en cada fotograma)
            ctx.data_mut(|d| d.remove::<String>(egui::Id::new(CAMPO_TITULO).with("abierto")));
            frame(vec![], &mut app, &mut acc);
            let cap = ctx.data(|d| d.get_temp::<String>(egui::Id::new(CAMPO_TITULO).with("abierto")));
            let cap = cap.expect("el menú se cerró al pinchar en el título");
            let tecla = |key, pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            };
            let mut borrar = Vec::new();
            for _ in 0..6 {
                borrar.extend([tecla(Key::Backspace, true), tecla(Key::Backspace, false)]);
            }
            eprintln!("foco={:?}", ctx.memory(|m| m.focused()));
            frame(borrar, &mut app, &mut acc);
            eprintln!(
                "abierto tras borrar={:?}",
                ctx.data(|d| d.get_temp::<String>(egui::Id::new(CAMPO_TITULO).with("abierto")))
            );
            frame(vec![Event::Text("Planta baja".into())], &mut app, &mut acc);
            frame(
                vec![tecla(Key::Enter, true), tecla(Key::Enter, false)],
                &mut app,
                &mut acc,
            );
            frame(vec![], &mut app, &mut acc);
            hecho = acc
                .iter()
                .any(|a| matches!(a, Accion::Resumen(c, t) if *c == cap && t.contains("Planta baja")));
            assert!(hecho, "no se guardó el título de {cap}");
            break;
        }
        assert!(hecho, "no se encontró ningún capítulo");
    }

    #[test]
    fn soltar_en_el_espacio_libre_lleva_al_nivel_principal() {
        // El subcapítulo C01.01 arrastrado al hueco de debajo del árbol pasa a la raíz
        let raiz = Aplicacion::sin_ventana().p.raiz;
        let llega = (40..400).step_by(4).any(|y| {
            arrastrar_a_subcapitulo(y as f32, 760.0)
                .iter()
                .any(|a| matches!(a, Accion::Trasladar(o, h, d, None) if o == "C01" && h == "C01.01" && *d == raiz))
        });
        assert!(llega, "no se puede llevar un subcapítulo al nivel principal");
    }

    #[test]
    fn arrastrar_un_capitulo_dentro_de_otro() {
        let ys: Vec<f32> = (40..500).step_by(4).map(|y| y as f32).collect();
        let a_sub = |acc: &[Accion], h: &str| {
            acc.iter()
                .any(|a| matches!(a, Accion::Trasladar(_, x, d, _) if x == h && d == "C01.01"))
        };
        let desde = ys
            .iter()
            .copied()
            .find(|&y| {
                arrastrar_a_subcapitulo(y, y + 1.0)
                    .iter()
                    .any(|a| matches!(a, Accion::Seleccionar(_, h) if h == "SR.COL8"))
            })
            .unwrap();
        let destino = ys
            .iter()
            .copied()
            .find(|&h| a_sub(&arrastrar_a_subcapitulo(desde, h), "SR.COL8"))
            .unwrap();
        // El título de C02 (capítulo de la raíz) arrastrado al de C01.01
        let llega = ys.iter().any(|&y| {
            let acc = arrastrar_a_subcapitulo(y, destino);
            acc.iter()
                .any(|a| matches!(a, Accion::Trasladar(o, x, d, _) if *o == Aplicacion::sin_ventana().p.raiz && x == "C02" && d == "C01.01"))
        });
        assert!(llega, "no se puede arrastrar un capítulo a otro");
    }

    #[test]
    fn arrastre_con_raton() {
        use eframe::egui::{Event, PointerButton, Pos2};
        // Posiciones de las dos partidas (clic de prueba en cada y)
        let mut pos_de = std::collections::BTreeMap::new();
        for y in (60..420).step_by(6) {
            let mut app = Aplicacion::sin_ventana();
            let ctx = Context::default();
            let mut acc = Vec::new();
            let p = Pos2::new(90.0, y as f32);
            let b = |pressed| Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            for e in [vec![], vec![Event::PointerMoved(p)], vec![b(true)], vec![b(false)]] {
                let entrada = RawInput {
                    events: e,
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(entrada, |ui| {
                    egui::Panel::left("a")
                        .default_size(330.0)
                        .show(ui, |ui| app.arbol(ui, &mut acc));
                });
                out.textures_delta.clear();
            }
            for a in acc {
                if let Accion::Seleccionar(_, h) = a {
                    pos_de.entry(h).or_insert(y as f32);
                }
            }
        }
        let (desde, hasta) = (pos_de["SR.COL8"], pos_de["SR.M2"]);
        let mut app = Aplicacion::sin_ventana();
        let ctx = Context::default();
        let mut acc = Vec::new();
        let p0 = Pos2::new(90.0, desde);
        let mut eventos = vec![
            vec![],
            vec![Event::PointerMoved(p0)],
            vec![Event::PointerButton {
                pos: p0,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        ];
        for k in 1..=10 {
            eventos.push(vec![Event::PointerMoved(Pos2::new(
                90.0,
                desde + (hasta - desde) * k as f32 / 10.0,
            ))]);
        }
        let p1 = Pos2::new(90.0, hasta);
        eventos.push(vec![Event::PointerButton {
            pos: p1,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        for e in eventos {
            let entrada = RawInput {
                events: e,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(entrada, |ui| {
                egui::Panel::left("a")
                    .default_size(330.0)
                    .show(ui, |ui| app.arbol(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
        assert!(
            acc.iter().any(|a| matches!(a, Accion::Trasladar(o, h, d, Some(antes)) if o == "C01" && h == "SR.COL8" && d == "C01" && antes == "SR.M2")),
            "no hubo traslado"
        );
        assert!(
            !acc.iter().any(|a| matches!(a, Accion::Seleccionar(..))),
            "arrastrar no debe contar como clic"
        );
    }

    #[test]
    fn deshacer_y_rehacer() {
        // Ejemplo: PEM 7.005,15. Tubo 1,10 → 1,25: 7.229,85. Quitar SR.COL8: 6.501,60.
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::Precio("MT.TUBO16".into(), dec!(1.25))]);
        assert_eq!(app.calc.pem, dec!(7229.85));
        app.aplicar(vec![Accion::QuitarLinea("C01".into(), "SR.COL8".into(), true)]);
        assert_eq!(app.calc.pem, dec!(6501.60));
        app.deshacer();
        assert_eq!(app.calc.pem, dec!(7229.85));
        assert!(app.p.conceptos.contains_key("SR.COL8"), "vuelve con su descompuesto");
        app.deshacer();
        assert_eq!(app.calc.pem, dec!(7005.15));
        app.deshacer(); // nada más que deshacer
        assert_eq!(app.calc.pem, dec!(7005.15));
        app.rehacer();
        app.rehacer();
        assert_eq!(app.calc.pem, dec!(6501.60));
        // Un cambio nuevo borra lo que se podía rehacer
        app.deshacer();
        app.aplicar(vec![Accion::Ci(dec!(10))]);
        assert!(app.historial.rehacer.is_empty());
        // Escribir un resumen letra a letra se deshace de una vez
        let antes = app.p.conceptos["SR.M2"].resumen.clone();
        for t in ["S", "Su", "Sue"] {
            app.aplicar(vec![Accion::Resumen("SR.M2".into(), t.into())]);
        }
        app.deshacer();
        assert_eq!(app.p.conceptos["SR.M2"].resumen, antes);
        // Un error no deja copia en el historial
        let n = app.historial.deshacer.len();
        app.aplicar(vec![Accion::AnadirLinea("C01".into(), "SR.M2".into(), dec!(1))]);
        assert_eq!(app.historial.deshacer.len(), n);
        // Abrir otro presupuesto empieza un historial nuevo
        app.nuevo_presupuesto();
        assert!(app.historial.deshacer.is_empty());
    }

    #[test]
    fn subcontrata_a_ratio_desde_la_interfaz() {
        // SR.M2 entera (salvo %MA) a 38 €/m² → 38,76; SR.COL8 sin tocar 242,75 × 3
        // PEM = 210 × 38,76 + 728,25 = 8.139,60 + 728,25 = 8.867,85
        let mut app = Aplicacion::sin_ventana();
        app.pestana = Pestana::Subcontratar;
        let todos: Vec<String> = app.p.conceptos["SR.M2"]
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .filter(|h| h != "%MA")
            .collect();
        let paq = ppto_core::ratio::PaqueteRatio {
            codigo: "SUB.SR".into(),
            descripcion: "Suelo radiante".into(),
            unidad: "m2".into(),
            precio: dec!(38),
            medidor: vec!["SR.M2".into()],
            asignaciones: std::collections::BTreeMap::from([("SR.M2".into(), todos)]),
        };
        app.aplicar(vec![Accion::AplicarRatio(paq)]);
        assert_eq!(app.calc.pem, dec!(8867.85));
        app.deshacer();
        assert_eq!(app.calc.pem, dec!(7005.15));
        // La pestaña se pinta con el formulario vacío
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_subcontratar(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
    }
    #[test]
    fn linea_de_tarifa_de_subcontrata() {
        // Tarifa sintética: detector 11,50 €/ud
        let mut app = Aplicacion::sin_ventana();
        app.tarifas.anadir(ppto_tarifas::Tarifa {
            proveedor: "Subco".into(),
            vigencia: "2031".into(),
            especialidad: "PCI".into(),
            notas: String::new(),
            lineas: vec![ppto_tarifas::LineaTarifa {
                referencia: "X010".into(),
                descripcion: "INSTALACION DETECTOR".into(),
                unidad: "ud".into(),
                precio: dec!(11.5),
                revisar_unidad: false,
            }],
        });
        let antes = app.calc.precios["SR.M2"];
        app.aplicar(vec![Accion::Alta(TipoAlta::Linea { padre: "SR.M2".into() })]);
        let e = app.tarifas.buscar("detector")[0];
        if let Some(a) = app.alta.as_mut() {
            a.de_tarifa = true;
            a.buscar = "detector".into();
            a.tarifa_elegida = Some(e);
        }
        pintar(&mut app);
        assert!(
            app.alta.is_some(),
            "la ventana sigue abierta hasta pulsar «Añadir línea»"
        );
        let acc = app.accion_tarifa("SR.M2", e, dec!(0.1)).unwrap();
        assert!(matches!(
            acc,
            Accion::NuevoRecurso {
                naturaleza: Naturaleza::Subcontrata,
                ..
            }
        ));
        app.aplicar(vec![acc]);
        let c = app.p.concepto("SUBCO.X010").unwrap();
        assert_eq!((c.naturaleza, c.precio), (Naturaleza::Subcontrata, dec!(11.5)));
        assert!(app.calc.precios["SR.M2"] > antes);
        // Ya existe: la segunda vez solo añade la línea (en otra partida)
        assert!(matches!(
            app.accion_tarifa("SR.COL8", e, dec!(1)),
            Some(Accion::AnadirLinea(..))
        ));
    }
    #[test]
    fn coeficiente_de_dificultad() {
        let mut app = Aplicacion::sin_ventana();
        let pem = app.calc.pem;
        app.aplicar(vec![Accion::Dificultad {
            ambito: "OBRA".into(),
            coeficiente: dec!(1.2),
            motivo: "obra en altura".into(),
        }]);
        assert!(app.calc.pem > pem);
        let mo = app.p.conceptos["SR.M2"]
            .descomposicion
            .iter()
            .find(|l| l.hijo == "MO.OF1F")
            .unwrap()
            .rendimiento;
        assert_eq!(mo, dec!(0.3), "0,25 × 1,20");
        assert_eq!(app.notas_revision, ["dificultad ×1,20 en OBRA (obra en altura)"]);
        app.deshacer();
        assert_eq!(app.calc.pem, pem);
        assert!(app.notas_revision.is_empty());
        app.rehacer();
        assert_eq!(app.notas_revision.len(), 1);
        // La pestaña Recursos se pinta con el formulario
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_recursos(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
    }
    #[test]
    fn propuestas_de_la_ia() {
        use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta as E, Propuesta};
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P.DIR".into(),
            unidad: "ud".into(),
            resumen: "Partida".into(),
        }]);
        let obra = ppto_propuestas::clave(&app.p);
        for (partida, precio, conf) in [
            ("P.DIR", dec!(57.30), Confianza::Alta),
            ("SR.M2", dec!(5), Confianza::Baja),
        ] {
            app.enlace.bandeja.anadir(
                &obra,
                Propuesta {
                    id: 0,
                    partida: partida.into(),
                    contenido: Contenido::PrecioDirecto(precio),
                    confianza: conf,
                    justificacion: "prueba".into(),
                    autor: "IA".into(),
                    estado: E::Pendiente,
                    descuentos: Default::default(),
                    nueva: None,
                },
            );
        }
        assert_eq!(app.propuestas_pendientes().len(), 2);
        // La ventana se pinta (una de las dos no se puede aplicar: SR.M2 tiene descompuesto)
        app.enlace.ventana = true;
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                let c = ui.ctx().clone();
                app.ventana_propuestas(&c, &mut acc);
                assert!(acc.is_empty());
            });
            out.textures_delta.clear();
        }
        let pem = app.calc.pem;
        app.aplicar(vec![Accion::AceptarPropuesta(1)]);
        assert_eq!(app.calc.precios["P.DIR"], dec!(57.30));
        assert_eq!(app.enlace.bandeja.buscar(1).unwrap().estado, E::Aceptada);
        assert!(app.calc.pem > pem);
        // Deshacer: vuelve a pendiente; rehacer: aceptada
        app.deshacer();
        assert_eq!(app.calc.pem, pem);
        assert_eq!(app.enlace.bandeja.buscar(1).unwrap().estado, E::Pendiente);
        app.rehacer();
        assert_eq!(app.enlace.bandeja.buscar(1).unwrap().estado, E::Aceptada);
        // La imposible da error sin cambiar nada; se puede rechazar
        let p = app.p.clone();
        app.aplicar(vec![Accion::AceptarPropuesta(2)]);
        assert_eq!(app.p, p);
        app.aplicar(vec![Accion::RechazarPropuesta(2)]);
        assert!(app.propuestas_pendientes().is_empty());
    }
    #[test]
    fn descuentos_por_fabricante_desde_recursos() {
        let mut app = Aplicacion::sin_ventana();
        app.p.fijar_tarifa("MT.PANEL", "acme", dec!(20)).unwrap();
        app.recalcular();
        app.aplicar(vec![Accion::Descuento("Acme".into(), dec!(50))]);
        assert_eq!(app.p.concepto("MT.PANEL").unwrap().precio, dec!(10));
        app.aplicar(vec![Accion::Pvp("MT.PANEL".into(), dec!(30))]);
        assert_eq!(app.p.concepto("MT.PANEL").unwrap().precio, dec!(15));
        app.deshacer();
        assert_eq!(app.p.concepto("MT.PANEL").unwrap().precio, dec!(10));
        // La pestaña se pinta con la tabla de descuentos
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_recursos(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
    }
    /// Coste y venta partida a partida: el desglose de la partida y la
    /// tabla de «Resumen y venta» se pintan y la venta sale de PEM + GG + BI.
    #[test]
    fn coste_y_venta_partida_a_partida() {
        let mut app = Aplicacion::sin_ventana();
        app.gg = dec!(13);
        app.bi = dec!(6);
        let (cap, hijo) = app.sel.clone().unwrap();
        let l = app.calc.lineas[&cap].iter().find(|l| l.hijo == hijo).unwrap().clone();
        let v = venta::venta_partida(l.precio, app.gg, app.bi, 2);
        assert_eq!(v.venta, l.precio + v.gastos_generales + v.beneficio_industrial);
        assert!(v.venta > l.precio);
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    app.pestana_partida(ui, &mut acc);
                    app.pestana_resumen(ui, &mut acc);
                });
            });
            out.textures_delta.clear();
        }
        // Margen propio: la partida pasa a PEM × 1,30 y se puede deshacer
        app.aplicar(vec![Accion::Margen(hijo.clone(), Some(dec!(30)))]);
        let m = app.p.concepto(&hijo).unwrap().margen;
        assert_eq!(m, Some(dec!(30)));
        let v = venta::venta_con_margen(l.precio, m, app.gg, app.bi, 2);
        assert_eq!(v.venta, l.precio + ppto_core::redondear(l.precio * dec!(0.30), 2));
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    app.pestana_partida(ui, &mut acc);
                    app.pestana_resumen(ui, &mut acc);
                });
            });
            out.textures_delta.clear();
        }
        // Fancoil: coste 599,05 → venta 712,87: sobrecoste 19 %, margen 15,97 %
        let (sc, mv) = sobrecoste_y_margen(dec!(599.05), dec!(712.87));
        assert_eq!(sc.unwrap().round_dp(2), dec!(19.00));
        assert_eq!(mv.unwrap().round_dp(2), dec!(15.97));
        assert_eq!(sobrecoste_y_margen(dec!(0), dec!(0)), (None, None));
        app.deshacer();
        assert_eq!(app.p.concepto(&hijo).unwrap().margen, None);
        // Venta bloqueada: prevalece sobre el margen y se pinta
        app.aplicar(vec![
            Accion::Margen(hijo.clone(), Some(dec!(30))),
            Accion::VentaFija(hijo.clone(), Some(dec!(1))),
        ]);
        let c = app.p.concepto(&hijo).unwrap().clone();
        let b = venta::venta_concepto(l.precio, &c, app.gg, app.bi, 2);
        assert!(b.bloqueada && b.venta == dec!(1));
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let mut acc = Vec::new();
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    app.pestana_partida(ui, &mut acc);
                    app.pestana_resumen(ui, &mut acc);
                });
            });
            out.textures_delta.clear();
        }
        app.aplicar(vec![
            Accion::VentaFija(hijo.clone(), None),
            Accion::Margen(hijo.clone(), None),
        ]);
        assert_eq!(app.p.concepto(&hijo).unwrap().venta_fija, None);
        // Solo partidas
        app.aplicar(vec![Accion::Margen(cap.clone(), Some(dec!(10)))]);
        assert_eq!(app.p.concepto(&cap).unwrap().margen, None);
    }

    #[test]
    fn reaplicar_propuestas_aceptadas_tras_reabrir() {
        use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta as E, LineaPropuesta, Propuesta};
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P.X".into(),
            unidad: "ud".into(),
            resumen: "Partida".into(),
        }]);
        let vacio = app.p.clone();
        let obra = ppto_propuestas::clave(&app.p);
        app.enlace.bandeja.anadir(
            &obra,
            Propuesta {
                id: 0,
                partida: "P.X".into(),
                contenido: Contenido::Descompuesto(vec![LineaPropuesta {
                    codigo: "MO.OF1F".into(),
                    rendimiento: dec!(2),
                    naturaleza: None,
                    unidad: String::new(),
                    resumen: String::new(),
                    precio: None,
                    pvp: None,
                    fabricante: None,
                    grupo: None,
                }]),
                confianza: Confianza::Alta,
                justificacion: "x".into(),
                autor: "IA".into(),
                estado: E::Pendiente,
                descuentos: Default::default(),
                nueva: None,
            },
        );
        app.aplicar(vec![Accion::AceptarPropuesta(1)]);
        assert_eq!(app.calc.precios["P.X"], dec!(49));
        assert!(app.reaplicables().is_empty(), "ya tiene precio");
        // Se vuelve a abrir la obra vacía (como al importar otra vez el ciego)
        app.cargar_presupuesto(vacio);
        assert_eq!(app.reaplicables().len(), 1);
        app.aplicar(vec![Accion::ReaplicarAceptadas]);
        assert_eq!(app.calc.precios["P.X"], dec!(49));
        assert!(app.reaplicables().is_empty());
        app.deshacer();
        assert_eq!(app.reaplicables().len(), 1, "se deshace en un paso");
    }

    #[test]
    fn panel_del_asistente_en_modo_encargo() {
        use ppto_propuestas::conversacion::Autor;
        let mut app = Aplicacion::sin_ventana();
        app.asistente.abierto = true;
        app.asistente.entrada = "Valora la climatización con la oferta".into();
        app.asistente.adjuntos = vec!["C:/ofertas/koolair.pdf".into()];
        let p = app.p.clone();
        app.asistente.enviar(&p).unwrap();
        assert!(app.asistente.entrada.is_empty() && app.asistente.adjuntos.is_empty());
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| app.panel_asistente(ui));
            out.textures_delta.clear();
        }
        let obra = ppto_propuestas::clave(&app.p);
        let m = app.asistente.mensajes(&obra);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].autor, Autor::Usuario);
        assert!(!m[0].atendido);
        assert_eq!(m[0].adjuntos, ["C:/ofertas/koolair.pdf"]);
        // El presupuesto no cambia
        assert_eq!(app.p, p);
    }

    #[test]
    fn panel_del_asistente_con_varias_ias() {
        let mut app = Aplicacion::sin_ventana();
        app.asistente.abierto = true;
        let a = app.asistente.con_dos_conexiones();
        assert_eq!(a.conexion, "ChatGPT");
        assert!(a.usar_conexion("Claude"));
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| app.panel_asistente(ui));
            out.textures_delta.clear();
        }
        let a = app.asistente.con_dos_conexiones();
        assert_eq!(a.conexiones.len(), 2);
    }

    #[test]
    fn ventana_de_biblioteca_guarda_la_obra() {
        let dir = std::env::temp_dir().join(format!("gui-bib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("biblioteca.json");
        // Biblioteca con una tarifa antigua (aviso en naranja)
        ppto_biblioteca::Biblioteca::modificar(&ruta, |b| {
            b.cargar_tarifa("KOOLAIR", "2020", 2020, Default::default());
            b.fijar_descuento("KOOLAIR", Some(dec!(45)));
        })
        .unwrap();
        let mut app = Aplicacion::sin_ventana();
        app.ventana_biblioteca = Some(biblioteca::VentanaBiblioteca::en(Some(ruta.clone())));
        let ctx = Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let c = ui.ctx().clone();
                app.ventana_biblioteca(&c);
            });
            out.textures_delta.clear();
        }
        assert!(app.ventana_biblioteca.is_some());
        // Guardar la obra (lo mismo que el botón)
        let p = app.p.clone();
        ppto_biblioteca::Biblioteca::modificar(&ruta, |b| {
            b.guardar_obra(&p, "Ejemplo", ppto_biblioteca::Origen::Real, None)
        })
        .unwrap();
        assert_eq!(ppto_biblioteca::Biblioteca::cargar(&ruta).partidas.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bases_de_precios_copiar_mano_de_obra() {
        // Base sintética (no es una base real): 2 manos de obra, 1 material y una partida
        let dir = std::env::temp_dir().join(format!("gui-bases-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let origen = dir.join("base_sintetica.bc3");
        std::fs::write(
            &origen,
            "~V|PRUEBA|FIEBDC-3/2002|prueba||ANSI|\r\n\
             ~K|\\2\\2\\3\\2\\2\\2\\2\\EUR\\|0|\r\n\
             ~C|BASE##||Base sintetica|0||0|\r\n\
             ~C|CAP#||Capitulo|0||0|\r\n\
             ~C|CRP|UD|COLOCACION REJILLA INTERIOR HASTA 0,5 M2|32.5|010126|1|\r\n\
             ~C|CDC|UD|COLOCACION DIFUSOR CIRCULAR|22.1|010126|1|\r\n\
             ~C|REJ|UD|REJILLA DE PRUEBA|40|010126|3|\r\n\
             ~C|P.REJ|UD|REJILLA COLOCADA|72.5|010126|0|\r\n\
             ~D|BASE##|CAP#\\1\\1\\|\r\n\
             ~D|CAP#|P.REJ\\1\\1\\|\r\n\
             ~D|P.REJ|REJ\\1\\1\\CRP\\1\\1\\|\r\n",
        )
        .unwrap();
        let mut v = bases::VentanaBases::en(Some(dir.join("bases")));
        assert!(v.base.is_none());
        v.anadir(&origen);
        assert!(
            dir.join("bases").join("base_sintetica.bc3").exists(),
            "se guarda una copia"
        );
        assert_eq!(v.base.as_ref().unwrap().filas.len(), 4);
        // Solo mano de obra, buscando «rejilla»
        v.filtro = Some(Naturaleza::ManoObra);
        assert_eq!(v.visibles().len(), 2);
        v.buscar = "rejilla".into();
        let vis: Vec<String> = v.visibles().iter().map(|f| f.codigo.clone()).collect();
        assert_eq!(vis, ["CRP"]);
        // La ventana se dibuja
        let mut app = Aplicacion::sin_ventana();
        let origen_p = std::sync::Arc::clone(&v.base.as_ref().unwrap().p);
        app.ventana_bases = Some(v);
        let ctx = Context::default();
        for _ in 0..2 {
            let mut acc = Vec::new();
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let c = ui.ctx().clone();
                app.ventana_bases(&c, &mut acc);
            });
            out.textures_delta.clear();
        }
        assert!(app.ventana_bases.is_some());
        // Copiar y añadir CRP a SR.M2: entra al final, detrás del 2 % de medios
        // auxiliares (que solo afecta a lo anterior), 29,89 + 32,50 = 62,39 €/m2
        app.aplicar(vec![Accion::CopiarDeBase {
            origen: std::sync::Arc::clone(&origen_p),
            nombre: "base_sintetica.bc3".into(),
            codigos: vec!["CRP".into()],
            partida: Some("SR.M2".into()),
        }]);
        assert_eq!(app.p.precio("CRP").unwrap(), dec!(32.50));
        assert!(app.p.conceptos["SR.M2"].descomposicion.iter().any(|l| l.hijo == "CRP"));
        assert_eq!(app.p.precio("SR.M2").unwrap(), dec!(62.39));
        // Deshacer lo quita todo
        app.deshacer();
        assert!(!app.p.conceptos.contains_key("CRP"));
        // Copiar una partida sin colocarla: trae su descompuesto y el PEM no cambia
        let pem = app.p.pem().unwrap();
        app.aplicar(vec![Accion::CopiarDeBase {
            origen: origen_p,
            nombre: "base_sintetica.bc3".into(),
            codigos: vec!["P.REJ".into()],
            partida: None,
        }]);
        assert_eq!(app.p.precio("P.REJ").unwrap(), dec!(72.50));
        assert_eq!(app.p.pem().unwrap(), pem);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn la_barra_de_menus_usa_simbolos_que_existen_en_la_fuente() {
        // Las flechas de deshacer/rehacer deben estar en la fuente de egui
        // (↩ y ↪ no están y saldrían como un cuadro).
        let ctx = Context::default();
        let mut out = ctx.run_ui(RawInput::default(), |_| {});
        out.textures_delta.clear();
        for c in ['⟲', '⟳', '⏵', '⏷', '⏶'] {
            assert!(
                ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::default(), c)),
                "falta el símbolo {c}"
            );
        }
    }

    #[test]
    fn resumen_de_un_recurso_creado_sin_texto() {
        // Recurso creado sin resumen ni precio (como R0001): se le pone texto
        // y unidad después, con deshacer.
        let mut app = Aplicacion::sin_ventana();
        app.aplicar(vec![Accion::NuevoRecurso {
            codigo: "R0001".into(),
            unidad: String::new(),
            resumen: String::new(),
            naturaleza: Naturaleza::ManoObra,
            precio: dec!(0),
            en: "SR.M2".into(),
            cantidad: dec!(1),
        }]);
        app.sel = Some(("C01".into(), "SR.M2".into()));
        // La tabla del descompuesto y la de recursos se dibujan con campos editables
        let ctx = Context::default();
        for pestana in [Pestana::Partida, Pestana::Recursos] {
            app.pestana = pestana;
            let mut acc = Vec::new();
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if app.pestana == Pestana::Partida {
                        app.pestana_partida(ui, &mut acc);
                    } else {
                        app.pestana_recursos(ui, &mut acc);
                    }
                });
            });
            out.textures_delta.clear();
        }
        app.aplicar(vec![
            Accion::Resumen("R0001".into(), "Oficial 1ª calefactor".into()),
            Accion::Unidad("R0001".into(), "h".into()),
        ]);
        assert_eq!(app.p.conceptos["R0001"].resumen, "Oficial 1ª calefactor");
        assert_eq!(app.p.conceptos["R0001"].unidad, "h");
    }

    #[test]
    fn vistas_presupuesto_y_precios() {
        let mut app = Aplicacion::sin_ventana();
        // Precios y márgenes: SR.M2 cuesta 29,89 y se vende a 35,57 (GG 13 % + BI 6 %);
        // margen sobre venta (35,57 − 29,89) / 35,57 = 15,97 %
        let f = app.filas_precios();
        assert_eq!(f.len(), 2);
        let m2 = f.iter().find(|x| x.codigo == "SR.M2").unwrap();
        assert_eq!(
            (m2.coste, m2.venta, m2.margen),
            (dec!(29.89), dec!(35.57), Some(dec!(15.97)))
        );
        // 210 m² × 35,57 = 7.469,70
        assert_eq!(m2.importe_venta, dec!(7469.70));
        // Ordenar por importe (de mayor a menor) y filtrar por texto
        app.filtro_precios.orden = vistas::Orden::Importe;
        app.filtro_precios.descendente = true;
        assert_eq!(app.filas_precios_filtradas()[0].codigo, "SR.M2");
        app.filtro_precios.texto = "colector".into();
        let v = app.filas_precios_filtradas();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].codigo, "SR.COL8");
        app.filtro_precios.texto.clear();
        // Margen por debajo del 10 %: ninguna (las dos rondan el 16 %)
        app.filtro_precios.margen_bajo = Some(dec!(10));
        assert!(app.filas_precios_filtradas().is_empty());
        app.filtro_precios.margen_bajo = None;
        // Sobrecoste propio del 30 % en SR.COL8 desde la vista: 242,75 × 1,30 = 315,58
        app.aplicar(vec![Accion::Margen("SR.COL8".into(), Some(dec!(30)))]);
        let c = app.filas_precios().into_iter().find(|x| x.codigo == "SR.COL8").unwrap();
        assert_eq!(c.venta, dec!(315.58));
        // Las dos vistas se dibujan; plegar un capítulo en «Presupuesto»
        app.plegados.insert("C01".into());
        let ctx = Context::default();
        for p in [Pestana::Presupuesto, Pestana::Precios] {
            app.pestana = p;
            let mut acc = Vec::new();
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if p == Pestana::Presupuesto {
                        app.pestana_presupuesto(ui, &mut acc);
                    } else {
                        app.pestana_precios(ui, &mut acc);
                    }
                });
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn vista_de_mediciones() {
        let mut app = Aplicacion::sin_ventana();
        let ctx = Context::default();
        app.pestana = Pestana::Mediciones;
        for filtro in ["", "planta primera"] {
            app.filtro_mediciones = filtro.into();
            let mut acc = Vec::new();
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.pestana_mediciones(ui, &mut acc));
            });
            out.textures_delta.clear();
            assert!(acc.is_empty(), "dibujar no cambia nada");
        }
        // Lo que hace la celda «Uds» al cambiar Planta baja de 2 a 4 colectores:
        // 5 ud × 242,75 = 1.213,75 (antes 728,25) → PEM 7.005,15 + 485,50 = 7.490,65
        let mut m = app.p.mediciones[&("C01".to_owned(), "SR.COL8".to_owned())].clone();
        m.lineas[0].unidades = Some(dec!(4));
        app.aplicar(vec![Accion::Medicion("C01".into(), "SR.COL8".into(), m)]);
        assert_eq!(app.p.pem().unwrap(), dec!(7490.65));
    }

    #[test]
    fn recursos_horas_por_oficio_y_por_fabricante() {
        let mut app = Aplicacion::sin_ventana();
        // Oficial: 210 m² × 0,25 h + 3 ud × 1,5 h = 57 h; ayudante: 52,5 + 3 = 55,5 h
        let h = app.horas_por_oficio();
        let of = h.iter().find(|x| x.0 == "MO.OF1F").unwrap();
        let ay = h.iter().find(|x| x.0 == "MO.AYUF").unwrap();
        assert_eq!((of.2, ay.2), (dec!(57.00), dec!(55.50)));
        // Colector de tarifa: PVP 300 con 40 % → neto 180; 3 ud → 900 a PVP, 540 neto
        app.p.conceptos.get_mut("MT.COL8").unwrap().tarifa = Some(ppto_core::PrecioTarifa {
            fabricante: "COLEC".into(),
            grupo: None,
            pvp: dec!(300),
        });
        app.p.fijar_descuento("COLEC", dec!(40)).unwrap();
        app.recalcular();
        let f = app.por_fabricante();
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].referencias, f[0].pvp, f[0].neto), (1, dec!(900.00), dec!(540.00)));
        // La vista se dibuja con los dos resúmenes
        let ctx = Context::default();
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.pestana_recursos(ui, &mut acc));
        });
        out.textures_delta.clear();
    }

    #[test]
    fn la_barra_de_menus_se_dibuja() {
        let mut app = Aplicacion::sin_ventana();
        let ctx = Context::default();
        for _ in 0..2 {
            let mut acc = Vec::new();
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                egui::Panel::top("barra").show(ui, |ui| app.barra_superior(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
    }
}
