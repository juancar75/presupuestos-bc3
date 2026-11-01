// SPDX-License-Identifier: GPL-3.0-or-later
//! Ventana principal: árbol de capítulos, partida (descompuesto y mediciones),
//! recursos, subcontratación y resumen.

mod acciones;
mod actualizar;
mod alta;
mod arbol;
mod archivo;
mod asistente;
mod bases;
mod biblioteca;
mod ciego;
mod comparar;
mod dificultad;
mod informes;
mod legalizacion;
mod menus;
mod modelo;
mod obra;
mod ofertas;
mod partida;
mod pdf;
mod pestanas;
mod propuestas;
mod ratio;
mod recuperacion;
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
    /// Origen del precio de un recurso (oferta y documento); `true`: poner
    /// el neto ofertado como precio.
    Origen(String, Option<Box<ppto_core::OrigenPrecio>>, bool),
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
    /// Pegar en un capítulo lo copiado en otra ventana (u obra) con Ctrl+C.
    PegarFragmento(Box<ppto_core::fragmento::Fragmento>, String),
    /// Capítulo donde pegará Ctrl+V.
    CapituloActivo(String),
    /// Cortar o copiar (origen, hijo, `true` = cortar); `None` = vaciar.
    Portapapeles(Option<(String, String, bool)>),
    Alta(TipoAlta),
    /// Alta encima de una línea (F5 o «Insertar aquí»).
    Insertar(TipoAlta, String),
    /// Coloca lo recién creado encima de otra línea; va con el alta en el
    /// mismo «Deshacer».
    Colocar(String, String, String),
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
    /// Contradictorios y certificaciones.
    Obra(obra::OpObra),
}

/// Partida sin valorar (árbol).
const COLOR_SIN_VALORAR: Color32 = Color32::from_rgb(225, 70, 60);
/// Partida con el descompuesto a medias (árbol).
const COLOR_INCOMPLETA: Color32 = Color32::from_rgb(235, 150, 40);

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
    /// Qué falta por valorar (rojo / naranja en el árbol).
    estados: HashMap<String, ppto_core::valoracion::Estado>,
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
            c.estados = ppto_core::valoracion::estados(p);
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
    /// Retención de garantía de las certificaciones (puntos).
    retencion: Decimal,
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
    /// Texto para el portapapeles del sistema (se copia al final del fotograma).
    copiar_al_sistema: Option<String>,
    /// Capítulo donde pega Ctrl+V (el último elegido en el árbol).
    capitulo_activo: Option<String>,
    historial: Historial,
    ratio: ratio::FormRatio,
    /// Tarifas de subcontratas del usuario (fuera del repositorio).
    tarifas: ppto_tarifas::Biblioteca,
    ventana_tarifas: Option<tarifas::VentanaTarifas>,
    ventana_biblioteca: Option<biblioteca::VentanaBiblioteca>,
    ventana_bases: Option<bases::VentanaBases>,
    /// Formulario «Origen del precio» de un recurso.
    form_origen: Option<ofertas::FormOrigen>,
    /// Dónde abre renglón F5 (lo que está bajo el ratón en este fotograma).
    f5: std::cell::RefCell<Option<Accion>>,
    /// Guardado automático (copia de recuperación).
    recuperacion: recuperacion::Recuperacion,
    /// Ventana de informes (pedidos, mano de obra, coste de obra).
    ventana_informes: Option<informes::VentanaInformes>,
    /// Contradictorios y certificaciones.
    ventana_obra: Option<obra::VentanaObra>,
    /// Pedido de Informes por registrar en la obra (se pide desde `&self`).
    registrar: std::cell::RefCell<Option<(ppto_core::pedidos::Pedido, String, String)>>,
    /// Importar un modelo BIM (IFC o Revit).
    ventana_modelo: Option<modelo::VentanaModelo>,
    /// Aviso de versión nueva.
    actualizacion: actualizar::Actualizacion,
    /// Ventana Obra → Contratar.
    ver_contratar: bool,
    fecha_contrato: String,
    /// Resumen: «cerrar en» (texto del importe objetivo).
    cerrar_en: String,
    /// Aviso «obra abierta» propio y momento del último latido.
    aviso_abierta: Option<(ppto_db::bloqueo::Aviso, f64)>,
    /// Obra que otro tiene abierta: preguntar antes de abrirla.
    confirmar_abrir: Option<(PathBuf, ppto_db::bloqueo::Ocupante)>,
    /// Si los documentos de las ofertas siguen donde se vincularon.
    docs: ofertas::CacheDocumentos,
    ventana_comparar: Option<comparar::VentanaComparar>,
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
        Accion::Origen(c, ..) => (format!("origen del precio de {c}"), None),
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
        Accion::PegarFragmento(f, d) => (format!("pegar de «{}» en {d}", f.obra), None),
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
        Accion::Obra(op) => op.describir(),
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
        app.recuperacion = recuperacion::Recuperacion::en_disco();
        app.actualizacion = actualizar::Actualizacion::al_arrancar();
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
            retencion: Decimal::ZERO,
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
            copiar_al_sistema: None,
            capitulo_activo: None,
            historial: Historial::default(),
            ratio: ratio::FormRatio::default(),
            tarifas: ppto_tarifas::Biblioteca::default(),
            ventana_tarifas: None,
            ventana_biblioteca: None,
            ventana_bases: None,
            form_origen: None,
            ventana_informes: None,
            ventana_obra: None,
            registrar: Default::default(),
            ventana_modelo: None,
            actualizacion: Default::default(),
            ver_contratar: false,
            fecha_contrato: ppto_pdf::fecha_hoy(),
            cerrar_en: String::new(),
            aviso_abierta: None,
            confirmar_abrir: None,
            f5: std::cell::RefCell::new(None),
            recuperacion: recuperacion::Recuperacion::default(),
            docs: ofertas::CacheDocumentos::default(),
            ventana_comparar: None,
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
        if let Some(v) = &mut self.ventana_obra {
            v.sucia = true;
        }
        self.enlace.sucia = true;
    }

    fn marcar_cambio(&mut self) {
        self.cambios = true;
        self.recalcular();
    }

    /// Ctrl+C copia la partida elegida (también al portapapeles del sistema) y
    /// Ctrl+V pega lo copiado, aunque venga de otra ventana (otra obra).
    fn copiar_pegar_teclado(&mut self, ctx: &egui::Context, acciones: &mut Vec<Accion>) {
        if !ctx.egui_wants_keyboard_input() {
            let (copiar, pegado) = ctx.input(|i| {
                let mut copiar = false;
                let mut pegado = None;
                for e in &i.events {
                    match e {
                        egui::Event::Copy => copiar = true,
                        egui::Event::Paste(t) => pegado = Some(t.clone()),
                        _ => {}
                    }
                }
                (copiar, pegado)
            });
            if copiar && let Some((p, h)) = self.sel.clone() {
                acciones.push(Accion::Portapapeles(Some((p, h, false))));
            }
            if let Some(t) = pegado {
                match ppto_core::fragmento::Fragmento::de_texto(&t) {
                    Some(f) => {
                        let destino = self
                            .capitulo_activo
                            .clone()
                            .filter(|c| self.p.conceptos.contains_key(c))
                            .unwrap_or_else(|| self.p.raiz.clone());
                        acciones.push(Accion::PegarFragmento(Box::new(f), destino));
                    }
                    None => self.info("En el portapapeles no hay partidas copiadas de este programa."),
                }
            }
        }
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

    // ---------------------------------------------------------------- acciones

    // --------------------------------------------------------------------- UI
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
        self.pantalla(ui);
    }
}

impl Aplicacion {
    /// Toda la ventana (sin depender de eframe: lo usan las pruebas).
    pub(super) fn pantalla(&mut self, ui: &mut Ui) {
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
        // Ctrl+C / Ctrl+V fuera de los campos de texto (también entre ventanas)
        self.copiar_pegar_teclado(&ui.ctx().clone(), &mut acciones);
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
        self.tecla_f5(ui.ctx(), &mut acciones);
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
                    ui.label(RichText::new(e.to_string()).color(Color32::from_rgb(220, 80, 60)));
                }
                (None, Some((m, true))) => {
                    ui.label(RichText::new(m.to_string()).color(Color32::from_rgb(220, 80, 60)));
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
        self.ventana_origen(&ctx, &mut acciones);
        self.ventana_informes(&ctx);
        self.ventana_obra(&ctx);
        self.ventana_modelo(&ctx);
        self.ventana_actualizar(&ctx);
        self.ventana_contratar(&ctx);
        self.aviso_obra_abierta(&ctx);
        self.ventana_confirmar_abrir(&ctx);
        self.ventana_comparar(&ctx, &mut acciones);
        self.ventana_legalizacion(&ctx);
        self.ventana_propuestas(&ctx, &mut acciones);
        self.ventana_recuperacion(&ctx);
        if !acciones.is_empty() {
            self.aplicar(acciones);
        }
        self.autoguardar(&ctx);
        if let Some(t) = self.copiar_al_sistema.take() {
            ui.ctx().copy_text(t);
        }
    }
}

/// Lo que se arrastra en el árbol: una línea (`hijo`) de un capítulo (`origen`).
/// Versión del programa (la de Cargo.toml).
pub(super) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Memoria temporal: rectángulo del campo «Título» del menú del capítulo.
const CAMPO_TITULO: &str = "campo_titulo_capitulo";

#[derive(Clone)]
struct Arrastre {
    origen: String,
    hijo: String,
}

/// Opciones comunes a cualquier línea del árbol.
fn menu_linea(ui: &mut Ui, padre: &str, hijo: &str, es_capitulo: bool, acciones: &mut Vec<Accion>) {
    if !es_capitulo {
        if ui
            .button("Insertar partida aquí   F5")
            .on_hover_text("Partida nueva encima de esta")
            .clicked()
        {
            acciones.push(Accion::Insertar(
                TipoAlta::Partida {
                    capitulo: padre.to_owned(),
                },
                hijo.to_owned(),
            ));
        }
        ui.separator();
    }
    if ui
        .button("Cortar")
        .on_hover_text("Para moverla: luego clic derecho en el capítulo de destino → «Pegar aquí»")
        .clicked()
    {
        acciones.push(Accion::Portapapeles(Some((padre.to_owned(), hijo.to_owned(), true))));
    }
    if ui
        .button("Copiar")
        .on_hover_text(if es_capitulo {
            "Para pegarlo con Ctrl+V en otra obra (otra ventana del programa)"
        } else {
            "Para ponerla también en otro capítulo, o con Ctrl+V en otra obra (otra ventana)"
        })
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
mod pruebas;
